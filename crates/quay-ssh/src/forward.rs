//! Port forwarding over an SSH connection: local (-L), remote (-R) and
//! dynamic (-D, a SOCKS5 proxy).
//!
//! A [`Tunnel`] keeps its connection open and stops when dropped.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use quay_core::{TunnelKind, TunnelSpec};
use russh::client::Msg;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::{JoinHandle, JoinSet};

use crate::client::{block_on, runtime, Connection};
use crate::error::SshError;

/// What a tunnel has carried so far.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TunnelCounts {
    /// Connections open right now.
    pub active: usize,
    /// Connections since the tunnel started.
    pub total: u64,
    /// Bytes sent towards the destination.
    pub bytes_up: u64,
    /// Bytes received from the destination.
    pub bytes_down: u64,
}

#[derive(Default)]
pub(crate) struct Stats {
    active: AtomicUsize,
    total: AtomicU64,
    up: AtomicU64,
    down: AtomicU64,
}

/// Counts a connection as active for as long as it lives.
struct ActiveGuard(Arc<Stats>);

impl ActiveGuard {
    fn new(stats: &Arc<Stats>) -> Self {
        stats.active.fetch_add(1, Ordering::Relaxed);
        stats.total.fetch_add(1, Ordering::Relaxed);
        Self(stats.clone())
    }
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Wraps the local side of a connection to count bytes as they pass.
struct Counted<S> {
    inner: S,
    stats: Arc<Stats>,
}

impl<S: AsyncRead + Unpin> AsyncRead for Counted<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let r = Pin::new(&mut self.inner).poll_read(cx, buf);
        if matches!(r, Poll::Ready(Ok(()))) {
            let n = (buf.filled().len() - before) as u64;
            self.stats.up.fetch_add(n, Ordering::Relaxed);
        }
        r
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Counted<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let r = Pin::new(&mut self.inner).poll_write(cx, data);
        if let Poll::Ready(Ok(n)) = r {
            self.stats.down.fetch_add(n as u64, Ordering::Relaxed);
        }
        r
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// Carry bytes both ways until either side closes.
async fn pump(
    local: impl AsyncRead + AsyncWrite + Unpin,
    remote: impl AsyncRead + AsyncWrite + Unpin,
    stats: &Arc<Stats>,
) {
    let mut local = Counted {
        inner: local,
        stats: stats.clone(),
    };
    let mut remote = remote;
    let _ = tokio::io::copy_bidirectional(&mut local, &mut remote).await;
}

/// Destinations of remote forwards by the port the server listens on. The
/// server opens a channel for each connection; this says where it goes.
#[derive(Default)]
pub(crate) struct RemoteForwards {
    map: Mutex<HashMap<u32, RemoteEntry>>,
}

struct RemoteEntry {
    dest: (String, u16),
    stats: Arc<Stats>,
    /// Open connections; dropping the entry ends them.
    conns: Arc<tokio::sync::Mutex<JoinSet<()>>>,
}

impl RemoteForwards {
    /// A connection arrived from the server for `port`. Returns false to refuse it.
    pub(crate) fn accept(&self, port: u32, channel: russh::Channel<Msg>) -> bool {
        let map = self.map.lock().unwrap();
        let Some(e) = map.get(&port) else {
            return false;
        };
        let (dest, stats, conns) = (e.dest.clone(), e.stats.clone(), e.conns.clone());
        drop(map);
        runtime().spawn(async move {
            let mut set = conns.lock().await;
            set.spawn(async move {
                let _active = ActiveGuard::new(&stats);
                if let Ok(tcp) = TcpStream::connect((dest.0.as_str(), dest.1)).await {
                    let _ = tcp.set_nodelay(true);
                    pump(tcp, channel.into_stream(), &stats).await;
                }
            });
        });
        true
    }
}

/// A running port forward. Dropping it stops the forward and ends its connections.
pub struct Tunnel {
    spec: TunnelSpec,
    bound_port: u16,
    stats: Arc<Stats>,
    conn: Arc<Connection>,
    kind: Running,
}

enum Running {
    /// Local and dynamic: a listener task on this machine.
    Listener(JoinHandle<()>),
    /// Remote: the server's listener, cancelled on drop.
    Remote { server_port: u32 },
}

impl Tunnel {
    /// The spec it was started from.
    pub fn spec(&self) -> &TunnelSpec {
        &self.spec
    }

    /// The port actually listening, which matters when 0 was asked for.
    pub fn bound_port(&self) -> u16 {
        self.bound_port
    }

    pub fn counts(&self) -> TunnelCounts {
        TunnelCounts {
            active: self.stats.active.load(Ordering::Relaxed),
            total: self.stats.total.load(Ordering::Relaxed),
            bytes_up: self.stats.up.load(Ordering::Relaxed),
            bytes_down: self.stats.down.load(Ordering::Relaxed),
        }
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        match &self.kind {
            Running::Listener(task) => task.abort(),
            Running::Remote { server_port } => {
                let port = *server_port;
                self.conn.remote.map.lock().unwrap().remove(&port);
                let conn = self.conn.clone();
                let bind = self.spec.bind_host.clone();
                // Best effort: if the connection is already gone there is nothing to cancel.
                runtime().spawn(async move {
                    let _ = tokio::time::timeout(
                        Duration::from_secs(2),
                        conn.handle.cancel_tcpip_forward(bind, port),
                    )
                    .await;
                });
            }
        }
    }
}

impl Connection {
    /// Start a port forward on this connection. Blocks briefly; call from a
    /// worker thread.
    pub fn start_tunnel(self: &Arc<Self>, spec: &TunnelSpec) -> Result<Tunnel, SshError> {
        block_on(start(self.clone(), spec.clone()))
    }
}

async fn start(conn: Arc<Connection>, spec: TunnelSpec) -> Result<Tunnel, SshError> {
    let stats = Arc::new(Stats::default());
    match spec.kind {
        TunnelKind::Local | TunnelKind::Dynamic => {
            let listener = TcpListener::bind((spec.bind_host.as_str(), spec.bind_port))
                .await
                .map_err(|e| bind_error(&spec, e))?;
            let bound_port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
            let task = runtime().spawn(accept_loop(
                listener,
                conn.clone(),
                spec.clone(),
                stats.clone(),
            ));
            Ok(Tunnel {
                spec,
                bound_port,
                stats,
                conn,
                kind: Running::Listener(task),
            })
        }
        TunnelKind::Remote => {
            let asked = spec.bind_port as u32;
            let server_port = conn
                .handle
                .tcpip_forward(spec.bind_host.clone(), asked)
                .await
                .map_err(|e| {
                    SshError::Tunnel(format!(
                        "The server would not listen on {}:{} ({e}). It may not allow remote forwarding.",
                        spec.bind_host, spec.bind_port
                    ))
                })?;
            // With port 0 the server picks one; some servers answer 0 even for a fixed port.
            let server_port = if server_port == 0 { asked } else { server_port };
            conn.remote.map.lock().unwrap().insert(
                server_port,
                RemoteEntry {
                    dest: (spec.dest_host.clone(), spec.dest_port),
                    stats: stats.clone(),
                    conns: Arc::new(tokio::sync::Mutex::new(JoinSet::new())),
                },
            );
            Ok(Tunnel {
                bound_port: server_port as u16,
                spec,
                stats,
                conn,
                kind: Running::Remote { server_port },
            })
        }
    }
}

fn bind_error(spec: &TunnelSpec, e: std::io::Error) -> SshError {
    let (host, port) = (&spec.bind_host, spec.bind_port);
    SshError::Tunnel(match e.kind() {
        std::io::ErrorKind::AddrInUse => format!("Port {port} on {host} is already in use."),
        std::io::ErrorKind::PermissionDenied => {
            format!(
                "Not allowed to listen on port {port} (ports below 1024 need extra privileges)."
            )
        }
        std::io::ErrorKind::AddrNotAvailable => {
            format!("This computer has no address {host} to listen on.")
        }
        _ => format!("Could not listen on {host}:{port}: {e}"),
    })
}

async fn accept_loop(
    listener: TcpListener,
    conn: Arc<Connection>,
    spec: TunnelSpec,
    stats: Arc<Stats>,
) {
    // Dropping the set (when this task is aborted) ends every open connection.
    let mut set = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let Ok((tcp, peer)) = accepted else { break };
                let _ = tcp.set_nodelay(true);
                let (conn, stats) = (conn.clone(), stats.clone());
                match spec.kind {
                    TunnelKind::Dynamic => {
                        set.spawn(socks_connection(tcp, peer, conn, stats));
                    }
                    _ => {
                        let dest = (spec.dest_host.clone(), spec.dest_port);
                        set.spawn(local_connection(tcp, peer, conn, dest, stats));
                    }
                }
            }
            Some(_) = set.join_next(), if !set.is_empty() => {}
        }
    }
}

async fn open_channel(
    conn: &Connection,
    host: &str,
    port: u16,
    peer: SocketAddr,
) -> Result<russh::Channel<Msg>, russh::Error> {
    conn.handle
        .channel_open_direct_tcpip(host, port as u32, peer.ip().to_string(), peer.port() as u32)
        .await
}

async fn local_connection(
    tcp: TcpStream,
    peer: SocketAddr,
    conn: Arc<Connection>,
    dest: (String, u16),
    stats: Arc<Stats>,
) {
    let _active = ActiveGuard::new(&stats);
    // If the server cannot reach the destination, dropping `tcp` resets the client.
    if let Ok(channel) = open_channel(&conn, &dest.0, dest.1, peer).await {
        pump(tcp, channel.into_stream(), &stats).await;
    }
}

// ---- SOCKS5 (RFC 1928), CONNECT only, no authentication ----

const SOCKS_TIMEOUT: Duration = Duration::from_secs(10);

async fn socks_connection(
    mut tcp: TcpStream,
    peer: SocketAddr,
    conn: Arc<Connection>,
    stats: Arc<Stats>,
) {
    let Ok(Ok(Some((host, port)))) =
        tokio::time::timeout(SOCKS_TIMEOUT, socks_handshake(&mut tcp)).await
    else {
        return;
    };
    let _active = ActiveGuard::new(&stats);
    match open_channel(&conn, &host, port, peer).await {
        Ok(channel) => {
            // Success, with a placeholder bound address.
            if tcp.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await.is_ok() {
                pump(tcp, channel.into_stream(), &stats).await;
            }
        }
        Err(e) => {
            let code = match e {
                russh::Error::ChannelOpenFailure(
                    russh::ChannelOpenFailure::AdministrativelyProhibited,
                ) => 2, // not allowed by ruleset
                russh::Error::ChannelOpenFailure(russh::ChannelOpenFailure::UnknownChannelType) => {
                    7
                }
                _ => 5, // connection refused
            };
            let _ = tcp.write_all(&[5, code, 0, 1, 0, 0, 0, 0, 0, 0]).await;
        }
    }
}

/// Read the greeting and the request. `Ok(None)` means the client was refused
/// (and told so); `Ok(Some)` is the destination, with the reply still to send.
async fn socks_handshake(tcp: &mut TcpStream) -> std::io::Result<Option<(String, u16)>> {
    let mut head = [0u8; 2];
    tcp.read_exact(&mut head).await?;
    if head[0] != 5 {
        return Ok(None); // not SOCKS5 (SOCKS4 is not supported)
    }
    let mut methods = vec![0u8; head[1] as usize];
    tcp.read_exact(&mut methods).await?;
    if !methods.contains(&0) {
        tcp.write_all(&[5, 0xff]).await?;
        return Ok(None);
    }
    tcp.write_all(&[5, 0]).await?;

    let mut req = [0u8; 4];
    tcp.read_exact(&mut req).await?;
    if req[1] != 1 {
        tcp.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0]).await?; // only CONNECT
        return Ok(None);
    }
    let host = match req[3] {
        1 => {
            let mut a = [0u8; 4];
            tcp.read_exact(&mut a).await?;
            std::net::Ipv4Addr::from(a).to_string()
        }
        4 => {
            let mut a = [0u8; 16];
            tcp.read_exact(&mut a).await?;
            std::net::Ipv6Addr::from(a).to_string()
        }
        3 => {
            let mut len = [0u8; 1];
            tcp.read_exact(&mut len).await?;
            let mut name = vec![0u8; len[0] as usize];
            tcp.read_exact(&mut name).await?;
            String::from_utf8_lossy(&name).into_owned()
        }
        _ => {
            tcp.write_all(&[5, 8, 0, 1, 0, 0, 0, 0, 0, 0]).await?; // address type not supported
            return Ok(None);
        }
    };
    let mut port = [0u8; 2];
    tcp.read_exact(&mut port).await?;
    Ok(Some((host, u16::from_be_bytes(port))))
}
