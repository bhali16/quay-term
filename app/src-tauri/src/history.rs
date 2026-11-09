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
const SCHEMA_VERSION: i32 = 1;

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

pub(crate) struct History {
    path: PathBuf,
    conn: Mutex<Option<Connection>>,
}

fn now_ms() -> i64 {
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

impl History {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            path,
            conn: Mutex::new(None),
        }
    }

    /// Run `f` on the database. With `create` false and no file yet, returns `Ok(None)` so that
    /// reading never creates a file.
    fn with<T>(
        &self,
        create: bool,
        f: impl FnOnce(&mut Connection) -> rusqlite::Result<T>,
    ) -> Result<Option<T>, String> {
        let mut guard = self.conn.lock().unwrap();
        if guard.is_none() {
            if !create && !self.path.exists() {
                return Ok(None);
            }
            if let Some(dir) = self.path.parent() {
                std::fs::create_dir_all(dir).map_err(err)?;
            }
            restrict(&self.path).map_err(err)?;
            *guard = Some(Self::init(Connection::open(&self.path).map_err(err)?).map_err(err)?);
        }
        f(guard.as_mut().unwrap()).map(Some).map_err(err)
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
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(conn)
    }

    /// Save one command. Returns false when it was not saved (empty, too long, or typed with a
    /// leading space). The same command finishing again on the same host only refreshes the
    /// latest entry.
    pub(crate) fn add(&self, e: NewHistoryEntry, now: i64) -> Result<bool, String> {
        if e.command.starts_with(char::is_whitespace) {
            return Ok(false);
        }
        let command = e.command.trim_end().to_string();
        if command.is_empty() || command.len() > MAX_COMMAND_LEN {
            return Ok(false);
        }
        let host = clip(Some(e.host)).unwrap_or_else(|| "local".into());
        let cwd = clip(e.cwd);
        self.with(true, |c| {
            let tx = c.transaction()?;
            let refreshed = tx.execute(
                "UPDATE entries SET ts = ?1, exit = ?2, cwd = ?3
                 WHERE id = (SELECT MAX(id) FROM entries) AND command = ?4 AND host = ?5",
                params![now, e.exit, cwd, command, host],
            )?;
            if refreshed == 0 {
                tx.execute(
                    "INSERT INTO entries (ts, command, host, session_id, cwd, exit)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![now, command, host, e.session_id, cwd, e.exit],
                )?;
            }
            tx.execute(
                "DELETE FROM entries WHERE ts < ?1",
                [now.saturating_sub(MAX_AGE_MS)],
            )?;
            tx.execute(
                "DELETE FROM entries WHERE id <= (SELECT id FROM entries ORDER BY id DESC LIMIT 1 OFFSET ?1)",
                [MAX_ENTRIES],
            )?;
            tx.commit()
        })?;
        Ok(true)
    }

    /// Every entry, newest first.
    pub(crate) fn list(&self) -> Result<Vec<HistoryEntry>, String> {
        Ok(self
            .with(false, |c| {
                let mut st = c.prepare(
                    "SELECT id, ts, command, host, session_id, cwd, exit FROM entries ORDER BY ts DESC, id DESC",
                )?;
                let rows = st.query_map([], |r| {
                    Ok(HistoryEntry {
                        id: r.get(0)?,
                        ts: r.get(1)?,
                        command: r.get(2)?,
                        host: r.get(3)?,
                        session_id: r.get(4)?,
                        cwd: r.get(5)?,
                        exit: r.get(6)?,
                    })
                })?;
                rows.collect()
            })?
            .unwrap_or_default())
    }

    pub(crate) fn count(&self) -> Result<i64, String> {
        Ok(self
            .with(false, |c| {
                c.query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
            })?
            .unwrap_or(0))
    }

    /// Forget a command everywhere it was run. Returns how many entries went.
    pub(crate) fn delete_command(&self, command: &str) -> Result<usize, String> {
        Ok(self
            .with(false, |c| {
                c.execute("DELETE FROM entries WHERE command = ?1", [command])
            })?
            .unwrap_or(0))
    }

    /// Forget everything: close the database and remove its files from disk.
    pub(crate) fn clear(&self) -> Result<(), String> {
        let mut guard = self.conn.lock().unwrap();
        if let Some(conn) = guard.take() {
            conn.close().map_err(|(_, e)| err(e))?;
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

    fn entry(command: &str, host: &str) -> NewHistoryEntry {
        NewHistoryEntry {
            command: command.into(),
            host: host.into(),
            session_id: None,
            cwd: None,
            exit: Some(0),
        }
    }

    fn store() -> (tempfile::TempDir, History) {
        let dir = tempfile::tempdir().unwrap();
        let h = History::new(dir.path().join("history.db"));
        (dir, h)
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
        let h = History::new(dir.path().join("history.db"));
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
}
