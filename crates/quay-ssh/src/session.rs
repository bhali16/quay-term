//! A live shell on a remote host.
//!
//! The connection runs on the shared tokio runtime. [`SshSession`] is a
//! handle that sends keystrokes and resizes to it; output comes back through
//! the callbacks given to [`SshSession::connect`], batched so a busy terminal
//! does not flood the UI.

use std::time::Duration;

use quay_core::{Error, Result, Session, TermSize};
use russh::client::Msg;
use russh::{ChannelMsg, ChannelReadHalf, ChannelWriteHalf, Disconnect};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use tokio::time::Instant;

use crate::client::{self, runtime, Connection};
use crate::error::SshError;
use crate::known_hosts::HostKeyInfo;
use crate::options::ConnectOptions;

const BATCH_WINDOW: Duration = Duration::from_millis(8);
const BATCH_MAX: usize = 64 * 1024;

enum Cmd {
    Write(Vec<u8>),
    Resize(TermSize),
    Close,
}

pub struct SshSession {
    tx: UnboundedSender<Cmd>,
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
        let conn = client::connect(opts, &mut verify)?;
        let (read, write) = runtime().block_on(open_shell(&conn, size))?;

        let (tx, rx) = unbounded_channel();
        let (closed_tx, closed_rx) = oneshot::channel();
        runtime().spawn(write_loop(write, rx, closed_tx));
        runtime().spawn(async move {
            let reason = read_loop(read, closed_rx, on_output).await;
            // Best-effort goodbye; a server that never answers must not hold us up.
            let bye = conn
                .handle
                .disconnect(Disconnect::ByApplication, "quay-term closing", "en");
            let _ = tokio::time::timeout(Duration::from_secs(2), bye).await;
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

async fn open_shell(
    conn: &Connection,
    size: TermSize,
) -> std::result::Result<(ChannelReadHalf, ChannelWriteHalf<Msg>), SshError> {
    let chan_err = |e: russh::Error| SshError::Channel(e.to_string());
    let channel = conn.handle.channel_open_session().await.map_err(chan_err)?;
    channel
        .request_pty(
            true,
            "xterm-256color",
            size.cols.max(1) as u32,
            size.rows.max(1) as u32,
            0,
            0,
            &[],
        )
        .await
        .map_err(chan_err)?;
    channel.request_shell(true).await.map_err(chan_err)?;
    Ok(channel.split())
}

/// Sends keystrokes and resizes. Separate from reading so a full remote
/// window can never stop us from draining output (which would deadlock a
/// big paste into a program that echoes as it reads).
async fn write_loop(
    write: ChannelWriteHalf<Msg>,
    mut rx: UnboundedReceiver<Cmd>,
    closed: oneshot::Sender<()>,
) {
    let mut asked_to_close = false;
    while let Some(cmd) = rx.recv().await {
        let ok = match cmd {
            Cmd::Write(data) => write.data_bytes(data).await.is_ok(),
            // A failed resize is not fatal.
            Cmd::Resize(s) => {
                let _ = write
                    .window_change(s.cols.max(1) as u32, s.rows.max(1) as u32, 0, 0)
                    .await;
                true
            }
            Cmd::Close => {
                asked_to_close = true;
                false
            }
        };
        if !ok {
            break;
        }
    }
    let _ = write.eof().await;
    let _ = write.close().await;
    // A shell on a pty may outlive its channel, so do not wait for the server
    // to confirm: tell the reader to wrap up.
    if asked_to_close {
        let _ = closed.send(());
    }
}

/// Collect output into batches until the session ends. Returns an error
/// message if it ended badly.
async fn read_loop(
    mut read: ChannelReadHalf,
    mut closed: oneshot::Receiver<()>,
    mut emit: impl FnMut(Vec<u8>),
) -> Option<String> {
    let mut batch: Vec<u8> = Vec::new();
    let mut flush_at: Option<Instant> = None;
    let mut ended_cleanly = false;

    loop {
        let timer = async {
            match flush_at {
                Some(t) => tokio::time::sleep_until(t).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            msg = read.wait() => match msg {
                Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                    if batch.is_empty() {
                        flush_at = Some(Instant::now() + BATCH_WINDOW);
                    }
                    batch.extend_from_slice(&data);
                    if batch.len() >= BATCH_MAX {
                        emit(std::mem::take(&mut batch));
                        flush_at = None;
                    }
                }
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) => {
                    ended_cleanly = true;
                    break;
                }
                Some(_) => {}
                None => break,
            },
            _ = &mut closed => {
                ended_cleanly = true;
                break;
            }
            _ = timer => {
                emit(std::mem::take(&mut batch));
                flush_at = None;
            }
        }
    }
    if !batch.is_empty() {
        emit(batch);
    }
    (!ended_cleanly).then(|| "Connection lost".to_string())
}
