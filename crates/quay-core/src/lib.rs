//! Shared types, errors and traits used by every quay-term crate.
//!
//! This crate has no UI or I/O dependencies so it stays easy to test.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Errors shared across quay-term crates.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("session closed")]
    SessionClosed,
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Terminal size in character cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../../app/ui/src/lib/bindings/")]
pub struct TermSize {
    pub cols: u16,
    pub rows: u16,
}

/// How to authenticate an SSH connection.
///
/// Secrets live only in memory for the duration of a connect call; `Debug`
/// never prints them.
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(tag = "method", rename_all = "snake_case")]
#[ts(export, export_to = "../../../app/ui/src/lib/bindings/")]
pub enum SshAuth {
    Password {
        password: String,
    },
    /// A private key file, optionally protected by a passphrase.
    Key {
        path: String,
        passphrase: Option<String>,
    },
    /// Keys held by the running ssh-agent (or Pageant on Windows).
    Agent,
}

impl std::fmt::Debug for SshAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Password { .. } => f.write_str("Password(<redacted>)"),
            Self::Key { path, passphrase } => f
                .debug_struct("Key")
                .field("path", path)
                .field("passphrase", &passphrase.as_ref().map(|_| "<redacted>"))
                .finish(),
            Self::Agent => f.write_str("Agent"),
        }
    }
}

/// Where and as whom to connect over SSH.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/ui/src/lib/bindings/")]
pub struct SshTarget {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: SshAuth,
}

/// Sent to the UI when a server's host key is not yet trusted.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../app/ui/src/lib/bindings/")]
pub struct HostKeyPrompt {
    pub prompt_id: u32,
    pub host: String,
    pub port: u16,
    /// e.g. "ssh-ed25519"
    pub key_type: String,
    /// "SHA256:..." as shown by OpenSSH.
    pub fingerprint: String,
}

/// Sent to the UI when a session ends. `reason` is set when it ended abnormally.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../app/ui/src/lib/bindings/")]
pub struct SessionExit {
    pub id: u32,
    pub reason: Option<String>,
}

/// A running terminal session, either a local PTY or an SSH channel.
///
/// Implemented by `quay-pty` and `quay-ssh`. Output is delivered separately
/// through a reader owned by the implementation.
pub trait Session: Send {
    /// Send bytes (keystrokes, pasted text) to the session.
    fn write(&mut self, data: &[u8]) -> Result<()>;
    /// Resize the session's terminal.
    fn resize(&mut self, size: TermSize) -> Result<()>;
    /// Close the session. Further calls fail with [`Error::SessionClosed`].
    fn close(&mut self) -> Result<()>;
}

/// Basic information about the running app, shown in the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../../app/ui/src/lib/bindings/")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub os: String,
}

impl AppInfo {
    pub fn current(version: &str) -> Self {
        Self {
            name: "quay-term".to_string(),
            version: version.to_string(),
            os: std::env::consts::OS.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_reports_name_and_os() {
        let info = AppInfo::current("1.2.3");
        assert_eq!(info.name, "quay-term");
        assert_eq!(info.version, "1.2.3");
        assert_eq!(info.os, std::env::consts::OS);
    }
}
