//! Host-key verification against an OpenSSH `known_hosts` file.
//!
//! Lookups go through russh's parser (so hashed host names work); new
//! entries are appended as one plain line so the user's existing file is
//! never rewritten.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use russh::keys::known_hosts::known_host_keys_path;
use russh::keys::{HashAlg, PublicKey};

use crate::error::SshError;

/// A server's host key, as shown to the user in the trust prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKeyInfo {
    /// The host presenting the key, which matters when connecting through jump hosts.
    pub host: String,
    pub port: u16,
    /// e.g. "ssh-ed25519"
    pub key_type: String,
    /// "SHA256:..." as printed by OpenSSH.
    pub fingerprint: String,
    pub(crate) key: PublicKey,
}

impl HostKeyInfo {
    pub(crate) fn new(key: PublicKey) -> Self {
        Self {
            host: String::new(),
            port: 0,
            key_type: key.algorithm().to_string(),
            fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
            key,
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

/// Host pattern as written in known_hosts: plain for port 22, `[host]:port` otherwise.
fn pattern(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

/// The keys `path` records for `host:port`. A missing file records none.
pub(crate) fn recorded_keys(
    path: &Path,
    host: &str,
    port: u16,
) -> Result<Vec<PublicKey>, SshError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    known_host_keys_path(host, port, path)
        .map(|keys| keys.into_iter().map(|(_, k)| k).collect())
        .map_err(|e| SshError::KnownHosts {
            path: path.to_path_buf(),
            detail: e.to_string(),
        })
}

/// Compare `key` with what `path` says about `host:port`. A missing file
/// means the host is unknown. A host recorded with only other keys, even of
/// another type, counts as changed: its key must not be silently replaced.
pub fn lookup(
    path: &Path,
    host: &str,
    port: u16,
    key: &PublicKey,
) -> Result<HostKeyStatus, SshError> {
    let recorded = recorded_keys(path, host, port)?;
    Ok(if recorded.is_empty() {
        HostKeyStatus::Unknown
    } else if recorded.iter().any(|k| k == key) {
        HostKeyStatus::Trusted
    } else {
        HostKeyStatus::Changed
    })
}

/// Append a trusted key for `host:port` to `path`, creating the file (and
/// `~/.ssh`-style parent directory, mode 0700) if needed.
pub fn trust(path: &Path, host: &str, port: u16, info: &HostKeyInfo) -> Result<(), SshError> {
    let kh_err = |detail: String| SshError::KnownHosts {
        path: path.to_path_buf(),
        detail,
    };
    if let Some(dir) = path.parent().filter(|d| !d.exists()) {
        fs::create_dir_all(dir).map_err(|e| kh_err(e.to_string()))?;
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
        .map_err(|e| kh_err(e.to_string()))?;
    let key = info.key.to_openssh().map_err(|e| kh_err(e.to_string()))?;
    let line = format!(
        "{}{} {}\n",
        if needs_newline { "\n" } else { "" },
        pattern(host, port),
        key
    );
    file.write_all(line.as_bytes())
        .map_err(|e| kh_err(e.to_string()))
}
