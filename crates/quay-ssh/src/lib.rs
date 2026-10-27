//! SSH client for quay-term, built on libssh2 (`ssh2`).
//!
//! [`SshSession::connect`] opens a TCP connection, verifies the host key
//! against `known_hosts` (asking the caller about unknown hosts),
//! authenticates with a password, key file or ssh-agent, and starts an
//! interactive shell that implements [`quay_core::Session`].

mod client;
mod error;
pub mod known_hosts;
mod legacy;
mod options;
mod session;

pub use client::Connection;
pub use error::SshError;
pub use known_hosts::{HostKeyInfo, HostKeyStatus};
pub use options::{Challenge, ChallengePrompt, Challenger, ConnectOptions};
pub use session::SshSession;
pub use ssh2::Session as RawSession;

/// Connect, verify the host key and authenticate, returning the raw libssh2
/// session (blocking mode). Used by SFTP, which opens its own connection.
pub fn connect_raw(
    opts: &ConnectOptions,
    verify: &mut dyn FnMut(&HostKeyInfo) -> bool,
) -> Result<RawSession, SshError> {
    legacy::connect(opts, verify)
}

#[cfg(test)]
mod tests {
    use super::known_hosts::*;
    use crate::options::expand_tilde;
    use russh::keys::PublicKey;

    // Generated with ssh-keygen; the fingerprint is what `ssh-keygen -lf` prints for KEY_A.
    const KEY_A: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDVLGU4gPKuK/1jGm4UbtDTiEpLO02iDCzj6Rf+ZodXv";
    const KEY_B: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIA29xIZefSbbpEoLSF9gi1OFoTpwNiNDEw1kCkyo90mW";

    fn key(s: &str) -> PublicKey {
        PublicKey::from_openssh(s).unwrap()
    }

    fn info(s: &str) -> HostKeyInfo {
        HostKeyInfo::new(key(s))
    }

    #[test]
    fn fingerprint_matches_openssh_format() {
        let i = info(KEY_A);
        assert_eq!(i.key_type, "ssh-ed25519");
        assert_eq!(
            i.fingerprint,
            "SHA256:n3qC5vsn57hcBlePbdItGgxTOIyuH0Jm+u7rGMd2DB8"
        );
    }

    #[test]
    fn unknown_then_trusted_then_changed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("known_hosts");
        let (key_a, key_b) = (key(KEY_A), key(KEY_B));

        // Missing file: unknown, and nothing is created by a lookup.
        assert_eq!(
            lookup(&path, "example.com", 22, &key_a).unwrap(),
            HostKeyStatus::Unknown
        );
        assert!(!path.exists());

        trust(&path, "example.com", 22, &info(KEY_A)).unwrap();
        assert_eq!(
            lookup(&path, "example.com", 22, &key_a).unwrap(),
            HostKeyStatus::Trusted
        );
        assert_eq!(
            lookup(&path, "example.com", 22, &key_b).unwrap(),
            HostKeyStatus::Changed
        );
        assert_eq!(
            lookup(&path, "other.com", 22, &key_a).unwrap(),
            HostKeyStatus::Unknown
        );
    }

    #[test]
    fn non_default_ports_are_tracked_separately() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        let key_a = key(KEY_A);
        trust(&path, "example.com", 2222, &info(KEY_A)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.starts_with("[example.com]:2222 ssh-ed25519 "),
            "{text}"
        );
        assert_eq!(
            lookup(&path, "example.com", 2222, &key_a).unwrap(),
            HostKeyStatus::Trusted
        );
        assert_eq!(
            lookup(&path, "example.com", 22, &key_a).unwrap(),
            HostKeyStatus::Unknown
        );
    }

    #[test]
    fn trust_preserves_existing_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        // Comment, and no trailing newline.
        std::fs::write(&path, "# my hosts\nold.example ssh-ed25519 AAAA").unwrap();
        trust(&path, "new.example", 22, &info(KEY_B)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.starts_with("# my hosts\nold.example ssh-ed25519 AAAA\nnew.example "),
            "{text}"
        );
    }

    #[test]
    fn tilde_expands_to_home() {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .unwrap();
        assert_eq!(
            expand_tilde("~/.ssh/id"),
            std::path::Path::new(&home).join(".ssh/id")
        );
        assert_eq!(
            expand_tilde("/abs/path"),
            std::path::PathBuf::from("/abs/path")
        );
        assert_eq!(
            expand_tilde("~other/x"),
            std::path::PathBuf::from("~other/x")
        );
    }
}
