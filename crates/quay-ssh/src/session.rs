//! A live shell on a remote host.
//!
//! libssh2 sessions are not thread-safe, so one thread owns the connection
//! and talks to it in non-blocking mode. [`SshSession`] is a handle that
//! sends commands to that thread.

use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use quay_core::{Error, Result, Session, TermSize};
use ssh2::{Channel, Session as RawSession};

use crate::connect::{connect, ConnectOptions};
use crate::error::{would_block, SshError};
use crate::known_hosts::HostKeyInfo;

const BATCH_WINDOW: Duration = Duration::from_millis(8);
const BATCH_MAX: usize = 64 * 1024;
const POLL_MIN: Duration = Duration::from_millis(1);
const POLL_MAX: Duration = Duration::from_millis(10);

enum Cmd {
    Write(Vec<u8>),
    Resize(TermSize),
    Close,
}

pub struct SshSession {
    tx: Sender<Cmd>,
    closed: bool,
}

impl SshSession {
    /// Connect, authenticate and start a shell. Blocks until the shell is
    /// open (or fails), so call it from a worker thread.
    ///
    /// `verify` decides whether to trust an unknown host key. `on_output`
    /// receives batched output; `on_exit` is called once when the session
    /// ends, with a message if it ended because of an error.
    pub fn connect(
        opts: &ConnectOptions,
        size: TermSize,
        mut verify: impl FnMut(&HostKeyInfo) -> bool,
        on_output: impl FnMut(Vec<u8>) + Send + 'static,
        on_exit: impl FnOnce(Option<String>) + Send + 'static,
    ) -> std::result::Result<Self, SshError> {
        let session = connect(opts, &mut verify)?;
        let mut channel = session
            .channel_session()
            .map_err(|e| SshError::Channel(e.message().to_string()))?;
        channel
            .request_pty(
                "xterm-256color",
                None,
                Some((size.cols.max(1) as u32, size.rows.max(1) as u32, 0, 0)),
            )
            .map_err(|e| SshError::Channel(e.message().to_string()))?;
        channel
            .shell()
            .map_err(|e| SshError::Channel(e.message().to_string()))?;

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let reason = io_loop(session, channel, rx, on_output);
            on_exit(reason);
        });
        Ok(Self { tx, closed: false })
    }

    fn send(&self, cmd: Cmd) -> Result<()> {
        if self.closed {
            return Err(Error::SessionClosed);
        }
        self.tx.send(cmd).map_err(|_| Error::SessionClosed)
    }
}

impl Session for SshSession {
    fn write(&mut self, data: &[u8]) -> Result<()> {
        self.send(Cmd::Write(data.to_vec()))
    }

    fn resize(&mut self, size: TermSize) -> Result<()> {
        self.send(Cmd::Resize(size))
    }

    fn close(&mut self) -> Result<()> {
        if !self.closed {
            let _ = self.tx.send(Cmd::Close);
            self.closed = true;
        }
        Ok(())
    }
}

impl Drop for SshSession {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

/// Run until the session ends. Returns an error message if it ended badly.
fn io_loop(
    session: RawSession,
    mut channel: Channel,
    rx: Receiver<Cmd>,
    mut emit: impl FnMut(Vec<u8>),
) -> Option<String> {
    session.set_blocking(false);

    let mut pending_out: Vec<u8> = Vec::new();
    let mut pending_resize: Option<TermSize> = None;
    let mut batch: Vec<u8> = Vec::new();
    let mut batch_since = Instant::now();
    let mut next_keepalive = Instant::now() + Duration::from_secs(1);
    let mut idle = POLL_MIN;
    let mut buf = [0u8; 16 * 1024];

    let outcome: Option<String> = 'run: loop {
        let mut busy = false;

        // Commands from the UI.
        loop {
            match rx.try_recv() {
                Ok(Cmd::Write(d)) => pending_out.extend_from_slice(&d),
                Ok(Cmd::Resize(s)) => pending_resize = Some(s),
                Ok(Cmd::Close) | Err(TryRecvError::Disconnected) => break 'run None,
                Err(TryRecvError::Empty) => break,
            }
        }

        // Keystrokes out.
        while !pending_out.is_empty() {
            match channel.write(&pending_out) {
                Ok(n) => {
                    pending_out.drain(..n);
                    busy = true;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => break 'run Some(format!("Connection lost: {e}")),
            }
        }
        if let Some(size) = pending_resize {
            match channel.request_pty_size(
                size.cols.max(1) as u32,
                size.rows.max(1) as u32,
                None,
                None,
            ) {
                Ok(()) => pending_resize = None,
                Err(e) if would_block(&e) => {}
                Err(_) => pending_resize = None, // a failed resize is not fatal
            }
        }

        // Output in. Reading stderr too keeps a non-pty fallback from stalling.
        let mut got_data = false;
        for stream in [0, 1] {
            loop {
                let res = if stream == 0 {
                    channel.read(&mut buf)
                } else {
                    channel.stderr().read(&mut buf)
                };
                match res {
                    Ok(0) => break,
                    Ok(n) => {
                        if batch.is_empty() {
                            batch_since = Instant::now();
                        }
                        batch.extend_from_slice(&buf[..n]);
                        got_data = true;
                        busy = true;
                        if batch.len() >= BATCH_MAX {
                            emit(std::mem::take(&mut batch));
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) => {
                        flush(&mut batch, &mut emit);
                        break 'run Some(format!("Connection lost: {e}"));
                    }
                }
            }
        }
        if !batch.is_empty() && (!got_data || batch_since.elapsed() >= BATCH_WINDOW) {
            flush(&mut batch, &mut emit);
        }
        if channel.eof() {
            flush(&mut batch, &mut emit);
            break 'run None;
        }

        // Keep the connection alive and notice when it has died.
        if Instant::now() >= next_keepalive {
            match session.keepalive_send() {
                Ok(secs) => {
                    next_keepalive = Instant::now() + Duration::from_secs(secs.max(1) as u64)
                }
                Err(e) if would_block(&e) => {}
                Err(e) => break 'run Some(format!("Connection lost: {}", e.message())),
            }
        }

        if busy {
            idle = POLL_MIN;
        } else {
            // Sleep, but wake at once for keystrokes.
            match rx.recv_timeout(idle) {
                Ok(Cmd::Write(d)) => pending_out.extend_from_slice(&d),
                Ok(Cmd::Resize(s)) => pending_resize = Some(s),
                Ok(Cmd::Close) | Err(RecvTimeoutError::Disconnected) => break 'run None,
                Err(RecvTimeoutError::Timeout) => idle = (idle * 2).min(POLL_MAX),
            }
        }
    };

    // Best-effort goodbye.
    session.set_blocking(true);
    session.set_timeout(2000);
    let _ = channel.close();
    let _ = session.disconnect(None, "quay-term closing", None);
    outcome
}

fn flush(batch: &mut Vec<u8>, emit: &mut impl FnMut(Vec<u8>)) {
    if !batch.is_empty() {
        emit(std::mem::take(batch));
    }
}
