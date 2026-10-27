//! Port forwarding against a real OpenSSH server.
//!
//! Skipped unless `QUAY_TEST_SSH_HOST` is set; see `scripts/ssh-test-server.sh`.
//! The server's own sshd (port 2222 inside the container) is the destination
//! that is reachable from the server's point of view.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

use quay_core::{SshAuth, SshTarget, TunnelKind, TunnelSpec};
use quay_ssh::{ConnectOptions, Connection, SshError};

fn connect() -> Option<Arc<Connection>> {
    let var = |k: &str| std::env::var(k).ok();
    let dir = tempfile::tempdir().ok()?;
    let mut o = ConnectOptions::new(SshTarget {
        host: var("QUAY_TEST_SSH_HOST")?,
        port: var("QUAY_TEST_SSH_PORT")?.parse().ok()?,
        user: var("QUAY_TEST_SSH_USER")?,
        auth: SshAuth::Password {
            password: var("QUAY_TEST_SSH_PASSWORD")?,
        },
    });
    o.known_hosts = Some(dir.path().join("known_hosts"));
    // The temp dir may go away; the file is only read during connect.
    Some(Arc::new(Connection::connect(&o, &mut |_| true).unwrap()))
}

macro_rules! conn {
    () => {
        match connect() {
            Some(c) => c,
            None => {
                eprintln!("skipped: QUAY_TEST_SSH_HOST not set");
                return;
            }
        }
    };
}

fn spec(kind: TunnelKind, dest_port: u16) -> TunnelSpec {
    TunnelSpec {
        kind,
        dest_host: "localhost".into(),
        dest_port,
        ..TunnelSpec::default()
    }
}

/// Read the SSH banner of whatever is listening at the other end.
fn banner(mut s: TcpStream) -> String {
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut b = [0u8; 8];
    s.read_exact(&mut b).unwrap();
    String::from_utf8_lossy(&b).into_owned()
}

/// An echo server on this machine; returns its port.
fn echo_server() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            std::thread::spawn(move || {
                let (mut r, mut w) = (s.try_clone().unwrap(), s);
                let mut buf = [0u8; 1024];
                while let Ok(n) = r.read(&mut buf) {
                    if n == 0 || w.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
            });
        }
    });
    port
}

fn wait_until(what: &str, mut ok: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    while !ok() {
        assert!(Instant::now() < end, "never: {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn local_forward_reaches_a_port_the_server_can_see() {
    let conn = conn!();
    let t = conn.start_tunnel(&spec(TunnelKind::Local, 2222)).unwrap();
    assert_ne!(t.bound_port(), 0, "port 0 resolves to the port chosen");

    let s = TcpStream::connect(("127.0.0.1", t.bound_port())).unwrap();
    assert_eq!(banner(s), "SSH-2.0-");

    wait_until("the count shows the connection", || {
        let c = t.counts();
        c.total == 1 && c.bytes_down >= 8
    });

    // Stopping the tunnel closes its port.
    let port = t.bound_port();
    drop(t);
    wait_until("the port closes", || {
        TcpStream::connect(("127.0.0.1", port)).is_err()
    });
}

#[test]
fn dynamic_forward_speaks_socks5() {
    let conn = conn!();
    let t = conn.start_tunnel(&spec(TunnelKind::Dynamic, 0)).unwrap();
    let addr = ("127.0.0.1", t.bound_port());

    // Greeting, then CONNECT to a host name (resolved by the server): localhost:2222.
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    s.write_all(&[5, 1, 0]).unwrap();
    let mut reply = [0u8; 2];
    s.read_exact(&mut reply).unwrap();
    assert_eq!(reply, [5, 0], "no authentication");
    let mut req = vec![5, 1, 0, 3, 9];
    req.extend_from_slice(b"localhost");
    req.extend_from_slice(&2222u16.to_be_bytes());
    s.write_all(&req).unwrap();
    let mut ok = [0u8; 10];
    s.read_exact(&mut ok).unwrap();
    assert_eq!(&ok[..2], [5, 0], "connected");
    assert_eq!(banner(s), "SSH-2.0-");

    // A destination the server cannot reach is a "connection refused" reply, not a hang.
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    s.write_all(&[5, 1, 0]).unwrap();
    s.read_exact(&mut reply).unwrap();
    s.write_all(&[5, 1, 0, 1, 127, 0, 0, 1, 0, 1]).unwrap(); // 127.0.0.1:1
    let mut fail = [0u8; 10];
    s.read_exact(&mut fail).unwrap();
    assert_eq!(fail[1], 5, "connection refused");

    // Anything but CONNECT is declined politely.
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    s.write_all(&[5, 1, 0]).unwrap();
    s.read_exact(&mut reply).unwrap();
    s.write_all(&[5, 2, 0, 1, 127, 0, 0, 1, 0, 80]).unwrap(); // BIND
    s.read_exact(&mut fail).unwrap();
    assert_eq!(fail[1], 7, "command not supported");
}

#[test]
fn remote_forward_brings_the_server_port_back_here() {
    let conn = conn!();
    let echo = echo_server();

    // The server listens (any free port) and sends connections to our echo server.
    let mut remote = spec(TunnelKind::Remote, echo);
    remote.dest_host = "127.0.0.1".into();
    let r = conn.start_tunnel(&remote).unwrap();
    assert_ne!(r.bound_port(), 0, "the server told us which port it chose");

    // Reach that port from the server's side with a local forward on the same connection.
    let hop = conn
        .start_tunnel(&spec(TunnelKind::Local, r.bound_port()))
        .unwrap();
    let mut s = TcpStream::connect(("127.0.0.1", hop.bound_port())).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    s.write_all(b"ping").unwrap();
    let mut back = [0u8; 4];
    s.read_exact(&mut back).unwrap();
    assert_eq!(&back, b"ping", "echoed by the server on our machine");
    wait_until("the remote side counted it", || r.counts().total == 1);
}

#[test]
fn a_busy_port_is_explained() {
    let conn = conn!();
    let taken = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut s = spec(TunnelKind::Local, 2222);
    s.bind_port = taken.local_addr().unwrap().port();
    let err = conn.start_tunnel(&s).err().unwrap();
    assert!(
        matches!(&err, SshError::Tunnel(m) if m.contains("already in use")),
        "{err}"
    );
}
