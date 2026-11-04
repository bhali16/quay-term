//! A secret store for machines with no OS keychain (a headless Linux box).
//!
//! The secrets live in one file sealed with a master password, and are
//! readable only after `unlock`. [`FallbackStore`] picks the keychain when
//! there is one and this file when there is not.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use zeroize::Zeroizing;

use crate::archive::{self, Cost};
use crate::{KeychainStore, Result, SecretKey, SecretStore, VaultError};

struct Unlocked {
    password: Zeroizing<String>,
    secrets: BTreeMap<String, String>,
}

pub struct FileStore {
    path: PathBuf,
    cost: Cost,
    state: Mutex<Option<Unlocked>>,
}

fn io(e: std::io::Error) -> VaultError {
    VaultError::Other(e.to_string())
}

impl FileStore {
    pub fn new(path: PathBuf) -> Self {
        Self::with_cost(path, Cost::default())
    }

    pub fn with_cost(path: PathBuf, cost: Cost) -> Self {
        Self {
            path,
            cost,
            state: Mutex::new(None),
        }
    }

    /// A vault file exists (so unlocking asks for the existing password rather than choosing one).
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn is_unlocked(&self) -> bool {
        self.state.lock().unwrap().is_some()
    }

    /// Open the file with `password`, or create it with that password when there is none yet.
    pub fn unlock(&self, password: &str) -> Result<()> {
        let secrets = if self.path.exists() {
            let data = std::fs::read(&self.path).map_err(io)?;
            let plain =
                archive::open(password, &data).map_err(|e| VaultError::Other(e.to_string()))?;
            serde_json::from_slice(&plain)
                .map_err(|_| VaultError::Other("the secrets file is damaged".into()))?
        } else {
            BTreeMap::new()
        };
        let unlocked = Unlocked {
            password: Zeroizing::new(password.to_string()),
            secrets,
        };
        if !self.path.exists() {
            self.persist(&unlocked)?;
        }
        *self.state.lock().unwrap() = Some(unlocked);
        Ok(())
    }

    fn persist(&self, u: &Unlocked) -> Result<()> {
        let json = Zeroizing::new(
            serde_json::to_vec(&u.secrets).map_err(|e| VaultError::Other(e.to_string()))?,
        );
        let sealed = archive::seal_with(&u.password, &json, self.cost)
            .map_err(|e| VaultError::Other(e.to_string()))?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, sealed).map_err(io)?;
        std::fs::rename(&tmp, &self.path).map_err(io)
    }

    fn with<T>(&self, f: impl FnOnce(&mut Unlocked) -> Result<T>) -> Result<T> {
        match self.state.lock().unwrap().as_mut() {
            Some(u) => f(u),
            None => Err(VaultError::Locked),
        }
    }
}

impl SecretStore for FileStore {
    fn set(&self, key: SecretKey, value: &str) -> Result<()> {
        self.with(|u| {
            let before = u.secrets.insert(key.account(), value.to_string());
            self.persist(u).inspect_err(|_| match before {
                Some(old) => drop(u.secrets.insert(key.account(), old)),
                None => drop(u.secrets.remove(&key.account())),
            })
        })
    }

    fn get(&self, key: SecretKey) -> Result<Option<String>> {
        self.with(|u| Ok(u.secrets.get(&key.account()).cloned()))
    }

    fn delete(&self, key: SecretKey) -> Result<()> {
        self.with(|u| {
            if u.secrets.remove(&key.account()).is_some() {
                self.persist(u)?;
            }
            Ok(())
        })
    }
}

/// Where secrets go on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Keychain,
    /// No keychain; the master-password file is locked.
    FileLocked,
    FileUnlocked,
}

/// The OS keychain when it works, otherwise a master-password file.
#[derive(Clone)]
pub struct FallbackStore {
    keychain: Arc<KeychainStore>,
    file: Arc<FileStore>,
    use_file: bool,
}

impl FallbackStore {
    /// Probes the keychain once. Only "no keychain at all" switches to the file; any other answer keeps the keychain.
    pub fn new(keychain: KeychainStore, file: FileStore) -> Self {
        let probe = SecretKey::new(0, crate::SecretKind::Password);
        let use_file = matches!(keychain.get(probe), Err(VaultError::Unavailable(_)));
        Self {
            keychain: Arc::new(keychain),
            file: Arc::new(file),
            use_file,
        }
    }

    #[cfg(test)]
    fn forced(keychain: KeychainStore, file: FileStore, use_file: bool) -> Self {
        Self {
            keychain: Arc::new(keychain),
            file: Arc::new(file),
            use_file,
        }
    }

    pub fn mode(&self) -> Mode {
        match (self.use_file, self.file.is_unlocked()) {
            (false, _) => Mode::Keychain,
            (true, false) => Mode::FileLocked,
            (true, true) => Mode::FileUnlocked,
        }
    }

    /// True when unlocking would create the file, so the UI should ask for a new password twice.
    pub fn file_is_new(&self) -> bool {
        !self.file.exists()
    }

    pub fn unlock(&self, password: &str) -> Result<()> {
        self.file.unlock(password)
    }

    fn active(&self) -> &dyn SecretStore {
        if self.use_file {
            &*self.file
        } else {
            &*self.keychain
        }
    }
}

impl SecretStore for FallbackStore {
    fn set(&self, key: SecretKey, value: &str) -> Result<()> {
        self.active().set(key, value)
    }
    fn get(&self, key: SecretKey) -> Result<Option<String>> {
        self.active().get(key)
    }
    fn delete(&self, key: SecretKey) -> Result<()> {
        self.active().delete(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SecretKind;

    const FAST: Cost = Cost {
        memory_kib: 64,
        passes: 1,
        lanes: 1,
    };
    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("quay-vault-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d.join("secrets.vault")
    }
    fn key(id: i64) -> SecretKey {
        SecretKey::new(id, SecretKind::Password)
    }

    #[test]
    fn locked_until_unlocked_then_survives_a_restart() {
        let path = temp("a");
        let s = FileStore::with_cost(path.clone(), FAST);
        assert_eq!(s.set(key(1), "x"), Err(VaultError::Locked));
        s.unlock("master pw").unwrap();
        s.set(key(1), "hunter2").unwrap();
        s.set(key(2), "other").unwrap();
        s.delete(key(2)).unwrap();

        let again = FileStore::with_cost(path.clone(), FAST);
        assert!(again.exists() && !again.is_unlocked());
        assert!(again.unlock("wrong").is_err());
        assert!(!again.is_unlocked());
        again.unlock("master pw").unwrap();
        assert_eq!(again.get(key(1)).unwrap().as_deref(), Some("hunter2"));
        assert_eq!(again.get(key(2)).unwrap(), None);
        assert!(!std::fs::read(path)
            .unwrap()
            .windows(7)
            .any(|w| w == b"hunter2"));
    }

    #[test]
    fn fallback_uses_the_file_only_when_told_there_is_no_keychain() {
        let kc = || KeychainStore::with_service("quay-term-test-none");
        let f = FallbackStore::forced(kc(), FileStore::with_cost(temp("b"), FAST), true);
        assert_eq!(f.mode(), Mode::FileLocked);
        assert!(f.file_is_new());
        assert_eq!(f.set(key(1), "x"), Err(VaultError::Locked));
        f.unlock("master pw").unwrap();
        assert_eq!(f.mode(), Mode::FileUnlocked);
        f.set(key(1), "x").unwrap();
        assert_eq!(f.get(key(1)).unwrap().as_deref(), Some("x"));
    }
}
