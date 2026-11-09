//! Cross-session command history: commands that finished in any pane (known from shell
//! integration marks), kept in a small SQLite file so they can be searched later.
//!
//! Recording is opt-in. The file is not created until the first command is saved while
//! `record_history` is on, and it is readable only by the user's account (0600 on Unix).
//! The UI drops commands typed with a leading space or matching the deny-list before they get
//! here; this side checks the setting again, rejects leading-space commands too, and enforces
//! the retention cap. Deleting uses SQLite's `secure_delete`, and "forget all" removes the file.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use quay_vault::field::{self, FieldKey};
use quay_vault::VaultError;
use rusqlite::types::Value;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use crate::{err, SettingsPath};

/// Most entries kept; the oldest go first.
pub const MAX_ENTRIES: i64 = 20_000;
/// Entries older than this are dropped.
pub const MAX_AGE_MS: i64 = 180 * 24 * 60 * 60 * 1000;
/// Longer commands are not saved (they are almost always pasted scripts).
pub const MAX_COMMAND_LEN: usize = 8 * 1024;
const MAX_FIELD_LEN: usize = 1024;
const SCHEMA_VERSION: i32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct HistoryEntry {
    #[ts(type = "number")]
    pub id: i64,
    /// Milliseconds since the Unix epoch, when the command finished.
    #[ts(type = "number")]
    pub ts: i64,
    pub command: String,
    /// Where it ran: a saved session's name, `user@host`, or `local`.
    pub host: String,
    /// The saved session, when it ran in one.
    #[ts(type = "number | null")]
    pub session_id: Option<i64>,
    /// Working directory, when the shell reported it (OSC 7).
    pub cwd: Option<String>,
    pub exit: Option<i32>,
}

/// A finished command as the UI reports it.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct NewHistoryEntry {
    pub command: String,
    pub host: String,
    #[ts(type = "number | null")]
    pub session_id: Option<i64>,
    pub cwd: Option<String>,
    pub exit: Option<i32>,
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Create the file readable only by this user, or tighten an existing one.
fn restrict(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        if path.exists() {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        } else {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)?;
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn clip(s: Option<String>) -> Option<String> {
    s.map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.len() <= MAX_FIELD_LEN)
}

/// Where the key for sealing entries comes from: `Ok(None)` stores them plain (a system
/// keychain is in use), `Err(Locked)` means the master-password file is not unlocked yet.
pub(crate) type KeySource = Box<dyn Fn() -> Result<Option<FieldKey>, VaultError> + Send + Sync>;

const LOCKED: &str =
    "Command history is encrypted with your master password. Unlock it in Settings > Backup.";

struct Db {
    conn: Connection,
    /// Every entry is known to be sealed, so the plain-to-sealed pass need not run again.
    sealed: bool,
}

pub(crate) struct History {
    path: PathBuf,
    key: KeySource,
    db: Mutex<Option<Db>>,
}

/// An entry as stored: text fields are plain, or sealed with the history key when `enc` is 1.
struct Raw {
    id: i64,
    ts: i64,
    command: Value,
    host: Value,
    session_id: Option<i64>,
    cwd: Value,
    exit: Option<i32>,
    enc: bool,
}

const SELECT: &str = "SELECT id, ts, command, host, session_id, cwd, exit, enc FROM entries";

fn raw(r: &rusqlite::Row<'_>) -> rusqlite::Result<Raw> {
    Ok(Raw {
        id: r.get(0)?,
        ts: r.get(1)?,
        command: r.get(2)?,
        host: r.get(3)?,
        session_id: r.get(4)?,
        cwd: r.get(5)?,
        exit: r.get(6)?,
        enc: r.get::<_, i64>(7)? != 0,
    })
}

fn put(key: Option<&FieldKey>, label: &str, text: &str) -> rusqlite::Result<Value> {
    match key {
        None => Ok(Value::Text(text.into())),
        Some(k) => field::seal(k, label, text.as_bytes())
            .map(Value::Blob)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into())),
    }
}

fn put_opt(key: Option<&FieldKey>, label: &str, text: Option<&str>) -> rusqlite::Result<Value> {
    text.map_or(Ok(Value::Null), |t| put(key, label, t))
}

/// Read a text field; `Err(())` when it is sealed and cannot be opened with `key`.
fn get(key: Option<&FieldKey>, enc: bool, label: &str, v: &Value) -> Result<Option<String>, ()> {
    match (v, enc) {
        (Value::Null, _) => Ok(None),
        (Value::Text(t), false) => Ok(Some(t.clone())),
        (Value::Blob(b), true) => {
            let plain = field::open(key.ok_or(())?, label, b).ok_or(())?;
            String::from_utf8(plain).map(Some).map_err(|_| ())
        }
        _ => Err(()),
    }
}

impl Raw {
    /// `None` when it cannot be read with this key (sealed under a vault this machine no
    /// longer uses, or damaged).
    fn open(&self, key: Option<&FieldKey>) -> Option<HistoryEntry> {
        Some(HistoryEntry {
            id: self.id,
            ts: self.ts,
            command: get(key, self.enc, "command", &self.command).ok()??,
            host: get(key, self.enc, "host", &self.host).ok()??,
            session_id: self.session_id,
            cwd: get(key, self.enc, "cwd", &self.cwd).ok()?,
            exit: self.exit,
        })
    }
}

fn all(c: &Connection) -> rusqlite::Result<Vec<Raw>> {
    let mut st = c.prepare(&format!("{SELECT} ORDER BY ts DESC, id DESC"))?;
    let rows = st.query_map([], raw)?;
    rows.collect()
}

#[allow(clippy::too_many_arguments)]
fn insert(
    c: &Connection,
    key: Option<&FieldKey>,
    ts: i64,
    command: &str,
    host: &str,
    session_id: Option<i64>,
    cwd: Option<&str>,
    exit: Option<i32>,
) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO entries (ts, command, host, session_id, cwd, exit, enc)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            ts,
            put(key, "command", command)?,
            put(key, "host", host)?,
            session_id,
            put_opt(key, "cwd", cwd)?,
            exit,
            key.is_some()
        ],
    )?;
    Ok(())
}

/// Enforce the age and size caps.
fn prune(c: &Connection, now: i64) -> rusqlite::Result<()> {
    c.execute(
        "DELETE FROM entries WHERE ts < ?1",
        [now.saturating_sub(MAX_AGE_MS)],
    )?;
    c.execute(
        "DELETE FROM entries WHERE id <= (SELECT id FROM entries ORDER BY id DESC LIMIT 1 OFFSET ?1)",
        [MAX_ENTRIES],
    )?;
    Ok(())
}

/// Seal every plain entry (left from before a master password was in use), then rewrite the
/// file so the plain text is gone from disk.
fn seal_plain(c: &mut Connection, key: &FieldKey) -> rusqlite::Result<()> {
    let plain: Vec<Raw> = {
        let mut st = c.prepare(&format!("{SELECT} WHERE enc = 0"))?;
        let rows = st.query_map([], raw)?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    if plain.is_empty() {
        return Ok(());
    }
    let tx = c.transaction()?;
    for r in &plain {
        let Some(e) = r.open(None) else {
            tx.execute("DELETE FROM entries WHERE id = ?1", [r.id])?;
            continue;
        };
        tx.execute(
            "UPDATE entries SET command = ?1, host = ?2, cwd = ?3, enc = 1 WHERE id = ?4",
            params![
                put(Some(key), "command", &e.command)?,
                put(Some(key), "host", &e.host)?,
                put_opt(Some(key), "cwd", e.cwd.as_deref())?,
                r.id
            ],
        )?;
    }
    tx.commit()?;
    c.execute_batch("VACUUM")
}

impl History {
    pub(crate) fn new(path: PathBuf, key: KeySource) -> Self {
        Self {
            path,
            key,
            db: Mutex::new(None),
        }
    }

    /// The key to use now. `Ok(None)` means plain; locked becomes `Err(None)` so callers can
    /// choose between skipping quietly and telling the user.
    fn key(&self) -> Result<Option<FieldKey>, Option<String>> {
        match (self.key)() {
            Ok(k) => Ok(k),
            Err(VaultError::Locked) => Err(None),
            Err(e) => Err(Some(err(e))),
        }
    }

    fn key_or_say(&self) -> Result<Option<FieldKey>, String> {
        self.key().map_err(|e| e.unwrap_or_else(|| LOCKED.into()))
    }

    /// Run `f` on the database. With `create` false and no file yet, returns `Ok(None)` so that
    /// reading never creates a file. With a key, plain entries are sealed first.
    fn with<T>(
        &self,
        create: bool,
        key: Option<&FieldKey>,
        f: impl FnOnce(&mut Connection) -> rusqlite::Result<T>,
    ) -> Result<Option<T>, String> {
        let mut guard = self.db.lock().unwrap();
        if guard.is_none() {
            if !create && !self.path.exists() {
                return Ok(None);
            }
            if let Some(dir) = self.path.parent() {
                std::fs::create_dir_all(dir).map_err(err)?;
            }
            restrict(&self.path).map_err(err)?;
            let conn = Self::init(Connection::open(&self.path).map_err(err)?).map_err(err)?;
            *guard = Some(Db {
                conn,
                sealed: false,
            });
        }
        let db = guard.as_mut().unwrap();
        if let (Some(k), false) = (key, db.sealed) {
            seal_plain(&mut db.conn, k).map_err(err)?;
            db.sealed = true;
        }
        f(&mut db.conn).map(Some).map_err(err)
    }

    fn init(conn: Connection) -> rusqlite::Result<Connection> {
        // Overwrite deleted rows with zeros, so a deleted command is gone from the file.
        conn.pragma_update(None, "secure_delete", "ON")?;
        let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < 1 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS entries (
                     id INTEGER PRIMARY KEY,
                     ts INTEGER NOT NULL,
                     command TEXT NOT NULL,
                     host TEXT NOT NULL,
                     session_id INTEGER,
                     cwd TEXT,
                     exit INTEGER
                 );
                 CREATE INDEX IF NOT EXISTS entries_ts ON entries(ts);",
            )?;
        }
        if version < 2 {
            // 1 when command, host and cwd are sealed with the history key (stored as blobs).
            conn.execute_batch("ALTER TABLE entries ADD COLUMN enc INTEGER NOT NULL DEFAULT 0;")?;
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(conn)
    }

    /// Save one command. Returns false when it was not saved (empty, too long, typed with a
    /// leading space, or the master-password vault is locked so it cannot be sealed). The same
    /// command finishing again on the same host only refreshes the latest entry.
    pub(crate) fn add(&self, e: NewHistoryEntry, now: i64) -> Result<bool, String> {
        if e.command.starts_with(char::is_whitespace) {
            return Ok(false);
        }
        let command = e.command.trim_end().to_string();
        if command.is_empty() || command.len() > MAX_COMMAND_LEN {
            return Ok(false);
        }
        let key = match self.key() {
            Ok(k) => k,
            Err(None) => return Ok(false),
            Err(Some(m)) => return Err(m),
        };
        let key = key.as_ref();
        let host = clip(Some(e.host)).unwrap_or_else(|| "local".into());
        let cwd = clip(e.cwd);
        self.with(true, key, |c| {
            let tx = c.transaction()?;
            let last = tx
                .query_row(
                    &format!("{SELECT} WHERE id = (SELECT MAX(id) FROM entries)"),
                    [],
                    raw,
                )
                .ok()
                .and_then(|r| r.open(key));
            match last {
                Some(l) if l.command == command && l.host == host => {
                    tx.execute(
                        "UPDATE entries SET ts = ?1, exit = ?2, cwd = ?3, enc = ?4 WHERE id = ?5",
                        params![
                            now,
                            e.exit,
                            put_opt(key, "cwd", cwd.as_deref())?,
                            key.is_some(),
                            l.id
                        ],
                    )?;
                }
                _ => insert(
                    &tx,
                    key,
                    now,
                    &command,
                    &host,
                    e.session_id,
                    cwd.as_deref(),
                    e.exit,
                )?,
            }
            prune(&tx, now)?;
            tx.commit()
        })?;
        Ok(true)
    }

    /// Every entry, newest first. Fails while the master-password vault is locked.
    pub(crate) fn list(&self) -> Result<Vec<HistoryEntry>, String> {
        if !self.path.exists() {
            return Ok(vec![]);
        }
        let key = self.key_or_say()?;
        let key = key.as_ref();
        Ok(self
            .with(false, key, |c| all(c))?
            .unwrap_or_default()
            .iter()
            .filter_map(|r| r.open(key))
            .collect())
    }

    pub(crate) fn count(&self) -> Result<i64, String> {
        Ok(self
            .with(false, None, |c| {
                c.query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
            })?
            .unwrap_or(0))
    }

    /// Forget a command everywhere it was run. Returns how many entries went.
    pub(crate) fn delete_command(&self, command: &str) -> Result<usize, String> {
        if !self.path.exists() {
            return Ok(0);
        }
        let key = self.key_or_say()?;
        let key = key.as_ref();
        Ok(self
            .with(false, key, |c| {
                let ids: Vec<i64> = all(c)?
                    .iter()
                    .filter_map(|r| r.open(key))
                    .filter(|e| e.command == command)
                    .map(|e| e.id)
                    .collect();
                let tx = c.transaction()?;
                for id in &ids {
                    tx.execute("DELETE FROM entries WHERE id = ?1", [id])?;
                }
                tx.commit()?;
                Ok(ids.len())
            })?
            .unwrap_or(0))
    }

    /// Add entries from a backup, skipping ones already here (same time, command and host).
    /// Returns how many were added.
    pub(crate) fn import(&self, entries: Vec<HistoryEntry>, now: i64) -> Result<usize, String> {
        let key = self.key_or_say()?;
        let key = key.as_ref();
        let entries: Vec<_> = entries
            .into_iter()
            .filter(|e| {
                !e.command.starts_with(char::is_whitespace)
                    && !e.command.trim().is_empty()
                    && e.command.len() <= MAX_COMMAND_LEN
                    && e.ts >= now.saturating_sub(MAX_AGE_MS)
            })
            .collect();
        if entries.is_empty() {
            return Ok(0);
        }
        Ok(self
            .with(true, key, |c| {
                let mut seen: std::collections::HashSet<(i64, String, String)> = all(c)?
                    .iter()
                    .filter_map(|r| r.open(key))
                    .map(|e| (e.ts, e.command, e.host))
                    .collect();
                let tx = c.transaction()?;
                let mut added = 0;
                // Oldest first, so ids keep time order and the size cap drops the oldest.
                for e in entries.iter().rev() {
                    let host = clip(Some(e.host.clone())).unwrap_or_else(|| "local".into());
                    let command = e.command.trim_end().to_string();
                    if !seen.insert((e.ts, command.clone(), host.clone())) {
                        continue;
                    }
                    let cwd = clip(e.cwd.clone());
                    insert(
                        &tx,
                        key,
                        e.ts,
                        &command,
                        &host,
                        e.session_id,
                        cwd.as_deref(),
                        e.exit,
                    )?;
                    added += 1;
                }
                prune(&tx, now)?;
                tx.commit()?;
                Ok(added)
            })?
            .unwrap_or(0))
    }

    /// Forget everything: close the database and remove its files from disk.
    pub(crate) fn clear(&self) -> Result<(), String> {
        let mut guard = self.db.lock().unwrap();
        if let Some(db) = guard.take() {
            db.conn.close().map_err(|(_, e)| err(e))?;
        }
        for suffix in ["", "-journal", "-wal", "-shm"] {
            let mut p = self.path.clone().into_os_string();
            p.push(suffix);
            match std::fs::remove_file(PathBuf::from(p)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(err(e)),
            }
        }
        Ok(())
    }
}

/// Save a finished command. Does nothing (and writes nothing) unless `record_history` is on.
#[tauri::command]
pub(crate) fn history_add(
    h: State<'_, History>,
    settings: State<'_, SettingsPath>,
    entry: NewHistoryEntry,
) -> Result<bool, String> {
    if !quay_config::Settings::load(&settings.0).record_history {
        return Ok(false);
    }
    h.add(entry, now_ms())
}

#[tauri::command]
pub(crate) fn history_list(h: State<'_, History>) -> Result<Vec<HistoryEntry>, String> {
    h.list()
}

#[tauri::command]
pub(crate) fn history_count(h: State<'_, History>) -> Result<i64, String> {
    h.count()
}

#[tauri::command]
pub(crate) fn history_delete(h: State<'_, History>, command: String) -> Result<usize, String> {
    h.delete_command(&command)
}

#[tauri::command]
pub(crate) fn history_clear(h: State<'_, History>) -> Result<(), String> {
    h.clear()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    fn entry(command: &str, host: &str) -> NewHistoryEntry {
        NewHistoryEntry {
            command: command.into(),
            host: host.into(),
            session_id: None,
            cwd: None,
            exit: Some(0),
        }
    }

    fn plain() -> KeySource {
        Box::new(|| Ok(None))
    }

    fn store() -> (tempfile::TempDir, History) {
        let dir = tempfile::tempdir().unwrap();
        let h = History::new(dir.path().join("history.db"), plain());
        (dir, h)
    }

    /// A key source whose lock can be flipped, standing in for the master-password file.
    fn vault() -> (Arc<AtomicBool>, FieldKey, KeySource) {
        let unlocked = Arc::new(AtomicBool::new(false));
        let key = field::new_key().unwrap();
        let (u, k) = (unlocked.clone(), key.clone());
        let src: KeySource = Box::new(move || {
            if u.load(Ordering::SeqCst) {
                Ok(Some(k.clone()))
            } else {
                Err(VaultError::Locked)
            }
        });
        (unlocked, key, src)
    }

    fn file_has(dir: &tempfile::TempDir, needle: &[u8]) -> bool {
        std::fs::read(dir.path().join("history.db"))
            .unwrap()
            .windows(needle.len())
            .any(|w| w == needle)
    }

    #[test]
    fn reading_never_creates_the_file() {
        let (dir, h) = store();
        assert!(h.list().unwrap().is_empty());
        assert_eq!(h.count().unwrap(), 0);
        assert_eq!(h.delete_command("ls").unwrap(), 0);
        assert!(!dir.path().join("history.db").exists());
    }

    #[test]
    fn saves_newest_first_and_survives_a_restart() {
        let (dir, h) = store();
        assert!(h.add(entry("ls -la", "local"), 1_000).unwrap());
        let mut e = entry("rsync -av a b", "web1");
        e.session_id = Some(4);
        e.cwd = Some("/srv".into());
        e.exit = Some(23);
        assert!(h.add(e, 2_000).unwrap());
        drop(h);
        let h = History::new(dir.path().join("history.db"), plain());
        let all = h.list().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].command, "rsync -av a b");
        assert_eq!(
            (all[0].session_id, all[0].cwd.as_deref(), all[0].exit),
            (Some(4), Some("/srv"), Some(23))
        );
        assert_eq!(all[1].host, "local");
    }

    #[test]
    fn skips_leading_space_empty_and_huge_commands() {
        let (_d, h) = store();
        assert!(!h.add(entry(" export TOKEN=x", "local"), 1).unwrap());
        assert!(!h.add(entry("   ", "local"), 1).unwrap());
        assert!(!h
            .add(entry(&"x".repeat(MAX_COMMAND_LEN + 1), "local"), 1)
            .unwrap());
        assert_eq!(h.count().unwrap(), 0);
    }

    #[test]
    fn a_repeat_on_the_same_host_refreshes_the_last_entry() {
        let (_d, h) = store();
        h.add(entry("make", "local"), 1).unwrap();
        h.add(entry("make", "local"), 2).unwrap();
        h.add(entry("make", "web1"), 3).unwrap();
        h.add(entry("ls", "local"), 4).unwrap();
        h.add(entry("make", "local"), 5).unwrap();
        let all = h.list().unwrap();
        assert_eq!(all.len(), 4);
        assert_eq!(all[0].ts, 5);
        assert_eq!(all.iter().filter(|e| e.ts == 2).count(), 1);
    }

    #[test]
    fn old_entries_are_pruned() {
        let (_d, h) = store();
        h.add(entry("old", "local"), 1_000).unwrap();
        h.add(entry("new", "local"), 1_000 + MAX_AGE_MS + 1)
            .unwrap();
        let all = h.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].command, "new");
    }

    #[test]
    fn delete_removes_the_text_from_disk() {
        let (dir, h) = store();
        h.add(entry("curl secret-host-xyz", "local"), 1).unwrap();
        h.add(entry("ls", "local"), 2).unwrap();
        assert_eq!(h.delete_command("curl secret-host-xyz").unwrap(), 1);
        assert_eq!(h.count().unwrap(), 1);
        let bytes = std::fs::read(dir.path().join("history.db")).unwrap();
        assert!(!bytes.windows(15).any(|w| w == b"secret-host-xyz"));
    }

    #[test]
    fn clear_removes_the_file() {
        let (dir, h) = store();
        h.add(entry("ls", "local"), 1).unwrap();
        assert!(dir.path().join("history.db").exists());
        h.clear().unwrap();
        assert!(!dir.path().join("history.db").exists());
        assert!(h.list().unwrap().is_empty());
        h.clear().unwrap();
        // Recording again starts a fresh file.
        h.add(entry("pwd", "local"), 2).unwrap();
        assert_eq!(h.count().unwrap(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, h) = store();
        h.add(entry("ls", "local"), 1).unwrap();
        let mode = std::fs::metadata(dir.path().join("history.db"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn with_a_master_password_entries_are_sealed_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let (unlocked, _k, src) = vault();
        let h = History::new(dir.path().join("history.db"), src);
        // Locked: nothing is saved and nothing is written.
        assert!(!h.add(entry("ls", "local"), 1).unwrap());
        assert!(!dir.path().join("history.db").exists());
        assert!(h.list().unwrap().is_empty());

        unlocked.store(true, Ordering::SeqCst);
        let mut e = entry("curl https://secret-host-xyz/api", "web-secret-host");
        e.cwd = Some("/srv/secret-dir".into());
        assert!(h.add(e.clone(), 10).unwrap());
        assert!(
            h.add(e, 11).unwrap(),
            "a repeat refreshes, through the seal"
        );
        h.add(entry("ls", "local"), 12).unwrap();
        for needle in [&b"secret-host-xyz"[..], b"web-secret-host", b"secret-dir"] {
            assert!(
                !file_has(&dir, needle),
                "{needle:?} is on disk in plain text"
            );
        }
        let all = h.list().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].command, "curl https://secret-host-xyz/api");
        assert_eq!(all[1].ts, 11);
        assert_eq!(all[1].cwd.as_deref(), Some("/srv/secret-dir"));
        assert_eq!(h.delete_command("ls").unwrap(), 1);

        // Locked again: reading and deleting say why; counting and forgetting still work.
        unlocked.store(false, Ordering::SeqCst);
        assert!(h.list().unwrap_err().contains("Unlock"));
        assert!(h.delete_command("x").unwrap_err().contains("Unlock"));
        assert!(!h.add(entry("pwd", "local"), 13).unwrap());
        assert_eq!(h.count().unwrap(), 1);
        h.clear().unwrap();
        assert!(!dir.path().join("history.db").exists());
    }

    #[test]
    fn plain_entries_are_sealed_once_a_key_is_available() {
        let dir = tempfile::tempdir().unwrap();
        let h = History::new(dir.path().join("history.db"), plain());
        h.add(entry("rsync old-plain-entry", "old-host-name"), 1)
            .unwrap();
        drop(h);
        assert!(file_has(&dir, b"old-plain-entry"));

        let (unlocked, _k, src) = vault();
        unlocked.store(true, Ordering::SeqCst);
        let h = History::new(dir.path().join("history.db"), src);
        let all = h.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].host, "old-host-name");
        assert!(!file_has(&dir, b"old-plain-entry"));
        assert!(!file_has(&dir, b"old-host-name"));
    }

    #[test]
    fn a_version_1_file_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.db");
        let c = Connection::open(&path).unwrap();
        c.execute_batch(
            "CREATE TABLE entries (id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, command TEXT NOT NULL,
               host TEXT NOT NULL, session_id INTEGER, cwd TEXT, exit INTEGER);
             INSERT INTO entries (ts, command, host) VALUES (5, 'make', 'local');
             PRAGMA user_version = 1;",
        )
        .unwrap();
        drop(c);
        let h = History::new(path, plain());
        assert_eq!(h.list().unwrap()[0].command, "make");
        h.add(entry("ls", "local"), 6).unwrap();
        assert_eq!(h.count().unwrap(), 2);
    }

    #[test]
    fn import_adds_only_what_is_missing() {
        let (_d, h) = store();
        h.add(entry("make", "local"), 100).unwrap();
        let mut theirs = store().1;
        theirs.add(entry("make", "local"), 100).unwrap();
        theirs.add(entry("deploy", "web1"), 200).unwrap();
        theirs.add(entry(" hidden", "web1"), 300).unwrap();
        let incoming = theirs.list().unwrap();
        theirs = store().1;
        drop(theirs);
        assert_eq!(h.import(incoming.clone(), 1_000).unwrap(), 1);
        assert_eq!(
            h.import(incoming, 1_000).unwrap(),
            0,
            "a second import adds nothing"
        );
        let all = h.list().unwrap();
        assert_eq!(
            all.iter().map(|e| e.command.as_str()).collect::<Vec<_>>(),
            ["deploy", "make"]
        );
        // Too old for the retention window: dropped.
        let mut old = all[0].clone();
        old.command = "ancient".into();
        old.ts = 0;
        assert_eq!(h.import(vec![old], MAX_AGE_MS + 10).unwrap(), 0);
    }
}
