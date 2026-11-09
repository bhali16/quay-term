//! Moving the saved sessions, their secrets and the snippets between machines
//! in one file sealed with a master password (see `quay_vault::archive`).

use std::path::PathBuf;

use quay_sessions::{Folder, SavedSession};
use quay_vault::archive;
use quay_vault::{FallbackStore, Mode, SecretKey, SecretKind};
use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use crate::history::{now_ms, History, HistoryEntry};
use crate::library::{err, secret_kind, Library};
use crate::snippets::{Snippet, Snippets};
use crate::SettingsPath;

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
    /// Command history, only when the user ticked it on export.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    history: Vec<HistoryEntry>,
}

/// What an import brings in besides sessions and secrets.
struct Absorbed {
    report: BackupReport,
    snippets: Vec<Snippet>,
    history: Vec<HistoryEntry>,
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
    /// Command history entries exported, or added on import.
    pub history: u32,
    /// History entries in the file that were not added (recording is off, or the vault is locked).
    pub history_skipped: u32,
}

impl Library {
    fn bundle(
        &self,
        snippets: Vec<Snippet>,
        history: Vec<HistoryEntry>,
        with_secrets: bool,
    ) -> Result<Bundle, String> {
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
            history,
        })
    }

    /// Add what the bundle holds and is not here yet; nothing existing is changed or removed.
    fn absorb(&self, b: Bundle) -> Result<Absorbed, String> {
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
        // Remembered snippet values follow their session to its id here; others are dropped.
        let snippets = b
            .snippets
            .into_iter()
            .map(|mut sn| {
                sn.defaults = std::mem::take(&mut sn.defaults)
                    .into_iter()
                    .filter_map(|(k, v)| {
                        let old = k.parse::<i64>().ok()?;
                        Some((session_map.get(&old)?.to_string(), v))
                    })
                    .collect();
                sn
            })
            .collect();
        // History keeps its link to a session only when that session came along.
        let history = b
            .history
            .into_iter()
            .map(|mut e| {
                e.session_id = e.session_id.and_then(|s| session_map.get(&s).copied());
                e
            })
            .collect();
        Ok(Absorbed {
            report: r,
            snippets,
            history,
        })
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
#[allow(clippy::too_many_arguments)]
pub(crate) async fn vault_export(
    library: State<'_, Library>,
    snippets: State<'_, Snippets>,
    history: State<'_, History>,
    path: String,
    password: String,
    with_secrets: bool,
    with_history: bool,
) -> Result<BackupReport, String> {
    let path = file_path(path)?;
    if password.chars().count() < 8 {
        return Err("Use a master password of at least 8 characters.".into());
    }
    let entries = if with_history {
        history.list()?
    } else {
        vec![]
    };
    let bundle = library.bundle(snippets.all(), entries, with_secrets)?;
    let report = BackupReport {
        sessions: bundle.sessions.len() as u32,
        secrets: bundle
            .sessions
            .iter()
            .filter(|i| i.secret.is_some())
            .count() as u32,
        snippets: bundle.snippets.len() as u32,
        history: bundle.history.len() as u32,
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
    history: State<'_, History>,
    settings: State<'_, SettingsPath>,
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
    let Absorbed {
        mut report,
        snippets: incoming,
        history: entries,
    } = library.absorb(bundle)?;
    report.snippets = snippets.merge(incoming)? as u32;
    // History is written only while recording is on, as everywhere else.
    if !entries.is_empty() {
        let total = entries.len() as u32;
        let added = if quay_config::Settings::load(&settings.0).record_history {
            history.import(entries, now_ms()).unwrap_or_else(|e| {
                log::warn!("history import skipped: {e}");
                0
            }) as u32
        } else {
            0
        };
        report.history = added;
        report.history_skipped = total.saturating_sub(added);
    }
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
            env: None,
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
        let json = serde_json::to_vec(&a.bundle(vec![], vec![], true).unwrap()).unwrap();

        // A fresh machine with one unrelated session already saved.
        let b = lib();
        b.store
            .lock()
            .unwrap()
            .create_session(&input("other", "o.example", None, None))
            .unwrap();
        let Absorbed { report: r, .. } = b.absorb(serde_json::from_slice(&json).unwrap()).unwrap();
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
        let Absorbed { report: again, .. } =
            b.absorb(serde_json::from_slice(&json).unwrap()).unwrap();
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
        let json = serde_json::to_string(&a.bundle(vec![], vec![], false).unwrap()).unwrap();
        assert!(!json.contains("hunter2"));
        let json = serde_json::to_string(&a.bundle(vec![], vec![], true).unwrap()).unwrap();
        assert!(json.contains("hunter2"));
    }

    #[test]
    fn snippets_travel_with_the_bundle_and_are_not_duplicated() {
        let snip = |name: &str| Snippet {
            id: 7,
            name: name.into(),
            folder: "ops".into(),
            body: format!("echo {name}"),
            ..Default::default()
        };
        let json = serde_json::to_vec(
            &lib()
                .bundle(vec![snip("a"), snip("b")], vec![], false)
                .unwrap(),
        )
        .unwrap();
        let Absorbed {
            report: r,
            snippets: incoming,
            ..
        } = lib()
            .absorb(serde_json::from_slice(&json).unwrap())
            .unwrap();
        assert_eq!(incoming.len(), 2);
        assert_eq!(r.snippets, 0, "merge, not absorb, counts what was added");

        let dir = std::env::temp_dir().join(format!("quay-backup-snip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let library = Snippets::open(dir.join("snippets.json"));
        assert_eq!(library.merge(incoming.clone()).unwrap(), 2);
        assert_eq!(
            library.merge(incoming).unwrap(),
            0,
            "a second import adds nothing"
        );
        let ids: Vec<u32> = library.all().iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![1, 2], "imported snippets get fresh ids");
    }

    #[test]
    fn remembered_snippet_values_follow_their_session() {
        let a = lib();
        let id = a
            .store
            .lock()
            .unwrap()
            .create_session(&input("web", "w.example", None, None))
            .unwrap()
            .id;
        let vals = std::collections::BTreeMap::from([("env".to_string(), "prod".to_string())]);
        let snip = Snippet {
            id: 1,
            name: "deploy".into(),
            body: "deploy {{env}}".into(),
            defaults: [(id.to_string(), vals.clone()), ("999".into(), vals.clone())].into(),
            ..Default::default()
        };
        let json = serde_json::to_vec(&a.bundle(vec![snip], vec![], false).unwrap()).unwrap();

        let b = lib();
        for n in ["x", "y"] {
            b.store
                .lock()
                .unwrap()
                .create_session(&input(n, &format!("{n}.example"), None, None))
                .unwrap();
        }
        let incoming = b
            .absorb(serde_json::from_slice(&json).unwrap())
            .unwrap()
            .snippets;
        let new_id = b
            .store
            .lock()
            .unwrap()
            .list_sessions()
            .unwrap()
            .into_iter()
            .find(|s| s.input.name == "web")
            .unwrap()
            .id;
        assert_ne!(new_id, id, "the session got a new id on this machine");
        let d = &incoming[0].defaults;
        assert_eq!(
            d.len(),
            1,
            "values for a session not in the file are dropped"
        );
        assert_eq!(d.get(&new_id.to_string()), Some(&vals));
    }

    #[test]
    fn refuses_a_newer_format() {
        let b = Bundle {
            version: VERSION + 1,
            folders: vec![],
            sessions: vec![],
            snippets: vec![],
            history: vec![],
        };
        assert!(lib().absorb(b).is_err());
    }

    #[test]
    fn history_travels_only_when_asked_and_follows_its_session() {
        let a = lib();
        let id = a
            .store
            .lock()
            .unwrap()
            .create_session(&input("web", "w.example", None, None))
            .unwrap()
            .id;
        let none = serde_json::to_string(&a.bundle(vec![], vec![], false).unwrap()).unwrap();
        assert!(!none.contains("history"), "no history key without the tick");

        let e = |cmd: &str, session_id| HistoryEntry {
            id: 1,
            ts: 5,
            command: cmd.into(),
            host: "web".into(),
            session_id,
            cwd: None,
            exit: Some(0),
        };
        let json = serde_json::to_vec(
            &a.bundle(
                vec![],
                vec![e("deploy", Some(id)), e("ls", Some(999))],
                false,
            )
            .unwrap(),
        )
        .unwrap();
        let b = lib();
        b.store
            .lock()
            .unwrap()
            .create_session(&input("other", "o.example", None, None))
            .unwrap();
        let got = b
            .absorb(serde_json::from_slice(&json).unwrap())
            .unwrap()
            .history;
        let new_id = b
            .store
            .lock()
            .unwrap()
            .list_sessions()
            .unwrap()
            .into_iter()
            .find(|s| s.input.name == "web")
            .unwrap()
            .id;
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].session_id, Some(new_id));
        assert_eq!(
            got[1].session_id, None,
            "a session not in the file is unlinked"
        );
    }

    /// The whole path on a machine with no keychain: the real master-password file feeds the
    /// history key, history is sealed on disk, exported, and lands on a keychain machine.
    #[test]
    fn history_round_trip_through_a_master_password_machine() {
        use quay_vault::{archive::Cost, FileStore, KeychainStore};
        let dir = tempfile::tempdir().unwrap();
        let fast = Cost {
            memory_kib: 64,
            passes: 1,
            lanes: 1,
        };
        let store = FallbackStore::forced(
            KeychainStore::with_service("quay-term-test-none"),
            FileStore::with_cost(dir.path().join("secrets.vault"), fast),
            true,
        );
        let keys = store.clone();
        let h = History::new(
            dir.path().join("history.db"),
            Box::new(move || keys.history_key()),
        );
        let cmd = |c: &str| crate::history::NewHistoryEntry {
            command: c.into(),
            host: "bastion-prod-77".into(),
            session_id: None,
            cwd: Some("/srv/app".into()),
            exit: Some(0),
        };
        let now = now_ms();
        assert!(!h.add(cmd("ls"), now).unwrap(), "locked: not saved");
        store.unlock("master password").unwrap();
        assert!(h
            .add(cmd("kubectl rollout restart deploy/api"), now)
            .unwrap());
        let raw = std::fs::read(dir.path().join("history.db")).unwrap();
        for needle in [&b"kubectl"[..], b"bastion-prod-77", b"/srv/app"] {
            assert!(!raw.windows(needle.len()).any(|w| w == needle));
        }

        // A restart: a fresh vault object over the same files must unlock to read history.
        let store2 = FallbackStore::forced(
            KeychainStore::with_service("quay-term-test-none"),
            FileStore::with_cost(dir.path().join("secrets.vault"), fast),
            true,
        );
        let keys2 = store2.clone();
        let h2 = History::new(
            dir.path().join("history.db"),
            Box::new(move || keys2.history_key()),
        );
        assert!(h2.list().is_err());
        store2.unlock("master password").unwrap();
        let entries = h2.list().unwrap();
        assert_eq!(entries[0].command, "kubectl rollout restart deploy/api");

        let json = serde_json::to_vec(&lib().bundle(vec![], entries, false).unwrap()).unwrap();
        let got = lib()
            .absorb(serde_json::from_slice(&json).unwrap())
            .unwrap()
            .history;
        let other = tempfile::tempdir().unwrap();
        let plain = History::new(other.path().join("history.db"), Box::new(|| Ok(None)));
        assert_eq!(plain.import(got, now_ms()).unwrap(), 1);
        assert_eq!(plain.list().unwrap()[0].host, "bastion-prod-77");
    }
}
