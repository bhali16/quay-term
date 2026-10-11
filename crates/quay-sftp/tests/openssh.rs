//! Integration tests against a real OpenSSH server (SFTP subsystem).
//!
//! Skipped unless `QUAY_TEST_SSH_HOST` is set; see `scripts/ssh-test-server.sh`.

use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc};
use std::time::Duration;

use quay_core::{SshAuth, SshTarget};
use quay_sftp::{
    Direction, EntryKind, SftpClient, SftpError, TransferEvent, TransferQueue, TransferRequest,
    TransferState,
};
use quay_ssh::ConnectOptions;

fn client(known_hosts: &std::path::Path) -> Option<SftpClient> {
    let var = |k: &str| std::env::var(k).ok();
    let mut opts = ConnectOptions::new(SshTarget {
        host: var("QUAY_TEST_SSH_HOST")?,
        port: var("QUAY_TEST_SSH_PORT")?.parse().ok()?,
        user: var("QUAY_TEST_SSH_USER")?,
        auth: SshAuth::Password {
            password: var("QUAY_TEST_SSH_PASSWORD")?,
        },
    });
    opts.known_hosts = Some(known_hosts.to_path_buf());
    Some(SftpClient::connect(&opts, &mut |_| true).expect("sftp connect"))
}

macro_rules! setup {
    () => {{
        let dir = tempfile::tempdir().unwrap();
        let Some(c) = client(&dir.path().join("known_hosts")) else {
            eprintln!("skipped: QUAY_TEST_SSH_HOST not set");
            return;
        };
        // Each test works in its own remote folder.
        let base = format!(
            "{}/t-{}",
            c.home().unwrap(),
            std::process::id() as u64 * 1000 + line!() as u64
        );
        let _ = c.remove(&base);
        c.mkdir(&base).unwrap();
        (dir, c, base)
    }};
}

#[test]
fn list_mkdir_rename_remove() {
    let (_d, c, base) = setup!();
    c.mkdir(&format!("{base}/sub")).unwrap();
    let local = _d.path().join("a.txt");
    std::fs::write(&local, "hello").unwrap();
    let no_cancel = AtomicBool::new(false);
    c.upload(&local, &format!("{base}/b.txt"), &no_cancel, &mut |_| {})
        .unwrap();
    c.upload(&local, &format!("{base}/A.txt"), &no_cancel, &mut |_| {})
        .unwrap();

    let list = c.list(&base).unwrap();
    let names: Vec<_> = list.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        ["sub", "A.txt", "b.txt"],
        "dirs first, then case-insensitive"
    );
    assert_eq!(list[0].kind, EntryKind::Dir);
    assert_eq!(list[2].size, 5);

    c.rename(&format!("{base}/b.txt"), &format!("{base}/c.txt"))
        .unwrap();
    assert!(c.list(&base).unwrap().iter().any(|e| e.name == "c.txt"));

    c.remove(&base).unwrap(); // recursive
    assert!(c.list(&base).is_err());
}

#[test]
fn errors_are_readable() {
    let (_d, c, base) = setup!();
    let msg = c.list(&format!("{base}/missing")).unwrap_err().to_string();
    assert!(msg.contains("no such file"), "{msg}");
    c.remove(&base).unwrap();
}

#[test]
fn round_trip_file_and_folder() {
    let (d, c, base) = setup!();
    let src = d.path().join("tree");
    std::fs::create_dir_all(src.join("inner")).unwrap();
    let big: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(src.join("big.bin"), &big).unwrap();
    std::fs::write(src.join("inner/note.txt"), "note").unwrap();

    let no_cancel = AtomicBool::new(false);
    let mut sent = 0;
    c.upload(&src, &format!("{base}/tree"), &no_cancel, &mut |n| {
        sent += n
    })
    .unwrap();
    assert_eq!(sent, big.len() as u64 + 4);
    assert_eq!(c.size_of(&format!("{base}/tree")).unwrap(), sent);

    let back = d.path().join("back");
    let mut got = 0;
    c.download(&format!("{base}/tree"), &back, &no_cancel, &mut |n| {
        got += n
    })
    .unwrap();
    assert_eq!(got, sent);
    assert_eq!(std::fs::read(back.join("big.bin")).unwrap(), big);
    assert_eq!(
        std::fs::read_to_string(back.join("inner/note.txt")).unwrap(),
        "note"
    );
    c.remove(&base).unwrap();
}

#[test]
fn cancel_removes_partial_download() {
    let (d, c, base) = setup!();
    let local = d.path().join("f.bin");
    std::fs::write(&local, vec![1u8; 300_000]).unwrap();
    let no_cancel = AtomicBool::new(false);
    c.upload(&local, &format!("{base}/f.bin"), &no_cancel, &mut |_| {})
        .unwrap();

    let cancel = AtomicBool::new(false);
    let out = d.path().join("out.bin");
    let r = c.download(&format!("{base}/f.bin"), &out, &cancel, &mut |_| {
        cancel.store(true, std::sync::atomic::Ordering::Relaxed)
    });
    assert!(matches!(r, Err(SftpError::Cancelled)));
    assert!(!out.exists(), "partial file is removed");
    c.remove(&base).unwrap();
}

#[test]
fn queue_reports_progress_and_completion() {
    let (d, c, base) = setup!();
    let local = d.path().join("q.bin");
    std::fs::write(&local, vec![7u8; 500_000]).unwrap();
    let (tx, rx) = mpsc::channel::<TransferEvent>();
    let tx = std::sync::Mutex::new(tx);
    let q = TransferQueue::new(Arc::new(c), move |e| {
        let _ = tx.lock().unwrap().send(e);
    });
    let id = q.submit(TransferRequest {
        direction: Direction::Upload,
        local: local.clone(),
        remote: format!("{base}/q.bin"),
    });
    let mut last = None;
    while let Ok(e) = rx.recv_timeout(Duration::from_secs(20)) {
        assert_eq!(e.id, id);
        let end = e.state == TransferState::Done;
        last = Some(e);
        if end {
            break;
        }
    }
    let last = last.expect("events");
    assert_eq!(last.state, TransferState::Done);
    assert_eq!((last.done, last.total), (500_000, 500_000));
    assert_eq!(last.name, "q.bin");
}
