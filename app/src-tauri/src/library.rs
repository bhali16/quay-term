//! Saved sessions plus the secrets that go with them.
//!
//! The session database holds no secrets; passwords and key passphrases are
//! kept in the keychain under the session id. This module keeps the two in
//! step and builds connection targets from saved sessions, so a stored
//! secret never has to travel to the webview.

use std::path::Path;
use std::sync::Mutex;

use quay_core::{SshAuth, SshTarget};
use quay_sessions::{AuthKind, Folder, ImportSummary, SavedSession, SessionInput, SessionStore};
use quay_vault::{SecretKey, SecretKind, SecretStore};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct LibraryView {
    pub folders: Vec<Folder>,
    pub sessions: Vec<SavedSession>,
}

/// What to do with a session's stored secret when saving.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "action", rename_all = "snake_case")]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub enum SecretUpdate {
    Unchanged,
    /// Store this password (or key passphrase, for key auth).
    Set {
        value: String,
    },
    Clear,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct ImportResult {
    pub added: u32,
    pub skipped: u32,
}

pub struct Library {
    store: Mutex<SessionStore>,
    vault: Box<dyn SecretStore>,
}

fn secret_kind(auth: AuthKind) -> Option<SecretKind> {
    match auth {
        AuthKind::Password => Some(SecretKind::Password),
        AuthKind::Key => Some(SecretKind::Passphrase),
        AuthKind::Agent => None,
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl Library {
    pub fn open(db: &Path, vault: Box<dyn SecretStore>) -> Result<Self, String> {
        Ok(Self {
            store: Mutex::new(SessionStore::open(db).map_err(err)?),
            vault,
        })
    }

    #[cfg(test)]
    fn in_memory(vault: Box<dyn SecretStore>) -> Self {
        Self {
            store: Mutex::new(SessionStore::open_in_memory().unwrap()),
            vault,
        }
    }

    pub fn load(&self, query: Option<&str>) -> Result<LibraryView, String> {
        let store = self.store.lock().unwrap();
        Ok(LibraryView {
            folders: store.list_folders().map_err(err)?,
            sessions: match query {
                Some(q) => store.search_sessions(q),
                None => store.list_sessions(),
            }
            .map_err(err)?,
        })
    }

    /// Create (`id` is `None`) or update a session and apply the secret change.
    pub fn save(
        &self,
        id: Option<i64>,
        input: &SessionInput,
        secret: SecretUpdate,
    ) -> Result<SavedSession, String> {
        let mut store = self.store.lock().unwrap();
        let previous = match id {
            Some(id) => store.get_session(id).map_err(err)?,
            None => None,
        };
        let saved = match id {
            Some(id) => store.update_session(id, input),
            None => store.create_session(input),
        }
        .map_err(err)?;

        // A secret of the old kind means nothing for the new one.
        let auth_changed = previous
            .as_ref()
            .is_some_and(|p| p.input.auth != saved.input.auth);
        let kind = secret_kind(saved.input.auth);
        if auth_changed || kind.is_none() {
            for k in [SecretKind::Password, SecretKind::Passphrase] {
                let _ = self.vault.delete(SecretKey::new(saved.id, k));
            }
        }

        let has_secret = match (&secret, kind) {
            (SecretUpdate::Set { value }, Some(kind)) if !value.is_empty() => {
                self.vault
                    .set(SecretKey::new(saved.id, kind), value)
                    .map_err(|e| {
                        format!("Session saved, but the secret could not be stored: {e}")
                    })?;
                true
            }
            (SecretUpdate::Clear, Some(kind)) => {
                let _ = self.vault.delete(SecretKey::new(saved.id, kind));
                false
            }
            _ => previous.as_ref().is_some_and(|p| p.has_secret) && !auth_changed && kind.is_some(),
        };
        store.set_has_secret(saved.id, has_secret).map_err(err)?;
        store
            .get_session(saved.id)
            .map_err(err)?
            .ok_or_else(|| "that saved session no longer exists".to_string())
    }

    pub fn delete(&self, id: i64) -> Result<(), String> {
        for k in [SecretKind::Password, SecretKind::Passphrase] {
            let _ = self.vault.delete(SecretKey::new(id, k));
        }
        self.store.lock().unwrap().delete_session(id).map_err(err)
    }

    pub fn create_folder(&self, name: &str, parent_id: Option<i64>) -> Result<Folder, String> {
        self.store
            .lock()
            .unwrap()
            .create_folder(name, parent_id)
            .map_err(err)
    }

    pub fn rename_folder(&self, id: i64, name: &str) -> Result<(), String> {
        self.store
            .lock()
            .unwrap()
            .rename_folder(id, name)
            .map_err(err)
    }

    pub fn move_session(&self, id: i64, folder_id: Option<i64>) -> Result<(), String> {
        self.store
            .lock()
            .unwrap()
            .move_session(id, folder_id)
            .map_err(err)
    }

    pub fn move_folder(&self, id: i64, parent_id: Option<i64>) -> Result<(), String> {
        self.store
            .lock()
            .unwrap()
            .move_folder(id, parent_id)
            .map_err(err)
    }

    pub fn delete_folder(&self, id: i64) -> Result<(), String> {
        self.store.lock().unwrap().delete_folder(id).map_err(err)
    }

    pub fn import_ssh_config(&self, path: &Path) -> Result<ImportResult, String> {
        let hosts = quay_sessions::read_ssh_config(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => format!("No SSH config found at {}.", path.display()),
            _ => format!("Could not read {}: {e}", path.display()),
        })?;
        let (ImportSummary { added, skipped }, _) = self
            .store
            .lock()
            .unwrap()
            .import_hosts(&hosts, None)
            .map_err(err)?;
        Ok(ImportResult {
            added: added as u32,
            skipped: skipped as u32,
        })
    }

    /// Build a connection target for a saved session. `secret` overrides the
    /// stored one (used when the user just typed it) and is saved if
    /// `remember` is set.
    pub fn target_for(
        &self,
        id: i64,
        secret: Option<String>,
        remember: bool,
    ) -> Result<SshTarget, String> {
        let mut store = self.store.lock().unwrap();
        let s = store
            .get_session(id)
            .map_err(err)?
            .ok_or("That saved session no longer exists.")?;
        let kind = secret_kind(s.input.auth);

        let secret = match (secret, kind) {
            (Some(v), Some(kind)) => {
                if remember {
                    self.vault
                        .set(SecretKey::new(id, kind), &v)
                        .map_err(|e| format!("Could not save the secret: {e}"))?;
                    store.set_has_secret(id, true).map_err(err)?;
                }
                Some(v)
            }
            (None, Some(kind)) if s.has_secret => {
                self.vault.get(SecretKey::new(id, kind)).map_err(err)?
            }
            _ => None,
        };

        let auth = match s.input.auth {
            AuthKind::Password => SshAuth::Password {
                password: secret
                    .ok_or("No saved password for this session. Enter one to connect.")?,
            },
            AuthKind::Key => SshAuth::Key {
                path: s.input.key_path.clone().unwrap_or_default(),
                passphrase: secret,
            },
            AuthKind::Agent => SshAuth::Agent,
        };
        Ok(SshTarget {
            host: s.input.host,
            port: s.input.port,
            user: s.input.user,
            auth,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quay_vault::MemoryStore;

    fn input(auth: AuthKind) -> SessionInput {
        SessionInput {
            name: "box".into(),
            folder_id: None,
            host: "h.example".into(),
            port: 22,
            user: "me".into(),
            auth,
            key_path: (auth == AuthKind::Key).then(|| "~/.ssh/id".to_string()),
            color: None,
            tags: vec![],
        }
    }

    fn lib() -> Library {
        Library::in_memory(Box::new(MemoryStore::default()))
    }

    fn password_of(t: &SshTarget) -> Option<&str> {
        match &t.auth {
            SshAuth::Password { password } => Some(password),
            _ => None,
        }
    }

    #[test]
    fn password_is_stored_in_the_vault_not_the_database() {
        let l = lib();
        let s = l
            .save(
                None,
                &input(AuthKind::Password),
                SecretUpdate::Set {
                    value: "pw1".into(),
                },
            )
            .unwrap();
        assert!(s.has_secret);
        let t = l.target_for(s.id, None, false).unwrap();
        assert_eq!(password_of(&t), Some("pw1"));
        // Nothing in the serialised session mentions the secret.
        assert!(!serde_json::to_string(&l.load(None).unwrap().sessions)
            .unwrap()
            .contains("pw1"));
    }

    #[test]
    fn editing_without_touching_the_secret_keeps_it() {
        let l = lib();
        let s = l
            .save(
                None,
                &input(AuthKind::Password),
                SecretUpdate::Set {
                    value: "pw1".into(),
                },
            )
            .unwrap();
        let mut edited = input(AuthKind::Password);
        edited.host = "new.example".into();
        let s = l
            .save(Some(s.id), &edited, SecretUpdate::Unchanged)
            .unwrap();
        assert!(s.has_secret);
        assert_eq!(
            password_of(&l.target_for(s.id, None, false).unwrap()),
            Some("pw1")
        );

        let s = l.save(Some(s.id), &edited, SecretUpdate::Clear).unwrap();
        assert!(!s.has_secret);
        assert!(l
            .target_for(s.id, None, false)
            .unwrap_err()
            .contains("No saved password"));
    }

    #[test]
    fn changing_auth_method_drops_the_old_secret() {
        let l = lib();
        let s = l
            .save(
                None,
                &input(AuthKind::Password),
                SecretUpdate::Set {
                    value: "pw1".into(),
                },
            )
            .unwrap();
        let s = l
            .save(Some(s.id), &input(AuthKind::Key), SecretUpdate::Unchanged)
            .unwrap();
        assert!(!s.has_secret);
        match l.target_for(s.id, None, false).unwrap().auth {
            SshAuth::Key { passphrase, .. } => assert_eq!(passphrase, None),
            other => panic!("{other:?}"),
        }
        // Back to password: the old password must not come back.
        let s = l
            .save(
                Some(s.id),
                &input(AuthKind::Password),
                SecretUpdate::Unchanged,
            )
            .unwrap();
        assert!(!s.has_secret);
    }

    #[test]
    fn typed_secret_overrides_and_can_be_remembered() {
        let l = lib();
        let s = l
            .save(None, &input(AuthKind::Password), SecretUpdate::Unchanged)
            .unwrap();
        assert!(l.target_for(s.id, None, false).is_err());

        let t = l.target_for(s.id, Some("typed".into()), false).unwrap();
        assert_eq!(password_of(&t), Some("typed"));
        assert!(
            !l.load(None).unwrap().sessions[0].has_secret,
            "not remembered"
        );

        l.target_for(s.id, Some("typed".into()), true).unwrap();
        assert!(l.load(None).unwrap().sessions[0].has_secret);
        assert_eq!(
            password_of(&l.target_for(s.id, None, false).unwrap()),
            Some("typed")
        );
    }

    #[test]
    fn key_passphrase_and_agent() {
        let l = lib();
        let k = l
            .save(
                None,
                &input(AuthKind::Key),
                SecretUpdate::Set {
                    value: "phrase".into(),
                },
            )
            .unwrap();
        match l.target_for(k.id, None, false).unwrap().auth {
            SshAuth::Key { path, passphrase } => {
                assert_eq!(path, "~/.ssh/id");
                assert_eq!(passphrase.as_deref(), Some("phrase"));
            }
            other => panic!("{other:?}"),
        }
        let a = l
            .save(
                None,
                &input(AuthKind::Agent),
                SecretUpdate::Set {
                    value: "ignored".into(),
                },
            )
            .unwrap();
        assert!(!a.has_secret, "agent auth has no secret");
        assert!(matches!(
            l.target_for(a.id, None, false).unwrap().auth,
            SshAuth::Agent
        ));
    }

    #[test]
    fn delete_removes_session_and_secret() {
        let vault = std::sync::Arc::new(MemoryStore::default());
        struct Shared(std::sync::Arc<MemoryStore>);
        impl SecretStore for Shared {
            fn set(&self, k: SecretKey, v: &str) -> quay_vault::Result<()> {
                self.0.set(k, v)
            }
            fn get(&self, k: SecretKey) -> quay_vault::Result<Option<String>> {
                self.0.get(k)
            }
            fn delete(&self, k: SecretKey) -> quay_vault::Result<()> {
                self.0.delete(k)
            }
        }
        let l = Library::in_memory(Box::new(Shared(vault.clone())));
        let s = l
            .save(
                None,
                &input(AuthKind::Password),
                SecretUpdate::Set { value: "pw".into() },
            )
            .unwrap();
        let key = SecretKey::new(s.id, SecretKind::Password);
        assert!(vault.get(key).unwrap().is_some());
        l.delete(s.id).unwrap();
        assert!(vault.get(key).unwrap().is_none());
        assert!(l.load(None).unwrap().sessions.is_empty());
    }

    #[test]
    fn missing_ssh_config_is_a_clear_error() {
        let e = lib()
            .import_ssh_config(Path::new("/definitely/not/here/config"))
            .unwrap_err();
        assert!(e.contains("No SSH config found"), "{e}");
    }
}
