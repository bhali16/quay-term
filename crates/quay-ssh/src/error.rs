//! Errors with messages meant to be shown to the user as-is.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum SshError {
    #[error("Could not find host \"{host}\". Check the name and your network connection.")]
    Resolve { host: String },
    #[error("Connection to {host}:{port} was refused. Is an SSH server running on that port?")]
    Refused { host: String, port: u16 },
    #[error(
        "Timed out connecting to {host}:{port}. The host may be down or blocked by a firewall."
    )]
    Timeout { host: String, port: u16 },
    #[error("Could not connect to {host}:{port}: {detail}")]
    Connect {
        host: String,
        port: u16,
        detail: String,
    },
    #[error("SSH handshake with the server failed: {0}")]
    Handshake(String),
    #[error(
        "WARNING: the host key for {host}:{port} has CHANGED (now {fingerprint}). \
         Someone may be intercepting the connection, or the server was reinstalled. \
         Connection aborted. If the change is expected, remove the old entry from {} and reconnect.",
        .known_hosts.display()
    )]
    HostKeyChanged {
        host: String,
        port: u16,
        fingerprint: String,
        known_hosts: PathBuf,
    },
    #[error("The host key was not trusted, so the connection was cancelled.")]
    HostKeyRejected,
    #[error("Sign-in was cancelled.")]
    AuthCancelled,
    #[error("Could not check known hosts ({}): {detail}", .path.display())]
    KnownHosts { path: PathBuf, detail: String },
    #[error("Authentication failed: the server rejected the password for \"{user}\".")]
    PasswordRejected { user: String },
    #[error("Authentication failed: the server rejected the key for \"{user}\".")]
    KeyRejected { user: String },
    #[error("Could not use the key file {}: {detail}", .path.display())]
    KeyFile { path: PathBuf, detail: String },
    #[error("Authentication failed: no key held by ssh-agent was accepted for \"{user}\".")]
    AgentRejected { user: String },
    #[error("Could not talk to ssh-agent: {0}")]
    AgentUnavailable(String),
    #[error(
        "The server does not allow {wanted} authentication (it offers: {}).",
        offered.join(", ")
    )]
    MethodNotOffered {
        wanted: &'static str,
        offered: Vec<String>,
    },
    /// A failure while going through a jump host, named so the user knows which hop.
    #[error("Jump host {host}:{port}: {source}")]
    JumpHost {
        host: String,
        port: u16,
        source: Box<SshError>,
    },
    #[error("Could not open a shell on the server: {0}")]
    Channel(String),
    #[error("SSH error: {0}")]
    Other(String),
}

impl From<SshError> for quay_core::Error {
    fn from(e: SshError) -> Self {
        quay_core::Error::Other(e.to_string())
    }
}
