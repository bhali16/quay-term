//! Moving the saved sessions, their secrets and the snippets between machines
//! in one file sealed with a master password (see `quay_vault::archive`).

use std::path::PathBuf;

use quay_sessions::{Folder, SavedSession};
use quay_vault::archive;
use quay_vault::{FallbackStore, Mode, SecretKey, SecretKind};
use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use crate::library::{err, secret_kind, Library};
use crate::snippets::{Snippet, Snippets};

const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Item {
    #[serde(flatten)]
    session: SavedSession,
    /// The password, or the key passphrase for key auth. Present only when exported with secrets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    secret: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Bundle {
    version: u32,
    folders: Vec<Folder>,
    sessions: Vec<Item>,
    #[serde(default)]
    snippets: Vec<Snippet>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct BackupReport {
    pub sessions: u32,
    pub secrets: u32,
    pub snippets: u32,
    /// Sessions the file held that were already saved, or secrets that could not be stored.
    pub skipped: u32,
    pub secrets_failed: u32,
}

impl Library {
    fn bundle(&self, snippets: Vec<Snippet>, with_secrets: bool) -> Result<Bundle, String> {
        let store = self.store.lock().unwrap();
        let sessions = store
            .list_sessions()
            .map_err(err)?
            .into_iter()
            .map(|session| {
                let secret = if with_secrets && session.has_secret {
                    secret_kind(session.input.auth)
                        .and_then(|k| self.vault.get(SecretKey::new(session.id, k)).ok().flatten())
                } else {
                    None
                };
                Item { session, secret }
            })
            .collect();
        Ok(Bundle {
            version: VERSION,
            folders: store.list_folders().map_err(err)?,
            sessions,
            snippets,
        })
    }

    /// Add what the bundle holds and is not here yet; nothing existing is changed or removed.
    fn absorb(&self, b: Bundle) -> Result<(BackupReport, Vec<Snippet>), String> {
        if b.version > VERSION {
            return Err(
                "This vault file was made by a newer quay-term. Update the app to read it.".into(),
            );
        }
        let mut store = self.store.lock().unwrap();
        let mut r = BackupReport::default();

        // Folders, parents first: an old id maps to an existing folder with the same name and parent, or a new one.
        let mut folder_map = std::collections::HashMap::<i64, i64>::new();
        let mut pending: Vec<&Folder> = b.folders.iter().collect();
        while !pending.is_empty() {
            let before = pending.len();
            let mut rest = Vec::new();
            for f in pending {
                let parent = match f.parent_id {
                    None => Some(None),
                    Some(p) => folder_map.get(&p).map(|n| Some(*n)),
                };
                let Some(parent) = parent else {
                    rest.push(f);
                    continue;
                };
                let existing = store
                    .list_folders()
                    .map_err(err)?
                    .into_iter()
                    .find(|e| e.name == f.name && e.parent_id == parent);
                let id = match existing {
                    Some(e) => e.id,
                    None => store.create_folder(&f.name, parent).map_err(err)?.id,
                };
                folder_map.insert(f.id, id);
            }
            if rest.len() == before {
                break; // a cycle or a missing parent: leave those folders out
            }
            pending = rest;
        }

        // Sessions without their jump host first, then link the jumps once every id is known.
        let mut session_map = std::collections::HashMap::<i64, i64>::new();
        let mut created = Vec::new();
        for item in &b.sessions {
            let mut input = item.session.input.clone();
            input.folder_id = input.folder_id.and_then(|f| folder_map.get(&f).copied());
            input.jump_id = None;
            let same = store.list_sessions().map_err(err)?.into_iter().find(|e| {
                e.input.name == input.name
                    && e.input.host == input.host
                    && e.input.port == input.port
                    && e.input.user == input.user
            });
            if let Some(e) = same {
                session_map.insert(item.session.id, e.id);
                r.skipped += 1;
                continue;
            }
            let saved = store.create_session(&input).map_err(err)?;
            session_map.insert(item.session.id, saved.id);
            created.push((item, saved));
            r.sessions += 1;
        }
        for (item, saved) in created {
            let mut input = saved.input.clone();
            input.jump_id = item
                .session
                .input
                .jump_id
                .and_then(|j| session_map.get(&j).copied());
            if input.jump_id.is_some() {
                store.update_session(saved.id, &input).map_err(err)?;
            }
            let kind: Option<SecretKind> = secret_kind(saved.input.auth);
            if let (Some(secret), Some(kind)) = (&item.secret, kind) {
                match self.vault.set(SecretKey::new(saved.id, kind), secret) {
                    Ok(()) => {
                        store.set_has_secret(saved.id, true).map_err(err)?;
                        r.secrets += 1;
                    }
                    Err(_) => r.secrets_failed += 1,
                }
            }
        }
        Ok((r, b.snippets))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct VaultStatus {
    /// "keychain", "locked" (a master-password file that needs unlocking) or "unlocked".
    pub mode: String,
    /// Unlocking would create the file, so ask for a new password.
    pub is_new: bool,
}

#[tauri::command]
pub(crate) fn vault_status(store: State<'_, FallbackStore>) -> VaultStatus {
    VaultStatus {
        mode: match store.mode() {
            Mode::Keychain => "keychain",
            Mode::FileLocked => "locked",
            Mode::FileUnlocked => "unlocked",
        }
        .into(),
        is_new: store.file_is_new(),
    }
}

#[tauri::command]
pub(crate) async fn vault_unlock(
    store: State<'_, FallbackStore>,
    password: String,
) -> Result<(), String> {
    if store.file_is_new() && password.chars().count() < 8 {
        return Err("Use a master password of at least 8 characters.".into());
    }
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || store.unlock(&password))
        .await
        .map_err(err)?
        .map_err(|e| match e {
            quay_vault::VaultError::Other(m) if m.contains("wrong password") => {
                "Wrong master password.".to_string()
            }
            e => e.to_string(),
        })
}

fn file_path(p: String) -> Result<PathBuf, String> {
    if p.trim().is_empty() {
        Err("Choose a file first.".into())
    } else {
        Ok(PathBuf::from(p))
    }
}

#[tauri::command]
pub(crate) async fn vault_export(
    library: State<'_, Library>,
    snippets: State<'_, Snippets>,
    path: String,
    password: String,
    with_secrets: bool,
) -> Result<BackupReport, String> {
    let path = file_path(path)?;
    if password.chars().count() < 8 {
        return Err("Use a master password of at least 8 characters.".into());
    }
    let bundle = library.bundle(snippets.all(), with_secrets)?;
    let report = BackupReport {
        sessions: bundle.sessions.len() as u32,
        secrets: bundle
            .sessions
            .iter()
            .filter(|i| i.secret.is_some())
            .count() as u32,
        snippets: bundle.snippets.len() as u32,
        ..Default::default()
    };
    let json = zeroize::Zeroizing::new(serde_json::to_vec(&bundle).map_err(err)?);
    let sealed = tauri::async_runtime::spawn_blocking(move || archive::seal(&password, &json))
        .await
        .map_err(err)?
        .map_err(err)?;
    std::fs::write(&path, sealed)
        .map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    Ok(report)
}

#[tauri::command]
pub(crate) async fn vault_import(
    library: State<'_, Library>,
    snippets: State<'_, Snippets>,
    path: String,
    password: String,
) -> Result<BackupReport, String> {
    let path = file_path(path)?;
    let data =
        std::fs::read(&path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    let plain = tauri::async_runtime::spawn_blocking(move || archive::open(&password, &data))
        .await
        .map_err(err)?
        .map_err(err)?;
    let bundle: Bundle =
        serde_json::from_slice(&plain).map_err(|_| "The vault file is damaged.".to_string())?;
    let (mut report, incoming) = library.absorb(bundle)?;
    report.snippets = snippets.merge(incoming)? as u32;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use quay_sessions::{AuthKind, SessionInput};

    fn input(name: &str, host: &str, folder: Option<i64>, jump: Option<i64>) -> SessionInput {
        SessionInput {
            name: name.into(),
            folder_id: folder,
            host: host.into(),
            port: 22,
            user: "me".into(),
            auth: AuthKind::Password,
            key_path: None,
            color: None,
            tags: vec![],
            jump_id: jump,
            tunnels: vec![],
        }
    }

    fn lib() -> Library {
        Library::open_for_test()
    }

    #[test]
    fn export_import_moves_folders_jumps_and_secrets_and_never_duplicates() {
        let a = lib();
        let (bastion, app) = {
            let mut s = a.store.lock().unwrap();
            let outer = s.create_folder("prod", None).unwrap();
            let inner = s.create_folder("web", Some(outer.id)).unwrap();
            let bastion = s
                .create_session(&input("bastion", "b.example", Some(outer.id), None))
                .unwrap();
            let app = s
                .create_session(&input("app", "a.example", Some(inner.id), Some(bastion.id)))
                .unwrap();
            (bastion.id, app.id)
        };
        for id in [bastion, app] {
            a.vault
                .set(SecretKey::new(id, SecretKind::Password), &format!("pw{id}"))
                .unwrap();
            a.store.lock().unwrap().set_has_secret(id, true).unwrap();
        }
        let json = serde_json::to_vec(&a.bundle(vec![], true).unwrap()).unwrap();

        // A fresh machine with one unrelated session already saved.
        let b = lib();
        b.store
            .lock()
            .unwrap()
            .create_session(&input("other", "o.example", None, None))
            .unwrap();
        let (r, _) = b.absorb(serde_json::from_slice(&json).unwrap()).unwrap();
        assert_eq!((r.sessions, r.secrets, r.skipped), (2, 2, 0));
        let store = b.store.lock().unwrap();
        let sessions = store.list_sessions().unwrap();
        let new_app = sessions.iter().find(|s| s.input.name == "app").unwrap();
        let new_bastion = sessions.iter().find(|s| s.input.name == "bastion").unwrap();
        assert_eq!(
            new_app.input.jump_id,
            Some(new_bastion.id),
            "the jump follows the new id"
        );
        let folder = store
            .list_folders()
            .unwrap()
            .into_iter()
            .find(|f| Some(f.id) == new_app.input.folder_id)
            .unwrap();
        assert_eq!(folder.name, "web");
        assert!(folder.parent_id.is_some(), "nesting is kept");
        assert!(new_app.has_secret);
        assert_eq!(
            b.vault
                .get(SecretKey::new(new_app.id, SecretKind::Password))
                .unwrap()
                .as_deref(),
            Some(format!("pw{app}").as_str())
        );
        drop(store);

        // Importing the same file again changes nothing.
        let (again, _) = b.absorb(serde_json::from_slice(&json).unwrap()).unwrap();
        assert_eq!((again.sessions, again.skipped), (0, 2));
        assert_eq!(b.store.lock().unwrap().list_sessions().unwrap().len(), 3);
    }

    #[test]
    fn secrets_stay_out_unless_asked_for() {
        let a = lib();
        let id = a
            .store
            .lock()
            .unwrap()
            .create_session(&input("x", "x.example", None, None))
            .unwrap()
            .id;
        a.vault
            .set(SecretKey::new(id, SecretKind::Password), "hunter2")
            .unwrap();
        a.store.lock().unwrap().set_has_secret(id, true).unwrap();
        let json = serde_json::to_string(&a.bundle(vec![], false).unwrap()).unwrap();
        assert!(!json.contains("hunter2"));
        let json = serde_json::to_string(&a.bundle(vec![], true).unwrap()).unwrap();
        assert!(json.contains("hunter2"));
    }

    #[test]
    fn refuses_a_newer_format() {
        let b = Bundle {
            version: VERSION + 1,
            folders: vec![],
            sessions: vec![],
            snippets: vec![],
        };
        assert!(lib().absorb(b).is_err());
    }
}
