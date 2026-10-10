//! Local shell sessions backed by a pseudo-terminal.
//!
//! [`PtySession`] implements [`quay_core::Session`]. Output is read on a
//! dedicated thread and coalesced into batches (~8 ms) before being handed to
//! the caller, so a busy program cannot flood the UI with tiny events.

use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use quay_core::{Error, Result, Session, TermSize};

/// How long output is held back to coalesce small reads.
const BATCH_WINDOW: Duration = Duration::from_millis(8);
/// Flush immediately once a batch reaches this size.
const BATCH_MAX: usize = 64 * 1024;

/// How to start a local session.
#[derive(Debug, Clone)]
pub struct SpawnOptions {
    /// Program to run; `None` uses the user's default shell.
    pub program: Option<String>,
    pub args: Vec<String>,
    pub size: TermSize,
}

/// The user's default shell for this platform.
pub fn default_shell() -> String {
    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/bin/sh".to_string())
    }
}

fn pty_size(size: TermSize) -> PtySize {
    PtySize {
        rows: size.rows.max(1),
        cols: size.cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn other<E: std::fmt::Display>(e: E) -> Error {
    Error::Other(e.to_string())
}

pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    closed: bool,
}

impl PtySession {
    /// Start a session. `on_output` receives batched output chunks and
    /// `on_exit` is called once when the process ends. Both run on a
    /// background thread.
    pub fn spawn(
        opts: SpawnOptions,
        on_output: impl FnMut(Vec<u8>) + Send + 'static,
        on_exit: impl FnOnce() + Send + 'static,
    ) -> Result<Self> {
        let pair = native_pty_system()
            .openpty(pty_size(opts.size))
            .map_err(other)?;

        let mut cmd = CommandBuilder::new(opts.program.unwrap_or_else(default_shell));
        cmd.args(&opts.args);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            cmd.cwd(home);
        }

        let child = pair.slave.spawn_command(cmd).map_err(other)?;
        // The child holds its own handle; keeping ours open would stop EOF.
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().map_err(other)?;
        let writer = pair.master.take_writer().map_err(other)?;

        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        thread::spawn(move || {
            batch_output(rx, on_output);
            on_exit();
        });

        Ok(Self {
            master: pair.master,
            writer,
            child,
            closed: false,
        })
    }

    fn ensure_open(&self) -> Result<()> {
        if self.closed {
            Err(Error::SessionClosed)
        } else {
            Ok(())
        }
    }
}

/// Coalesce chunks arriving within [`BATCH_WINDOW`] into one callback.
/// Returns when the sender is dropped (reader hit EOF).
fn batch_output(rx: mpsc::Receiver<Vec<u8>>, mut emit: impl FnMut(Vec<u8>)) {
    while let Ok(mut batch) = rx.recv() {
        let deadline = Instant::now() + BATCH_WINDOW;
        while batch.len() < BATCH_MAX {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(more) => batch.extend_from_slice(&more),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    emit(batch);
                    return;
                }
            }
        }
        emit(batch);
    }
}

impl Session for PtySession {
    fn write(&mut self, data: &[u8]) -> Result<()> {
        self.ensure_open()?;
        self.writer.write_all(data)?;
        self.writer.flush()?;
        Ok(())
    }

    fn resize(&mut self, size: TermSize) -> Result<()> {
        self.ensure_open()?;
        self.master.resize(pty_size(size)).map_err(other)
    }

    fn close(&mut self) -> Result<()> {
        if !self.closed {
            self.closed = true;
            // Already-exited children make kill fail; that is fine.
            let _ = self.child.kill();
        }
        Ok(())
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn echo_opts() -> SpawnOptions {
        #[cfg(windows)]
        let (program, args) = ("cmd.exe", vec!["/C", "echo quay-hello"]);
        #[cfg(not(windows))]
        let (program, args) = ("sh", vec!["-c", "echo quay-hello"]);
        SpawnOptions {
            program: Some(program.to_string()),
            args: args.into_iter().map(String::from).collect(),
            size: TermSize { cols: 80, rows: 24 },
        }
    }

    #[test]
    fn captures_output_and_reports_exit() {
        let out = Arc::new(Mutex::new(Vec::new()));
        let (done_tx, done_rx) = mpsc::channel();
        let sink = out.clone();
        let _session = PtySession::spawn(
            echo_opts(),
            move |chunk| sink.lock().unwrap().extend(chunk),
            move || {
                let _ = done_tx.send(());
            },
        )
        .unwrap();
        done_rx.recv_timeout(Duration::from_secs(10)).expect("exit");
        let text = String::from_utf8_lossy(&out.lock().unwrap()).to_string();
        assert!(text.contains("quay-hello"), "got: {text:?}");
    }

    #[test]
    fn batches_coalesce_and_flush_on_disconnect() {
        let (tx, rx) = mpsc::channel();
        for part in [&b"ab"[..], b"cd", b"ef"] {
            tx.send(part.to_vec()).unwrap();
        }
        drop(tx);
        let mut batches = Vec::new();
        batch_output(rx, |b| batches.push(b));
        assert_eq!(batches, vec![b"abcdef".to_vec()]);
    }

    #[test]
    fn write_after_close_fails() {
        let mut s = PtySession::spawn(echo_opts(), |_| {}, || {}).unwrap();
        s.close().unwrap();
        assert!(matches!(s.write(b"x"), Err(Error::SessionClosed)));
    }
}
