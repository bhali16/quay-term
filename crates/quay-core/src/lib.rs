//! Shared types, errors and traits used by every quay-term crate.
//!
//! This crate has no UI or I/O dependencies so it stays easy to test.

use serde::Serialize;
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
