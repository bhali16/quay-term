//! Options and prompts shared by the SSH implementations.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use quay_core::SshTarget;

/// A round of keyboard-interactive questions from the server (an OTP code,
/// a second password, a yes/no confirmation...).
#[derive(Debug, Clone)]
pub struct Challenge {
    /// The host asking, which matters when connecting through jump hosts.
    pub host: String,
    pub port: u16,
    pub name: String,
    pub instructions: String,
    pub prompts: Vec<ChallengePrompt>,
}

#[derive(Debug, Clone)]
pub struct ChallengePrompt {
    pub text: String,
    /// `false` for secrets: the answer should not be shown while typing.
    pub echo: bool,
}

type ChallengeFn = dyn Fn(&Challenge) -> Option<Vec<String>> + Send + Sync;

/// Asks the user to answer a [`Challenge`]; `None` means they cancelled.
#[derive(Clone)]
pub struct Challenger(pub(crate) Arc<ChallengeFn>);

impl Challenger {
    pub fn new(f: impl Fn(&Challenge) -> Option<Vec<String>> + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }
}

impl std::fmt::Debug for Challenger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Challenger")
    }
}

/// Everything needed to open a connection.
#[derive(Debug, Clone)]
pub struct ConnectOptions {
    pub target: SshTarget,
    /// Jump hosts to go through, first hop first. Each signs in with its own
    /// credentials; the target is reached through the last one.
    pub jumps: Vec<SshTarget>,
    /// known_hosts file to check and extend; `None` uses `~/.ssh/known_hosts`.
    pub known_hosts: Option<PathBuf>,
    /// Applies to the TCP connect and, separately, to the handshake and auth.
    pub timeout: Duration,
    /// How often an SSH keepalive is sent on an otherwise idle connection.
    pub keepalive: Duration,
    /// Answers keyboard-interactive questions the stored password cannot,
    /// such as one-time codes. Without it, those logins fail.
    pub challenger: Option<Challenger>,
}

impl ConnectOptions {
    pub fn new(target: SshTarget) -> Self {
        Self {
            target,
            jumps: Vec::new(),
            known_hosts: None,
            timeout: Duration::from_secs(15),
            keepalive: Duration::from_secs(15),
            challenger: None,
        }
    }
}

/// Expand a leading `~` to the user's home directory.
pub(crate) fn expand_tilde(path: &str) -> PathBuf {
    let home = || std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    match path.strip_prefix("~") {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => match home() {
            Some(h) => Path::new(&h).join(rest.trim_start_matches(['/', '\\'])),
            None => PathBuf::from(path),
        },
        _ => PathBuf::from(path),
    }
}
