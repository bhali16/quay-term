//! The russh-based SSH connection: TCP, host-key check, authentication.
//!
//! russh is async; the rest of quay-term is not. [`connect`] hides that: the
//! connection is built on a shared tokio runtime while the calling thread
//! answers the questions it raises (trust this host key? what is the OTP?),
//! so callers keep the plain blocking closures they always had.

use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use quay_core::{SshAuth, SshTarget};
use russh::client::AuthResult;
use russh::client::{self, Handle, KeyboardInteractiveAuthResponse};
use russh::keys::agent::client::AgentClient;
use russh::keys::{
    load_secret_key, Algorithm, HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate,
};
use russh::MethodKind;
use russh::Preferred;
use socket2::{SockRef, TcpKeepalive};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::runtime::Runtime;
use tokio::sync::oneshot;

use crate::error::SshError;
use crate::known_hosts::{self, HostKeyInfo, HostKeyStatus};
use crate::options::{expand_tilde, Challenge, ChallengePrompt, ConnectOptions};

/// The runtime every SSH connection runs on.
pub(crate) fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("quay-ssh")
            .enable_all()
            .build()
            .expect("could not start the SSH runtime")
    })
}

/// A questions from the connection task to the calling thread.
enum Event {
    TrustHost(HostKeyInfo, oneshot::Sender<bool>),
    Challenge(Challenge, oneshot::Sender<Option<Vec<String>>>),
    Done(Result<Connection, SshError>),
}

/// An authenticated SSH connection. Shells, SFTP and forwards open channels on it.
pub struct Connection {
    pub(crate) handle: Handle<Handler>,
}

impl Connection {
    /// Connect, check the host key and authenticate. Blocks, so call it from a
    /// worker thread. See [`ConnectOptions`] for what can be configured.
    pub fn connect(
        opts: &ConnectOptions,
        verify: &mut dyn FnMut(&HostKeyInfo) -> bool,
    ) -> Result<Self, SshError> {
        connect(opts, verify)
    }

    /// Open a subsystem such as `sftp` on its own channel and return it as a
    /// byte stream. Any number of channels share the one connection.
    pub async fn open_subsystem(
        &self,
        name: &str,
    ) -> Result<impl AsyncRead + AsyncWrite + Unpin + Send + 'static, SshError> {
        let err = |e: russh::Error| SshError::Channel(e.to_string());
        let channel = self.handle.channel_open_session().await.map_err(err)?;
        channel.request_subsystem(true, name).await.map_err(err)?;
        Ok(channel.into_stream())
    }
}

/// Run a future to completion on the SSH runtime, from a plain thread.
pub fn block_on<F: std::future::Future>(f: F) -> F::Output {
    runtime().block_on(f)
}

pub(crate) struct Handler {
    host: String,
    port: u16,
    known_hosts: PathBuf,
    events: mpsc::Sender<Event>,
    /// Why the key was refused, so the caller gets a real error and not "unknown key".
    refusal: Arc<Mutex<Option<SshError>>>,
}

impl Handler {
    fn refuse(&self, why: SshError) -> Result<bool, russh::Error> {
        *self.refusal.lock().unwrap() = Some(why);
        Ok(false)
    }
}

impl client::Handler for Handler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = match key {
            PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
            PublicKeyOrCertificate::Certificate(c) => c.public_key().clone().into(),
        };
        let info = HostKeyInfo::new(key);
        match known_hosts::lookup(&self.known_hosts, &self.host, self.port, &info.key) {
            Err(e) => self.refuse(e),
            Ok(HostKeyStatus::Trusted) => Ok(true),
            Ok(HostKeyStatus::Changed) => self.refuse(SshError::HostKeyChanged {
                host: self.host.clone(),
                port: self.port,
                fingerprint: info.fingerprint,
                known_hosts: self.known_hosts.clone(),
            }),
            Ok(HostKeyStatus::Unknown) => {
                let (tx, rx) = oneshot::channel();
                let _ = self.events.send(Event::TrustHost(info.clone(), tx));
                if rx.await.unwrap_or(false) {
                    match known_hosts::trust(&self.known_hosts, &self.host, self.port, &info) {
                        Ok(()) => Ok(true),
                        Err(e) => self.refuse(e),
                    }
                } else {
                    self.refuse(SshError::HostKeyRejected)
                }
            }
        }
    }
}

/// Connect, check the host key and authenticate. `verify` is called only for
/// hosts that are not in known_hosts yet; returning `true` trusts the key and
/// records it. A key that differs from a recorded one is always refused.
pub(crate) fn connect(
    opts: &ConnectOptions,
    verify: &mut dyn FnMut(&HostKeyInfo) -> bool,
) -> Result<Connection, SshError> {
    let SshTarget { host, port, .. } = &opts.target;
    let known_hosts = opts
        .known_hosts
        .clone()
        .or_else(known_hosts::default_path)
        .ok_or_else(|| SshError::KnownHosts {
            path: PathBuf::from("~/.ssh/known_hosts"),
            detail: "could not find your home directory".into(),
        })?;

    let (events, inbox) = mpsc::channel();
    let refusal = Arc::new(Mutex::new(None));
    let task = runtime().spawn({
        let (opts, events, refusal) = (opts.clone(), events.clone(), refusal.clone());
        async move {
            let done = connect_async(&opts, known_hosts, events.clone(), refusal).await;
            let _ = events.send(Event::Done(done));
        }
    });
    drop(events);

    // Every event restarts the clock, so time spent answering a prompt does
    // not count against a server that is merely slow.
    let patience = opts.timeout + Duration::from_secs(5);
    loop {
        match inbox.recv_timeout(patience) {
            Ok(Event::TrustHost(info, reply)) => {
                let _ = reply.send(verify(&info));
            }
            Ok(Event::Challenge(challenge, reply)) => {
                let answers = match &opts.challenger {
                    Some(ask) => (ask.0)(&challenge),
                    // Nobody to ask: let the server reject empty answers.
                    None => Some(vec![String::new(); challenge.prompts.len()]),
                };
                let _ = reply.send(answers);
            }
            Ok(Event::Done(result)) => return result,
            Err(RecvTimeoutError::Timeout) => {
                task.abort();
                return Err(SshError::Timeout {
                    host: host.clone(),
                    port: *port,
                });
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(SshError::Other(
                    "the connection task ended unexpectedly".into(),
                ))
            }
        }
    }
}

async fn connect_async(
    opts: &ConnectOptions,
    known_hosts: PathBuf,
    events: mpsc::Sender<Event>,
    refusal: Arc<Mutex<Option<SshError>>>,
) -> Result<Connection, SshError> {
    let SshTarget {
        host, port, user, ..
    } = &opts.target;
    let tcp = open_tcp(host, *port, opts.timeout).await?;

    let config = client::Config {
        preferred: preferred_for(&known_hosts, host, *port)?,
        keepalive_interval: Some(opts.keepalive),
        keepalive_max: 3,
        nodelay: true,
        ..Default::default()
    };
    let handler = Handler {
        host: host.clone(),
        port: *port,
        known_hosts,
        events: events.clone(),
        refusal: refusal.clone(),
    };
    let mut handle = client::connect_stream(Arc::new(config), tcp, handler)
        .await
        .map_err(|e| {
            refusal
                .lock()
                .unwrap()
                .take()
                .unwrap_or_else(|| SshError::Handshake(e.to_string()))
        })?;

    authenticate(&mut handle, user, &opts.target.auth, &events).await?;
    Ok(Connection { handle })
}

/// Like OpenSSH: ask for the host-key types already recorded for this host
/// first, so a server that has such a key presents it instead of another one
/// that would look like a change.
fn preferred_for(
    known_hosts: &std::path::Path,
    host: &str,
    port: u16,
) -> Result<Preferred, SshError> {
    let family = |a: &Algorithm| match a {
        Algorithm::Rsa { .. } => "rsa".to_string(),
        other => other.to_string(),
    };
    let recorded: Vec<String> = known_hosts::recorded_keys(known_hosts, host, port)?
        .iter()
        .map(|k| family(&k.algorithm()))
        .collect();
    let (mut first, rest): (Vec<Algorithm>, Vec<Algorithm>) = Preferred::DEFAULT
        .key
        .iter()
        .cloned()
        .partition(|a| recorded.contains(&family(a)));
    first.extend(rest);
    Ok(Preferred {
        key: first.into(),
        ..Preferred::DEFAULT
    })
}

async fn open_tcp(host: &str, port: u16, timeout: Duration) -> Result<TcpStream, SshError> {
    let resolve_err = || SshError::Resolve {
        host: host.to_string(),
    };
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| resolve_err())?
        .collect();
    if addrs.is_empty() {
        return Err(resolve_err());
    }
    let attempt = async {
        let mut last = None;
        for addr in addrs {
            match TcpStream::connect(addr).await {
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
        Err(last)
    };
    let (host, port) = (host.to_string(), port);
    match tokio::time::timeout(timeout, attempt).await {
        Err(_) => Err(SshError::Timeout { host, port }),
        Ok(Ok(tcp)) => Ok(tcp),
        Ok(Err(Some(e))) => Err(match e.kind() {
            ErrorKind::ConnectionRefused => SshError::Refused { host, port },
            ErrorKind::TimedOut | ErrorKind::WouldBlock => SshError::Timeout { host, port },
            _ => SshError::Connect {
                host,
                port,
                detail: e.to_string(),
            },
        }),
        Ok(Err(None)) => Err(SshError::Resolve { host }),
    }
}

type Offered = Vec<MethodKind>;

fn rejected(result: AuthResult) -> Result<Offered, ()> {
    match result {
        AuthResult::Success => Err(()),
        AuthResult::Failure {
            remaining_methods, ..
        } => Ok(remaining_methods.iter().copied().collect()),
    }
}

fn partial(result: &AuthResult) -> bool {
    matches!(
        result,
        AuthResult::Failure {
            partial_success: true,
            ..
        }
    )
}

fn method_name(m: &MethodKind) -> String {
    <&'static str>::from(m).to_string()
}

async fn authenticate(
    h: &mut Handle<Handler>,
    user: &str,
    auth: &SshAuth,
    events: &mpsc::Sender<Event>,
) -> Result<(), SshError> {
    let other = |e: russh::Error| SshError::Other(e.to_string());

    // Asking with "none" also lets servers without authentication through,
    // and tells us which methods are on offer.
    let first = h.authenticate_none(user).await.map_err(other)?;
    let mut offered = match rejected(first) {
        Ok(o) => o,
        Err(()) => return Ok(()),
    };
    let missing = |wanted: &'static str, offered: &Offered| SshError::MethodNotOffered {
        wanted,
        offered: offered.iter().map(method_name).collect(),
    };

    // The result of the first step: done, or the server wants a second one.
    let mut needs_second_factor = false;
    match auth {
        SshAuth::Password { password } => {
            if offered.contains(&MethodKind::Password) {
                let r = h
                    .authenticate_password(user, password)
                    .await
                    .map_err(other)?;
                if r.success() {
                    return Ok(());
                }
                needs_second_factor = partial(&r);
                offered = rejected(r).unwrap_or_default();
            }
            // Many servers expose password login only as keyboard-interactive.
            if needs_second_factor || offered.contains(&MethodKind::KeyboardInteractive) {
                if keyboard_interactive(h, user, Some(password), events).await? {
                    return Ok(());
                }
                return Err(SshError::PasswordRejected { user: user.into() });
            }
            return Err(if offered.contains(&MethodKind::Password) {
                SshError::PasswordRejected { user: user.into() }
            } else {
                missing("password", &offered)
            });
        }
        SshAuth::Key { path, passphrase } => {
            if !offered.contains(&MethodKind::PublicKey) {
                return Err(missing("public key", &offered));
            }
            let path = expand_tilde(path);
            std::fs::metadata(&path).map_err(|e| SshError::KeyFile {
                path: path.clone(),
                detail: match e.kind() {
                    ErrorKind::NotFound => "file not found".into(),
                    _ => e.to_string(),
                },
            })?;
            let key = load_secret_key(&path, passphrase.as_deref()).map_err(|e| {
                use russh::keys::Error as KeyError;
                SshError::KeyFile {
                    path: path.clone(),
                    detail: match e {
                        KeyError::KeyIsEncrypted | KeyError::SshKey(_) => {
                            "wrong passphrase, or a key format this build cannot read".into()
                        }
                        other => other.to_string(),
                    },
                }
            })?;
            let hash = h.best_supported_rsa_hash().await.map_err(other)?.flatten();
            let r = h
                .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
                .await
                .map_err(other)?;
            if r.success() {
                return Ok(());
            }
            needs_second_factor = partial(&r);
            if !needs_second_factor {
                return Err(SshError::KeyRejected { user: user.into() });
            }
        }
        SshAuth::Agent => {
            if !offered.contains(&MethodKind::PublicKey) {
                return Err(missing("public key", &offered));
            }
            let mut agent = connect_agent().await?;
            let identities = agent
                .request_identities()
                .await
                .map_err(|e| SshError::AgentUnavailable(e.to_string()))?;
            let hash: Option<HashAlg> = h.best_supported_rsa_hash().await.map_err(other)?.flatten();
            for id in identities {
                let r = match h
                    .authenticate_publickey_with(
                        user,
                        id.public_key().into_owned(),
                        hash,
                        &mut agent,
                    )
                    .await
                {
                    Ok(r) => r,
                    // The agent declined to sign with this one; try the next.
                    Err(_) => continue,
                };
                if r.success() {
                    return Ok(());
                }
                if partial(&r) {
                    needs_second_factor = true;
                    break;
                }
            }
            if !needs_second_factor {
                return Err(SshError::AgentRejected { user: user.into() });
            }
        }
    }

    // First step accepted, login incomplete: a code or second password follows.
    if needs_second_factor && keyboard_interactive(h, user, None, events).await? {
        return Ok(());
    }
    Err(SshError::Other(
        "the server did not accept the second sign-in step".into(),
    ))
}

#[cfg(unix)]
async fn connect_agent(
) -> Result<AgentClient<Box<dyn russh::keys::agent::client::AgentStream + Send + Unpin>>, SshError>
{
    if !std::env::var_os("SSH_AUTH_SOCK").is_some_and(|v| !v.is_empty()) {
        return Err(SshError::AgentUnavailable(
            "SSH_AUTH_SOCK is not set, so no agent is running".into(),
        ));
    }
    AgentClient::connect_env()
        .await
        .map(AgentClient::dynamic)
        .map_err(|e| SshError::AgentUnavailable(e.to_string()))
}

#[cfg(windows)]
async fn connect_agent(
) -> Result<AgentClient<Box<dyn russh::keys::agent::client::AgentStream + Send + Unpin>>, SshError>
{
    // The Windows OpenSSH agent first, then Pageant.
    if let Ok(a) = AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await {
        return Ok(a.dynamic());
    }
    AgentClient::connect_pageant()
        .await
        .map(AgentClient::dynamic)
        .map_err(|e| SshError::AgentUnavailable(e.to_string()))
}

/// Run keyboard-interactive auth. A lone "password" prompt is answered with
/// `password`; anything else goes to the user. `Ok(false)` means rejected.
async fn keyboard_interactive(
    h: &mut Handle<Handler>,
    user: &str,
    password: Option<&str>,
    events: &mpsc::Sender<Event>,
) -> Result<bool, SshError> {
    let other = |e: russh::Error| SshError::Other(e.to_string());
    let mut password = password;
    let mut response = h
        .authenticate_keyboard_interactive_start(user, None)
        .await
        .map_err(other)?;
    loop {
        match response {
            KeyboardInteractiveAuthResponse::Success => return Ok(true),
            KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
            KeyboardInteractiveAuthResponse::InfoRequest {
                name,
                instructions,
                prompts,
            } => {
                let answers = match (&password, prompts.as_slice()) {
                    (Some(pw), [only])
                        if !only.echo && only.prompt.to_lowercase().contains("password") =>
                    {
                        let answers = vec![pw.to_string()];
                        password = None; // only once: a second ask means it was wrong
                        answers
                    }
                    _ if prompts.is_empty() => Vec::new(),
                    _ => {
                        let (tx, rx) = oneshot::channel();
                        let challenge = Challenge {
                            name,
                            instructions,
                            prompts: prompts
                                .iter()
                                .map(|p| ChallengePrompt {
                                    text: p.prompt.clone(),
                                    echo: p.echo,
                                })
                                .collect(),
                        };
                        let _ = events.send(Event::Challenge(challenge, tx));
                        match rx.await {
                            Ok(Some(a)) if a.len() == prompts.len() => a,
                            _ => return Err(SshError::AuthCancelled),
                        }
                    }
                };
                response = h
                    .authenticate_keyboard_interactive_respond(answers)
                    .await
                    .map_err(other)?;
            }
        }
    }
}
