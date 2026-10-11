//! Errors with messages meant to be shown to the user as-is.

use quay_ssh::SshError;

#[derive(Debug, thiserror::Error)]
pub enum SftpError {
    #[error(transparent)]
    Connect(#[from] SshError),
    #[error("Could not start the SFTP subsystem: {0}")]
    Subsystem(String),
    #[error("{op} \"{path}\" failed: {detail}")]
    Op {
        op: &'static str,
        path: String,
        detail: String,
    },
    #[error("Local file error for \"{path}\": {detail}")]
    Local { path: String, detail: String },
    #[error("The transfer was cancelled.")]
    Cancelled,
}

impl SftpError {
    pub(crate) fn op(op: &'static str, path: &str, e: ssh2::Error) -> Self {
        // SFTP status codes: 2 = no such file, 3 = permission denied.
        let detail = match e.code() {
            ssh2::ErrorCode::SFTP(2) => "no such file or directory".to_string(),
            ssh2::ErrorCode::SFTP(3) => "permission denied".to_string(),
            _ => e.message().to_string(),
        };
        Self::Op {
            op,
            path: path.to_string(),
            detail,
        }
    }

    pub(crate) fn local(path: &std::path::Path, e: std::io::Error) -> Self {
        Self::Local {
            path: path.display().to_string(),
            detail: e.to_string(),
        }
    }
}
