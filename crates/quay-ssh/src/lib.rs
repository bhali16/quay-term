//! SSH client for quay-term, built on libssh2 (`ssh2`).
//!
//! [`SshSession::connect`] opens a TCP connection, verifies the host key
//! against `known_hosts` (asking the caller about unknown hosts),
//! authenticates with a password, key file or ssh-agent, and starts an
//! interactive shell that implements [`quay_core::Session`].

mod connect;
mod error;
pub mod known_hosts;
mod session;

pub use connect::{Challenge, ChallengePrompt, Challenger, ConnectOptions};
pub use error::SshError;
pub use known_hosts::{HostKeyInfo, HostKeyStatus};
pub use session::SshSession;
pub use ssh2::Session as RawSession;

/// Connect, verify the host key and authenticate, returning the raw libssh2
/// session (blocking mode). Used by SFTP, which opens its own connection.
pub fn connect_raw(
    opts: &ConnectOptions,
    verify: &mut dyn FnMut(&HostKeyInfo) -> bool,
) -> Result<RawSession, SshError> {
    connect::connect(opts, verify)
}

#[cfg(test)]
mod tests {
    use super::known_hosts::*;
    use crate::connect::expand_tilde;

    fn info(key: &[u8]) -> HostKeyInfo {
        HostKeyInfo {
            key_type: "ssh-ed25519".into(),
            fingerprint: "SHA256:test".into(),
            key: key.to_vec(),
        }
    }

    #[test]
    fn fingerprint_matches_openssh_format() {
        // sha256("") in base64 without padding.
        let digest = [
            0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f,
            0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b,
            0x78, 0x52, 0xb8, 0x55,
        ];
        assert_eq!(
            fingerprint(&digest),
            "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU"
        );
    }

    #[test]
    fn unknown_then_trusted_then_changed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("known_hosts");
        let key_a = [7u8; 32];
        let key_b = [9u8; 32];

        // Missing file: unknown, and nothing is created by a lookup.
        assert_eq!(
            lookup(&path, "example.com", 22, &key_a).unwrap(),
            HostKeyStatus::Unknown
        );
        assert!(!path.exists());

        trust(&path, "example.com", 22, &info(&key_a)).unwrap();
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
        let key = [3u8; 32];
        trust(&path, "example.com", 2222, &info(&key)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.starts_with("[example.com]:2222 ssh-ed25519 "),
            "{text}"
        );
        assert_eq!(
            lookup(&path, "example.com", 2222, &key).unwrap(),
            HostKeyStatus::Trusted
        );
        assert_eq!(
            lookup(&path, "example.com", 22, &key).unwrap(),
            HostKeyStatus::Unknown
        );
    }

    #[test]
    fn trust_preserves_existing_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        // Comment, and no trailing newline.
        std::fs::write(&path, "# my hosts\nold.example ssh-ed25519 AAAA").unwrap();
        trust(&path, "new.example", 22, &info(&[1u8; 32])).unwrap();
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
