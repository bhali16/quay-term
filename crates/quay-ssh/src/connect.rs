//! Open a TCP connection, verify the host key and authenticate.

use std::io::ErrorKind;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use quay_core::{SshAuth, SshTarget};
use socket2::{SockRef, TcpKeepalive};
use ssh2::{HashType, KeyboardInteractivePrompt, Prompt, Session};

use crate::error::{code, session_code, SshError};
use crate::known_hosts::{self, HostKeyInfo, HostKeyStatus};

/// A round of keyboard-interactive questions from the server (an OTP code,
/// a second password, a yes/no confirmation...).
#[derive(Debug, Clone)]
pub struct Challenge {
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
pub struct Challenger(Arc<ChallengeFn>);

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
            known_hosts: None,
            timeout: Duration::from_secs(15),
            keepalive: Duration::from_secs(15),
            challenger: None,
        }
    }
}

/// Connect, check the host key and authenticate. `verify` is called only for
/// hosts that are not in known_hosts yet; returning `true` trusts the key and
/// records it. A key that differs from a recorded one is always refused.
pub(crate) fn connect(
    opts: &ConnectOptions,
    verify: &mut dyn FnMut(&HostKeyInfo) -> bool,
) -> Result<Session, SshError> {
    let SshTarget {
        host, port, user, ..
    } = &opts.target;
    let tcp = open_tcp(host, *port, opts.timeout)?;

    let mut session = Session::new().map_err(|e| SshError::Other(e.to_string()))?;
    session.set_tcp_stream(tcp);
    session.set_timeout(opts.timeout.as_millis() as u32);
    session
        .handshake()
        .map_err(|e| SshError::Handshake(e.message().to_string()))?;

    check_host_key(&session, opts, verify)?;

    authenticate(&session, user, &opts.target.auth, opts.challenger.as_ref())?;
    if !session.authenticated() {
        return Err(SshError::Other("authentication did not complete".into()));
    }

    session.set_timeout(0);
    let secs = opts.keepalive.as_secs().clamp(1, u32::MAX as u64) as u32;
    session.set_keepalive(true, secs);
    Ok(session)
}

fn open_tcp(host: &str, port: u16, timeout: Duration) -> Result<TcpStream, SshError> {
    let addrs: Vec<_> = (host, port)
        .to_socket_addrs()
        .map_err(|_| SshError::Resolve {
            host: host.to_string(),
        })?
        .collect();
    if addrs.is_empty() {
        return Err(SshError::Resolve {
            host: host.to_string(),
        });
    }
    let mut last = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(tcp) => {
                let _ = tcp.set_nodelay(true);
                // Detect a silently dead peer (laptop lid, NAT drop) in about a
                // minute instead of the OS default of hours.
                let ka = TcpKeepalive::new()
                    .with_time(Duration::from_secs(30))
                    .with_interval(Duration::from_secs(10));
                let _ = SockRef::from(&tcp).set_tcp_keepalive(&ka);
                return Ok(tcp);
            }
            Err(e) => last = Some(e),
        }
    }
    let (host, port) = (host.to_string(), port);
    Err(match last.map(|e| (e.kind(), e)) {
        Some((ErrorKind::ConnectionRefused, _)) => SshError::Refused { host, port },
        Some((ErrorKind::TimedOut | ErrorKind::WouldBlock, _)) => SshError::Timeout { host, port },
        Some((_, e)) => SshError::Connect {
            host,
            port,
            detail: e.to_string(),
        },
        None => SshError::Resolve { host },
    })
}

fn check_host_key(
    session: &Session,
    opts: &ConnectOptions,
    verify: &mut dyn FnMut(&HostKeyInfo) -> bool,
) -> Result<(), SshError> {
    let SshTarget { host, port, .. } = &opts.target;
    let (key, key_type) = session
        .host_key()
        .ok_or_else(|| SshError::Handshake("the server sent no host key".into()))?;
    let digest = session
        .host_key_hash(HashType::Sha256)
        .ok_or_else(|| SshError::Handshake("could not fingerprint the host key".into()))?;
    let info = HostKeyInfo::new(key, key_type, digest);
    let path = opts
        .known_hosts
        .clone()
        .or_else(known_hosts::default_path)
        .ok_or_else(|| SshError::KnownHosts {
            path: PathBuf::from("~/.ssh/known_hosts"),
            detail: "could not find your home directory".into(),
        })?;

    match known_hosts::lookup(&path, host, *port, key)? {
        HostKeyStatus::Trusted => Ok(()),
        HostKeyStatus::Changed => Err(SshError::HostKeyChanged {
            host: host.clone(),
            port: *port,
            fingerprint: info.fingerprint,
            known_hosts: path,
        }),
        HostKeyStatus::Unknown => {
            if verify(&info) {
                known_hosts::trust(&path, host, *port, &info)
            } else {
                Err(SshError::HostKeyRejected)
            }
        }
    }
}

fn authenticate(
    session: &Session,
    user: &str,
    auth: &SshAuth,
    challenger: Option<&Challenger>,
) -> Result<(), SshError> {
    let first = authenticate_primary(session, user, auth, challenger);
    if session.authenticated() {
        return Ok(());
    }
    // A server that wants a second factor (key or password first, then a
    // code) reports the first step as done but the login as incomplete.
    if first.is_ok() {
        let offered = offered_methods(session, user)?;
        if offered.iter().any(|o| o == "keyboard-interactive") {
            let mut ask = Interactive::new(None, challenger);
            let done = session.userauth_keyboard_interactive(user, &mut ask);
            if ask.cancelled {
                return Err(SshError::AuthCancelled);
            }
            if done.is_ok() && session.authenticated() {
                return Ok(());
            }
            return Err(SshError::Other(
                "the server did not accept the second sign-in step".into(),
            ));
        }
    }
    first
}

fn offered_methods(session: &Session, user: &str) -> Result<Vec<String>, SshError> {
    Ok(session
        .auth_methods(user)
        .map_err(|e| SshError::Other(e.message().to_string()))?
        .split(',')
        .map(str::to_string)
        .collect())
}

fn authenticate_primary(
    session: &Session,
    user: &str,
    auth: &SshAuth,
    challenger: Option<&Challenger>,
) -> Result<(), SshError> {
    // Asking for methods also lets servers with "none" auth through.
    let offered = offered_methods(session, user)?;
    if session.authenticated() {
        return Ok(());
    }
    let has = |m: &str| offered.iter().any(|o| o == m);
    let missing = |wanted: &'static str| SshError::MethodNotOffered {
        wanted,
        offered: offered.clone(),
    };

    match auth {
        SshAuth::Password { password } => {
            if has("password") && session.userauth_password(user, password).is_ok() {
                return Ok(());
            }
            // Many servers expose password login only as keyboard-interactive.
            if has("keyboard-interactive") {
                let mut ask = Interactive::new(Some(password), challenger);
                let done = session.userauth_keyboard_interactive(user, &mut ask);
                if ask.cancelled {
                    return Err(SshError::AuthCancelled);
                }
                if done.is_ok() {
                    return Ok(());
                }
            }
            if has("password") || has("keyboard-interactive") {
                Err(SshError::PasswordRejected { user: user.into() })
            } else {
                Err(missing("password"))
            }
        }
        SshAuth::Key { path, passphrase } => {
            if !has("publickey") {
                return Err(missing("public key"));
            }
            let path = expand_tilde(path);
            std::fs::metadata(&path).map_err(|e| SshError::KeyFile {
                path: path.clone(),
                detail: match e.kind() {
                    ErrorKind::NotFound => "file not found".into(),
                    _ => e.to_string(),
                },
            })?;
            session
                .userauth_pubkey_file(user, None, &path, passphrase.as_deref())
                .map_err(|e| match session_code(&e) {
                    Some(code::AUTHENTICATION_FAILED | code::PUBLICKEY_UNVERIFIED) => {
                        SshError::KeyRejected { user: user.into() }
                    }
                    Some(code::FILE) => SshError::KeyFile {
                        path: path.clone(),
                        detail: "wrong passphrase, or a key format this build cannot read".into(),
                    },
                    _ => SshError::KeyFile {
                        path: path.clone(),
                        detail: e.message().to_string(),
                    },
                })
        }
        SshAuth::Agent => {
            if !has("publickey") {
                return Err(missing("public key"));
            }
            #[cfg(unix)]
            if !std::env::var_os("SSH_AUTH_SOCK").is_some_and(|v| !v.is_empty()) {
                return Err(SshError::AgentUnavailable(
                    "SSH_AUTH_SOCK is not set, so no agent is running".into(),
                ));
            }
            session
                .userauth_agent(user)
                .map_err(|e| match session_code(&e) {
                    Some(code::AUTHENTICATION_FAILED | code::PUBLICKEY_UNVERIFIED) => {
                        SshError::AgentRejected { user: user.into() }
                    }
                    _ => SshError::AgentUnavailable(e.message().to_string()),
                })
        }
    }
}

/// Answers keyboard-interactive prompts: the stored password goes to the
/// first lone "password" prompt; everything else is put to the user.
struct Interactive<'a> {
    password: Option<&'a str>,
    password_used: bool,
    challenger: Option<&'a Challenger>,
    cancelled: bool,
}

impl<'a> Interactive<'a> {
    fn new(password: Option<&'a str>, challenger: Option<&'a Challenger>) -> Self {
        Self {
            password,
            password_used: false,
            challenger,
            cancelled: false,
        }
    }
}

impl KeyboardInteractivePrompt for Interactive<'_> {
    fn prompt(&mut self, _user: &str, instructions: &str, prompts: &[Prompt<'_>]) -> Vec<String> {
        let blank = || vec![String::new(); prompts.len()];
        if prompts.is_empty() {
            return Vec::new();
        }
        if let (Some(pw), false, [only]) = (self.password, self.password_used, prompts) {
            if !only.echo && only.text.to_lowercase().contains("password") {
                self.password_used = true;
                return vec![pw.to_string()];
            }
        }
        let Some(ask) = self.challenger else {
            return blank();
        };
        let challenge = Challenge {
            name: String::new(),
            instructions: instructions.to_string(),
            prompts: prompts
                .iter()
                .map(|p| ChallengePrompt {
                    text: p.text.to_string(),
                    echo: p.echo,
                })
                .collect(),
        };
        match (ask.0)(&challenge) {
            Some(answers) if answers.len() == prompts.len() => answers,
            _ => {
                self.cancelled = true;
                blank()
            }
        }
    }
}

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
