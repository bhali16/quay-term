//! Credential storage for quay-term.
//!
//! Secrets (passwords, key passphrases) live in the operating system's
//! keychain and are addressed by saved-session id, so no secret ever appears
//! in the session database or config files.

use std::collections::HashMap;
use std::sync::Mutex;

/// What kind of secret is stored for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretKind {
    Password,
    Passphrase,
}

impl SecretKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Passphrase => "passphrase",
        }
    }
}

/// Identifies one secret: a kind of secret belonging to a saved session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SecretKey {
    pub session_id: i64,
    pub kind: SecretKind,
}

impl SecretKey {
    pub fn new(session_id: i64, kind: SecretKind) -> Self {
        Self { session_id, kind }
    }

    fn account(&self) -> String {
        format!("session:{}:{}", self.session_id, self.kind.as_str())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    /// The platform has no usable keychain (e.g. a headless Linux box).
    #[error("the system keychain is not available: {0}")]
    Unavailable(String),
    #[error("keychain error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, VaultError>;

pub trait SecretStore: Send + Sync {
    fn set(&self, key: SecretKey, value: &str) -> Result<()>;
    /// `Ok(None)` when nothing is stored.
    fn get(&self, key: SecretKey) -> Result<Option<String>>;
    /// Deleting a missing secret is not an error.
    fn delete(&self, key: SecretKey) -> Result<()>;
}

/// The OS keychain: macOS Keychain, Windows Credential Manager, or the
/// Secret Service on Linux.
pub struct KeychainStore {
    service: String,
}

impl KeychainStore {
    pub fn new() -> Self {
        Self::with_service("quay-term")
    }

    /// Use a different keychain service name (tests use this to stay isolated).
    pub fn with_service(service: &str) -> Self {
        Self {
            service: service.to_string(),
        }
    }

    fn entry(&self, key: SecretKey) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, &key.account()).map_err(map_err)
    }
}

impl Default for KeychainStore {
    fn default() -> Self {
        Self::new()
    }
}

fn map_err(e: keyring::Error) -> VaultError {
    match e {
        keyring::Error::NoStorageAccess(inner) => VaultError::Unavailable(inner.to_string()),
        keyring::Error::PlatformFailure(inner) => VaultError::Unavailable(inner.to_string()),
        other => VaultError::Other(other.to_string()),
    }
}

impl SecretStore for KeychainStore {
    fn set(&self, key: SecretKey, value: &str) -> Result<()> {
        self.entry(key)?.set_password(value).map_err(map_err)
    }

    fn get(&self, key: SecretKey) -> Result<Option<String>> {
        match self.entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_err(e)),
        }
    }

    fn delete(&self, key: SecretKey) -> Result<()> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_err(e)),
        }
    }
}

/// In-memory store for tests and as a stand-in where no keychain exists.
#[derive(Default)]
pub struct MemoryStore {
    map: Mutex<HashMap<SecretKey, String>>,
}

impl SecretStore for MemoryStore {
    fn set(&self, key: SecretKey, value: &str) -> Result<()> {
        self.map.lock().unwrap().insert(key, value.to_string());
        Ok(())
    }

    fn get(&self, key: SecretKey) -> Result<Option<String>> {
        Ok(self.map.lock().unwrap().get(&key).cloned())
    }

    fn delete(&self, key: SecretKey) -> Result<()> {
        self.map.lock().unwrap().remove(&key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(store: &dyn SecretStore, id: i64) {
        let pw = SecretKey::new(id, SecretKind::Password);
        let pp = SecretKey::new(id, SecretKind::Passphrase);
        assert_eq!(store.get(pw).unwrap(), None);
        store.set(pw, "hunter2").unwrap();
        store.set(pp, "open sesame").unwrap();
        assert_eq!(store.get(pw).unwrap().as_deref(), Some("hunter2"));
        assert_eq!(store.get(pp).unwrap().as_deref(), Some("open sesame"));
        store.set(pw, "changed").unwrap();
        assert_eq!(store.get(pw).unwrap().as_deref(), Some("changed"));
        store.delete(pw).unwrap();
        store.delete(pw).unwrap(); // already gone: fine
        assert_eq!(store.get(pw).unwrap(), None);
        assert_eq!(store.get(pp).unwrap().as_deref(), Some("open sesame"));
        store.delete(pp).unwrap();
    }

    #[test]
    fn memory_store_roundtrip() {
        roundtrip(&MemoryStore::default(), 1);
    }

    #[test]
    fn accounts_are_distinct_per_session_and_kind() {
        let a = SecretKey::new(1, SecretKind::Password).account();
        let b = SecretKey::new(1, SecretKind::Passphrase).account();
        let c = SecretKey::new(2, SecretKind::Password).account();
        assert_eq!(a, "session:1:password");
        assert!(a != b && a != c);
    }

    /// Touches the real OS keychain, which may prompt, so it is opt-in:
    /// `cargo test -p quay-vault -- --ignored`
    #[test]
    #[ignore]
    fn keychain_roundtrip() {
        roundtrip(&KeychainStore::with_service("quay-term-test"), 987_654);
    }
}
