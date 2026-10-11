//! Integration tests against a real OpenSSH server.
//!
//! Skipped unless `QUAY_TEST_SSH_HOST` is set; see `scripts/ssh-test-server.sh`.

use std::sync::{mpsc, Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use quay_core::{Session, SshAuth, SshTarget, TermSize};
use quay_ssh::{ConnectOptions, SshError, SshSession};

struct Env {
    host: String,
    port: u16,
    user: String,
    password: String,
    key: String,
    key_protected: String,
    key_passphrase: String,
}

fn env() -> Option<Env> {
    let var = |k: &str| std::env::var(k).ok();
    Some(Env {
        host: var("QUAY_TEST_SSH_HOST")?,
        port: var("QUAY_TEST_SSH_PORT")?.parse().ok()?,
        user: var("QUAY_TEST_SSH_USER")?,
        password: var("QUAY_TEST_SSH_PASSWORD")?,
        key: var("QUAY_TEST_SSH_KEY")?,
        key_protected: var("QUAY_TEST_SSH_KEY_PROTECTED")?,
        key_passphrase: var("QUAY_TEST_SSH_KEY_PASSPHRASE")?,
    })
}

/// Tests share one server. Ordinary tests hold the read side; the test that
/// restarts the server holds the write side so nothing else is mid-flight.
static GATE: RwLock<()> = RwLock::new(());

macro_rules! server {
    () => {{
        let Some(e) = env() else {
            eprintln!("skipped: QUAY_TEST_SSH_HOST not set");
            return;
        };
        (e, GATE.read().unwrap_or_else(|p| p.into_inner()))
    }};
}

fn opts(e: &Env, auth: SshAuth, known_hosts: &std::path::Path) -> ConnectOptions {
    let mut o = ConnectOptions::new(SshTarget {
        host: e.host.clone(),
        port: e.port,
        user: e.user.clone(),
        auth,
    });
    o.known_hosts = Some(known_hosts.to_path_buf());
    o.timeout = Duration::from_secs(10);
    o
}

struct Live {
    session: SshSession,
    out: Arc<Mutex<Vec<u8>>>,
    exit: mpsc::Receiver<Option<String>>,
}

impl Live {
    fn wait_for(&self, needle: &str) {
        let end = Instant::now() + Duration::from_secs(10);
        while Instant::now() < end {
            if String::from_utf8_lossy(&self.out.lock().unwrap()).contains(needle) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!(
            "never saw {needle:?}; output: {:?}",
            String::from_utf8_lossy(&self.out.lock().unwrap())
        );
    }
}

fn connect(
    o: &ConnectOptions,
    verify: impl FnMut(&quay_ssh::HostKeyInfo) -> bool,
) -> Result<Live, SshError> {
    let out = Arc::new(Mutex::new(Vec::new()));
    let sink = out.clone();
    let (tx, exit) = mpsc::channel();
    let session = SshSession::connect(
        o,
        TermSize {
            cols: 100,
            rows: 30,
        },
        verify,
        move |chunk| sink.lock().unwrap().extend(chunk),
        move |reason| {
            let _ = tx.send(reason);
        },
    )?;
    Ok(Live { session, out, exit })
}

fn password(e: &Env) -> SshAuth {
    SshAuth::Password {
        password: e.password.clone(),
    }
}

#[test]
fn password_login_runs_commands_and_resizes() {
    let (e, _gate) = server!();
    let dir = tempfile::tempdir().unwrap();
    let kh = dir.path().join("known_hosts");
    let mut live = connect(&opts(&e, password(&e), &kh), |_| true).unwrap();

    live.session.write(b"echo quay-$((6*7))\n").unwrap();
    live.wait_for("quay-42");

    live.session
        .resize(TermSize {
            cols: 120,
            rows: 40,
        })
        .unwrap();
    live.session.write(b"stty size\n").unwrap();
    live.wait_for("40 120");

    live.session.write(b"exit\n").unwrap();
    let reason = live.exit.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(reason, None, "a clean exit carries no error");
}

#[test]
fn host_key_is_prompted_once_then_remembered() {
    let (e, _gate) = server!();
    let dir = tempfile::tempdir().unwrap();
    let kh = dir.path().join("known_hosts");
    let o = opts(&e, password(&e), &kh);

    let mut prompts = 0;
    let mut seen = None;
    let first = connect(&o, |info| {
        prompts += 1;
        seen = Some(info.clone());
        true
    })
    .unwrap();
    drop(first);
    assert_eq!(prompts, 1);
    let info = seen.unwrap();
    assert!(info.fingerprint.starts_with("SHA256:"), "{info:?}");
    let text = std::fs::read_to_string(&kh).unwrap();
    assert!(text.contains(&info.key_type), "{text}");

    // Second connect: trusted, so the verifier must not run.
    let second = connect(&o, |_| panic!("should not prompt for a known host")).unwrap();
    drop(second);
}

#[test]
fn rejecting_the_host_key_cancels_and_records_nothing() {
    let (e, _gate) = server!();
    let dir = tempfile::tempdir().unwrap();
    let kh = dir.path().join("known_hosts");
    let err = connect(&opts(&e, password(&e), &kh), |_| false)
        .err()
        .unwrap();
    assert!(matches!(err, SshError::HostKeyRejected), "{err}");
    assert!(!kh.exists());
}

#[test]
fn changed_host_key_is_refused_without_prompting() {
    let (e, _gate) = server!();
    let dir = tempfile::tempdir().unwrap();
    let kh = dir.path().join("known_hosts");
    // A bogus ed25519 key recorded for the host.
    let bogus = "AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let host = if e.port == 22 {
        e.host.clone()
    } else {
        format!("[{}]:{}", e.host, e.port)
    };
    std::fs::write(&kh, format!("{host} ssh-ed25519 {bogus}\n")).unwrap();

    let err = connect(&opts(&e, password(&e), &kh), |_| panic!("must not prompt"))
        .err()
        .unwrap();
    assert!(matches!(err, SshError::HostKeyChanged { .. }), "{err}");
    assert!(err.to_string().contains("CHANGED"));
}

#[test]
fn wrong_password_is_reported_clearly() {
    let (e, _gate) = server!();
    let dir = tempfile::tempdir().unwrap();
    let kh = dir.path().join("known_hosts");
    let auth = SshAuth::Password {
        password: "definitely-wrong".into(),
    };
    let err = connect(&opts(&e, auth, &kh), |_| true).err().unwrap();
    assert!(matches!(err, SshError::PasswordRejected { .. }), "{err}");
}

#[test]
fn key_login_with_and_without_passphrase() {
    let (e, _gate) = server!();
    let dir = tempfile::tempdir().unwrap();
    let kh = dir.path().join("known_hosts");

    let plain = SshAuth::Key {
        path: e.key.clone(),
        passphrase: None,
    };
    let mut live = connect(&opts(&e, plain, &kh), |_| true).unwrap();
    live.session.write(b"echo key-ok\n").unwrap();
    live.wait_for("key-ok");
    drop(live);

    let protected = SshAuth::Key {
        path: e.key_protected.clone(),
        passphrase: Some(e.key_passphrase.clone()),
    };
    let live = connect(&opts(&e, protected, &kh), |_| true).unwrap();
    drop(live);

    let wrong = SshAuth::Key {
        path: e.key_protected.clone(),
        passphrase: Some("nope".into()),
    };
    let err = connect(&opts(&e, wrong, &kh), |_| true).err().unwrap();
    assert!(matches!(err, SshError::KeyFile { .. }), "{err}");

    let missing = SshAuth::Key {
        path: "/definitely/not/here".into(),
        passphrase: None,
    };
    let err = connect(&opts(&e, missing, &kh), |_| true).err().unwrap();
    assert!(err.to_string().contains("file not found"), "{err}");
}

#[test]
fn closing_the_session_ends_it_cleanly() {
    let (e, _gate) = server!();
    let dir = tempfile::tempdir().unwrap();
    let kh = dir.path().join("known_hosts");
    let mut live = connect(&opts(&e, password(&e), &kh), |_| true).unwrap();
    live.session.close().unwrap();
    assert_eq!(
        live.exit.recv_timeout(Duration::from_secs(10)).unwrap(),
        None
    );
    assert!(matches!(
        live.session.write(b"x"),
        Err(quay_core::Error::SessionClosed)
    ));
}

#[test]
fn refused_connection_has_a_friendly_message() {
    // No server needed: nothing listens on port 1 of localhost.
    let dir = tempfile::tempdir().unwrap();
    let mut o = ConnectOptions::new(SshTarget {
        host: "127.0.0.1".into(),
        port: 1,
        user: "x".into(),
        auth: SshAuth::Agent,
    });
    o.known_hosts = Some(dir.path().join("kh"));
    let err = connect(&o, |_| true).err().unwrap();
    assert!(matches!(err, SshError::Refused { .. }), "{err}");
}

#[test]
fn unresolvable_host_has_a_friendly_message() {
    let dir = tempfile::tempdir().unwrap();
    let mut o = ConnectOptions::new(SshTarget {
        host: "no-such-host.invalid".into(),
        port: 22,
        user: "x".into(),
        auth: SshAuth::Agent,
    });
    o.known_hosts = Some(dir.path().join("kh"));
    let err = connect(&o, |_| true).err().unwrap();
    assert!(matches!(err, SshError::Resolve { .. }), "{err}");
}

/// Known limitation: libssh2 reports a recorded key of another type as a
/// mismatch (OpenSSH would negotiate the recorded type instead). The safe
/// outcome, refusing without trusting anything, is what we pin here.
#[test]
fn other_key_type_on_record_is_refused_not_trusted() {
    let (e, _gate) = server!();
    let dir = tempfile::tempdir().unwrap();
    let kh = dir.path().join("known_hosts");
    let bogus = "AAAAB3NzaC1yc2EAAAADAQABAAAAgQC7bogusbogusbogusbogusbogusbogusbogusbogusbogus";
    let host = format!("[{}]:{}", e.host, e.port);
    std::fs::write(&kh, format!("{host} ssh-rsa {bogus}\n")).unwrap();
    let err = connect(&opts(&e, password(&e), &kh), |_| panic!("must not prompt"))
        .err()
        .unwrap();
    assert!(matches!(err, SshError::HostKeyChanged { .. }), "{err}");
}

/// Needs permission to restart the server container, so it is opt-in.
#[test]
fn lost_connection_reports_a_reason() {
    let Some(e) = env() else {
        eprintln!("skipped: QUAY_TEST_SSH_HOST not set");
        return;
    };
    let _gate = GATE.write().unwrap_or_else(|p| p.into_inner());
    let Ok(container) = std::env::var("QUAY_TEST_SSH_CONTAINER") else {
        eprintln!("skipped: QUAY_TEST_SSH_CONTAINER not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let kh = dir.path().join("known_hosts");
    let live = connect(&opts(&e, password(&e), &kh), |_| true).unwrap();
    let status = std::process::Command::new("docker")
        .args(["restart", &container])
        .status()
        .unwrap();
    assert!(status.success());
    let reason = live.exit.recv_timeout(Duration::from_secs(60)).unwrap();
    assert!(
        reason.is_some(),
        "expected an error reason, got a clean exit"
    );
    eprintln!("reason: {}", reason.unwrap());

    // Leave the server usable for whatever runs next.
    let end = Instant::now() + Duration::from_secs(60);
    while std::net::TcpStream::connect((e.host.as_str(), e.port)).is_err() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(500));
    }
    std::thread::sleep(Duration::from_secs(3));
}
