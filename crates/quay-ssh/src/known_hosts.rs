//! Host-key verification against an OpenSSH `known_hosts` file.
//!
//! Lookups go through libssh2's parser (so hashed host names work); new
//! entries are appended as one plain line so the user's existing file is
//! never rewritten.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use base64::Engine;
use ssh2::{CheckResult, HostKeyType, KnownHostFileKind, Session};

use crate::error::SshError;

/// A server's host key, as shown to the user in the trust prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKeyInfo {
    /// e.g. "ssh-ed25519"
    pub key_type: String,
    /// "SHA256:..." as printed by OpenSSH.
    pub fingerprint: String,
    pub(crate) key: Vec<u8>,
}

impl HostKeyInfo {
    pub(crate) fn new(key: &[u8], key_type: HostKeyType, sha256: &[u8]) -> Self {
        Self {
            key_type: key_type_name(key_type).to_string(),
            fingerprint: fingerprint(sha256),
            key: key.to_vec(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyStatus {
    /// The key is in known_hosts.
    Trusted,
    /// The host has no entry yet.
    Unknown,
    /// The host has an entry with a different key.
    Changed,
}

/// `~/.ssh/known_hosts`
pub fn default_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".ssh").join("known_hosts"))
}

pub(crate) fn key_type_name(t: HostKeyType) -> &'static str {
    match t {
        HostKeyType::Rsa => "ssh-rsa",
        HostKeyType::Dss => "ssh-dss",
        HostKeyType::Ecdsa256 => "ecdsa-sha2-nistp256",
        HostKeyType::Ecdsa384 => "ecdsa-sha2-nistp384",
        HostKeyType::Ecdsa521 => "ecdsa-sha2-nistp521",
        HostKeyType::Ed25519 => "ssh-ed25519",
        _ => "unknown",
    }
}

/// OpenSSH-style fingerprint from a raw SHA-256 digest of the public key.
pub(crate) fn fingerprint(sha256: &[u8]) -> String {
    format!("SHA256:{}", STANDARD_NO_PAD.encode(sha256))
}

/// Host pattern as written in known_hosts: plain for port 22, `[host]:port` otherwise.
fn pattern(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

/// Compare `key` with what `path` says about `host:port`. A missing file
/// means the host is unknown.
pub fn lookup(path: &Path, host: &str, port: u16, key: &[u8]) -> Result<HostKeyStatus, SshError> {
    let kh_err = |detail: String| SshError::KnownHosts {
        path: path.to_path_buf(),
        detail,
    };
    // Any session works as a parser handle; no connection is needed.
    let session = Session::new().map_err(|e| kh_err(e.to_string()))?;
    let mut hosts = session.known_hosts().map_err(|e| kh_err(e.to_string()))?;
    match hosts.read_file(path, KnownHostFileKind::OpenSSH) {
        Ok(_) => {}
        Err(_) if !path.exists() => return Ok(HostKeyStatus::Unknown),
        Err(e) => return Err(kh_err(e.to_string())),
    }
    Ok(match hosts.check_port(host, port, key) {
        CheckResult::Match => HostKeyStatus::Trusted,
        CheckResult::NotFound => HostKeyStatus::Unknown,
        CheckResult::Mismatch => HostKeyStatus::Changed,
        CheckResult::Failure => return Err(kh_err("lookup failed".to_string())),
    })
}

/// Append a trusted key for `host:port` to `path`, creating the file (and
/// `~/.ssh`-style parent directory, mode 0700) if needed.
pub fn trust(path: &Path, host: &str, port: u16, info: &HostKeyInfo) -> Result<(), SshError> {
    let kh_err = |e: std::io::Error| SshError::KnownHosts {
        path: path.to_path_buf(),
        detail: e.to_string(),
    };
    if let Some(dir) = path.parent().filter(|d| !d.exists()) {
        fs::create_dir_all(dir).map_err(kh_err)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
        }
    }
    let needs_newline = fs::read(path)
        .map(|b| !b.is_empty() && !b.ends_with(b"\n"))
        .unwrap_or(false);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(kh_err)?;
    let line = format!(
        "{}{} {} {}\n",
        if needs_newline { "\n" } else { "" },
        pattern(host, port),
        info.key_type,
        STANDARD.encode(&info.key)
    );
    file.write_all(line.as_bytes()).map_err(kh_err)
}
