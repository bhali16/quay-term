//! Local shell sessions backed by a pseudo-terminal.
//!
//! [`PtySession`] implements [`quay_core::Session`]. Output is read on a
//! dedicated thread and coalesced into batches (~8 ms) before being handed to
//! the caller, so a busy program cannot flood the UI with tiny events.

mod integration;
pub use integration::install as install_shell_integration;

use std::io::{Read, Write};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use quay_core::{Error, Result, Session, TermSize};

/// How long output is held back to coalesce small reads.
const BATCH_WINDOW: Duration = Duration::from_millis(8);
/// Flush immediately once a batch reaches this size.
const BATCH_MAX: usize = 64 * 1024;
/// How often the child process is polled for exit.
const EXIT_POLL: Duration = Duration::from_millis(50);
/// Time given to trailing output after the child exits.
const EXIT_GRACE: Duration = Duration::from_millis(150);

enum Msg {
    Data(Vec<u8>),
    /// The session ended (EOF on the PTY or the child exited).
    End,
}

/// How to start a local session.
#[derive(Debug, Clone)]
pub struct SpawnOptions {
    /// Program to run; `None` uses the user's default shell.
    pub program: Option<String>,
    pub args: Vec<String>,
    pub size: TermSize,
    /// Where the shell integration scripts live (see [`install_shell_integration`]).
    /// Applied to zsh and bash only; `None` starts the shell untouched.
    pub integration_dir: Option<std::path::PathBuf>,
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
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
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

        // The default shell starts as a login shell, like Terminal.app and iTerm, so the
        // user's profile (PATH, prompt, LS_COLORS) is loaded even when the app was
        // launched from the Dock with a minimal environment.
        let default = opts.program.is_none();
        let shell = opts.program.unwrap_or_else(default_shell);
        let mut cmd = CommandBuilder::new(&shell);
        #[cfg(unix)]
        if let Some(dir) = opts.integration_dir.as_deref() {
            integration::configure(&mut cmd, &shell, dir);
        }
        #[cfg(not(windows))]
        if default && opts.args.is_empty() {
            cmd.arg("-l");
        }
        let _ = default;
        cmd.args(&opts.args);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        // BSD/macOS `ls` only colors with this; harmless elsewhere.
        if std::env::var_os("CLICOLOR").is_none() {
            cmd.env("CLICOLOR", "1");
        }
        // Apps started from the Dock often lack a locale, which breaks UTF-8 text.
        if std::env::var_os("LANG").is_none() && std::env::var_os("LC_ALL").is_none() {
            cmd.env("LANG", "en_US.UTF-8");
        }
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            cmd.cwd(home);
        }

        let child = pair.slave.spawn_command(cmd).map_err(other)?;
        // The child holds its own handle; keeping ours open would stop EOF.
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().map_err(other)?;
        let writer = pair.master.take_writer().map_err(other)?;

        let child = Arc::new(Mutex::new(child));
        let (tx, rx) = mpsc::channel::<Msg>();
        let reader_tx = tx.clone();
        thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => {
                        let _ = reader_tx.send(Msg::End);
                        break;
                    }
                    Ok(n) => {
                        if reader_tx.send(Msg::Data(buf[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        // Windows ConPTY keeps the output pipe open after the child exits, so
        // the reader never sees EOF there. Watch the process directly too.
        let watched = child.clone();
        thread::spawn(move || {
            loop {
                let exited = !matches!(watched.lock().unwrap().try_wait(), Ok(None));
                if exited {
                    break;
                }
                thread::sleep(EXIT_POLL);
            }
            thread::sleep(EXIT_GRACE);
            let _ = tx.send(Msg::End);
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
fn batch_output(rx: mpsc::Receiver<Msg>, mut emit: impl FnMut(Vec<u8>)) {
    while let Ok(Msg::Data(mut batch)) = rx.recv() {
        let deadline = Instant::now() + BATCH_WINDOW;
        while batch.len() < BATCH_MAX {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(Msg::Data(more)) => batch.extend_from_slice(&more),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Ok(Msg::End) | Err(mpsc::RecvTimeoutError::Disconnected) => {
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
            let _ = self.child.lock().unwrap().kill();
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
            integration_dir: None,
        }
    }

    #[test]
    fn captures_output_and_reports_exit() {
        let out = Arc::new(Mutex::new(Vec::new()));
        let (done_tx, done_rx) = mpsc::channel();
        let sink = out.clone();
        #[cfg_attr(not(windows), allow(unused_mut))]
        let mut session = PtySession::spawn(
            echo_opts(),
            move |chunk| sink.lock().unwrap().extend(chunk),
            move || {
                let _ = done_tx.send(());
            },
        )
        .unwrap();
        // Windows ConPTY asks the terminal for the cursor position at startup
        // and stalls until it gets an answer. xterm.js answers in the app; the
        // test has to play the terminal.
        #[cfg(windows)]
        session.write(b"\x1b[1;1R").unwrap();
        if done_rx.recv_timeout(Duration::from_secs(10)).is_err() {
            let got = String::from_utf8_lossy(&out.lock().unwrap()).to_string();
            panic!("no exit within 10s; output so far: {got:?}");
        }
        drop(session);
        let text = String::from_utf8_lossy(&out.lock().unwrap()).to_string();
        assert!(text.contains("quay-hello"), "got: {text:?}");
    }

    #[test]
    fn batches_coalesce_and_flush_on_disconnect() {
        let (tx, rx) = mpsc::channel();
        for part in [&b"ab"[..], b"cd", b"ef"] {
            tx.send(Msg::Data(part.to_vec())).unwrap();
        }
        drop(tx);
        let mut batches = Vec::new();
        batch_output(rx, |b| batches.push(b));
        assert_eq!(batches, vec![b"abcdef".to_vec()]);
    }

    /// A real interactive shell, started the way the app starts it, must emit the command marks.
    #[cfg(unix)]
    fn marks_from(shell: &str, args: &[&str]) -> Option<Vec<u8>> {
        if !std::path::Path::new(shell).exists() {
            return None;
        }
        let dir = std::env::temp_dir().join(format!(
            "quay-pty-e2e-{}-{}",
            shell.replace('/', "_"),
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        install_shell_integration(&dir).unwrap();
        let out = Arc::new(Mutex::new(Vec::<u8>::new()));
        let sink = out.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut s = PtySession::spawn(
            SpawnOptions {
                program: Some(shell.to_string()),
                args: args.iter().map(|a| a.to_string()).collect(),
                size: TermSize { cols: 80, rows: 24 },
                integration_dir: Some(dir.clone()),
            },
            move |c| sink.lock().unwrap().extend(c),
            move || {
                let _ = tx.send(());
            },
        )
        .unwrap();
        // Give the shell time to read its startup files, then run a command and leave.
        std::thread::sleep(std::time::Duration::from_millis(1500));
        s.write(b"echo quaytest\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(800));
        s.write(b"exit\n").unwrap();
        let _ = rx.recv_timeout(std::time::Duration::from_secs(10));
        let _ = std::fs::remove_dir_all(&dir);
        let bytes = out.lock().unwrap().clone();
        Some(bytes)
    }

    #[cfg(unix)]
    fn has(bytes: &[u8], needle: &[u8]) -> bool {
        bytes.windows(needle.len()).any(|w| w == needle)
    }

    #[cfg(unix)]
    #[test]
    fn zsh_started_with_integration_emits_marks() {
        let Some(out) = marks_from("/bin/zsh", &["-i"]) else {
            return;
        };
        for mark in [
            &b"\x1b]133;A\x07"[..],
            b"\x1b]133;C\x07",
            b"\x1b]133;D;0\x07",
        ] {
            assert!(
                has(&out, mark),
                "missing {:?} in {:?}",
                String::from_utf8_lossy(mark),
                String::from_utf8_lossy(&out)
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn modern_bash_started_with_integration_emits_marks() {
        // PS0 needs bash 4.4+; the macOS system bash is 3.2 and simply gets no command marks.
        let modern = std::process::Command::new("/bin/bash")
            .args(["-c", "[[ ${BASH_VERSINFO[0]} -gt 4 || ( ${BASH_VERSINFO[0]} -eq 4 && ${BASH_VERSINFO[1]} -ge 4 ) ]]"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !modern {
            return;
        }
        let Some(out) = marks_from("/bin/bash", &["--norc", "-i"]) else {
            return;
        };
        for mark in [&b"\x1b]133;C\x07"[..], b"\x1b]133;D;0\x07"] {
            assert!(
                has(&out, mark),
                "missing {:?} in {:?}",
                String::from_utf8_lossy(mark),
                String::from_utf8_lossy(&out)
            );
        }
    }

    #[test]
    fn write_after_close_fails() {
        let mut s = PtySession::spawn(echo_opts(), |_| {}, || {}).unwrap();
        s.close().unwrap();
        assert!(matches!(s.write(b"x"), Err(Error::SessionClosed)));
    }
}
