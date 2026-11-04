use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use quay_core::{TunnelKind, TunnelSpec};

use crate::model::{AuthKind, Folder, SavedSession, SessionInput};
use crate::sshconfig::ImportedHost;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("session database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Result of importing hosts from an ssh_config file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportSummary {
    pub added: usize,
    /// Already saved (same name, host, port and user).
    pub skipped: usize,
}

const SCHEMA_VERSION: i32 = 3;

/// Longest chain of jump hosts we follow; a longer one is almost surely a mistake.
pub const MAX_JUMPS: usize = 8;

pub struct SessionStore {
    conn: Connection,
}

impl SessionStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let mut version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(Error::Invalid(format!(
                "the session database was written by a newer quay-term (schema {version})"
            )));
        }
        if version < 1 {
            conn.execute_batch(
                "CREATE TABLE folders (
                     id INTEGER PRIMARY KEY,
                     name TEXT NOT NULL,
                     parent_id INTEGER REFERENCES folders(id) ON DELETE SET NULL
                 );
                 CREATE TABLE sessions (
                     id INTEGER PRIMARY KEY,
                     name TEXT NOT NULL,
                     folder_id INTEGER REFERENCES folders(id) ON DELETE SET NULL,
                     host TEXT NOT NULL,
                     port INTEGER NOT NULL,
                     user TEXT NOT NULL,
                     auth TEXT NOT NULL,
                     key_path TEXT,
                     color TEXT,
                     has_secret INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE TABLE session_tags (
                     session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                     tag TEXT NOT NULL COLLATE NOCASE,
                     PRIMARY KEY (session_id, tag)
                 );",
            )?;
            conn.pragma_update(None, "user_version", 1)?;
            version = 1;
        }
        if version < 2 {
            // Connect through another saved session.
            conn.execute_batch(
                "ALTER TABLE sessions ADD COLUMN jump_id INTEGER REFERENCES sessions(id) ON DELETE SET NULL;",
            )?;
            conn.pragma_update(None, "user_version", 2)?;
            version = 2;
        }
        if version < 3 {
            // Port forwards saved with a session, in the order the user arranged them.
            conn.execute_batch(
                "CREATE TABLE session_tunnels (
                     session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                     position INTEGER NOT NULL,
                     kind TEXT NOT NULL,
                     bind_host TEXT NOT NULL,
                     bind_port INTEGER NOT NULL,
                     dest_host TEXT NOT NULL,
                     dest_port INTEGER NOT NULL,
                     auto_start INTEGER NOT NULL DEFAULT 0,
                     PRIMARY KEY (session_id, position)
                 );",
            )?;
            conn.pragma_update(None, "user_version", 3)?;
        }
        Ok(Self { conn })
    }

    // ---- sessions ----

    pub fn create_session(&mut self, input: &SessionInput) -> Result<SavedSession> {
        let input = clean(input)?;
        self.check_jump(None, input.jump_id)?;
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO sessions (name, folder_id, host, port, user, auth, key_path, color, jump_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                input.name,
                input.folder_id,
                input.host,
                input.port,
                input.user,
                input.auth.as_str(),
                input.key_path,
                input.color,
                input.jump_id
            ],
        )?;
        let id = tx.last_insert_rowid();
        write_tags(&tx, id, &input.tags)?;
        write_tunnels(&tx, id, &input.tunnels)?;
        tx.commit()?;
        Ok(SavedSession {
            id,
            input,
            has_secret: false,
        })
    }

    pub fn update_session(&mut self, id: i64, input: &SessionInput) -> Result<SavedSession> {
        let input = clean(input)?;
        self.check_jump(Some(id), input.jump_id)?;
        let tx = self.conn.transaction()?;
        let changed = tx.execute(
            "UPDATE sessions SET name=?2, folder_id=?3, host=?4, port=?5, user=?6,
                                 auth=?7, key_path=?8, color=?9, jump_id=?10 WHERE id=?1",
            params![
                id,
                input.name,
                input.folder_id,
                input.host,
                input.port,
                input.user,
                input.auth.as_str(),
                input.key_path,
                input.color,
                input.jump_id
            ],
        )?;
        if changed == 0 {
            return Err(Error::Invalid("that saved session no longer exists".into()));
        }
        write_tags(&tx, id, &input.tags)?;
        write_tunnels(&tx, id, &input.tunnels)?;
        tx.commit()?;
        self.get_session(id)?
            .ok_or_else(|| Error::Invalid("that saved session no longer exists".into()))
    }

    /// Record whether the keychain holds a secret for this session.
    pub fn set_has_secret(&mut self, id: i64, has_secret: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET has_secret = ?2 WHERE id = ?1",
            params![id, has_secret],
        )?;
        Ok(())
    }

    pub fn delete_session(&mut self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM sessions WHERE id = ?1", [id])?;
        Ok(())
    }

    /// The jump hosts `id` is reached through, first hop first (not `id` itself).
    pub fn jump_chain(&self, id: i64) -> Result<Vec<SavedSession>> {
        let mut chain = Vec::new();
        let mut next = self
            .get_session(id)?
            .ok_or_else(|| Error::Invalid("that saved session no longer exists".into()))?
            .input
            .jump_id;
        while let Some(jid) = next {
            if jid == id || chain.iter().any(|s: &SavedSession| s.id == jid) {
                return Err(Error::Invalid("the jump hosts form a loop".into()));
            }
            if chain.len() >= MAX_JUMPS {
                return Err(Error::Invalid(format!(
                    "more than {MAX_JUMPS} jump hosts in a row"
                )));
            }
            let hop = self
                .get_session(jid)?
                .ok_or_else(|| Error::Invalid("a jump host no longer exists".into()))?;
            next = hop.input.jump_id;
            chain.push(hop);
        }
        chain.reverse();
        Ok(chain)
    }

    /// Reject a jump host that does not exist or would make a loop or an
    /// absurdly long chain. `id` is the session being saved, if it exists yet.
    fn check_jump(&self, id: Option<i64>, jump_id: Option<i64>) -> Result<()> {
        let Some(jid) = jump_id else { return Ok(()) };
        if Some(jid) == id {
            return Err(Error::Invalid(
                "a session cannot jump through itself".into(),
            ));
        }
        if self.get_session(jid)?.is_none() {
            return Err(Error::Invalid(
                "the chosen jump host no longer exists".into(),
            ));
        }
        // Walk the chain from the jump host; meeting `id` again means a loop.
        let mut seen = 0;
        let mut cur = Some(jid);
        while let Some(c) = cur {
            if Some(c) == id {
                return Err(Error::Invalid(
                    "that would make a loop of jump hosts".into(),
                ));
            }
            seen += 1;
            if seen > MAX_JUMPS {
                return Err(Error::Invalid(format!(
                    "more than {MAX_JUMPS} jump hosts in a row"
                )));
            }
            cur = self.get_session(c)?.and_then(|s| s.input.jump_id);
        }
        Ok(())
    }

    pub fn get_session(&self, id: i64) -> Result<Option<SavedSession>> {
        Ok(self
            .query_sessions("WHERE s.id = ?1", vec![id.to_string()])?
            .into_iter()
            .next())
    }

    /// All sessions, ordered by name.
    pub fn list_sessions(&self) -> Result<Vec<SavedSession>> {
        self.query_sessions("", vec![])
    }

    /// Sessions whose name, host, user or any tag contains every word of
    /// `query` (case-insensitive).
    pub fn search_sessions(&self, query: &str) -> Result<Vec<SavedSession>> {
        let words: Vec<&str> = query.split_whitespace().collect();
        if words.is_empty() {
            return self.list_sessions();
        }
        let mut clauses = Vec::new();
        let mut args = Vec::new();
        for (i, w) in words.iter().enumerate() {
            let like = format!(
                "%{}%",
                w.replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            );
            clauses.push(format!(
                "(s.name LIKE ?{n} ESCAPE '\\' OR s.host LIKE ?{n} ESCAPE '\\' OR s.user LIKE ?{n} ESCAPE '\\'
                  OR EXISTS (SELECT 1 FROM session_tags t WHERE t.session_id = s.id AND t.tag LIKE ?{n} ESCAPE '\\'))",
                n = i + 1
            ));
            args.push(like);
        }
        self.query_sessions(&format!("WHERE {}", clauses.join(" AND ")), args)
    }

    fn query_sessions(&self, filter: &str, args: Vec<String>) -> Result<Vec<SavedSession>> {
        let sql = format!(
            "SELECT s.id, s.name, s.folder_id, s.host, s.port, s.user, s.auth, s.key_path, s.color, s.has_secret, s.jump_id
             FROM sessions s {filter} ORDER BY s.name COLLATE NOCASE, s.id"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut sessions = stmt
            .query_map(rusqlite::params_from_iter(args), |r| {
                Ok(SavedSession {
                    id: r.get(0)?,
                    input: SessionInput {
                        name: r.get(1)?,
                        folder_id: r.get(2)?,
                        host: r.get(3)?,
                        port: r.get(4)?,
                        user: r.get(5)?,
                        auth: {
                            let raw = r.get::<_, String>(6)?;
                            AuthKind::parse(&raw).ok_or_else(|| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    6,
                                    rusqlite::types::Type::Text,
                                    format!("unknown auth method {raw:?}; was this saved by a newer version?").into(),
                                )
                            })?
                        },
                        key_path: r.get(7)?,
                        color: r.get(8)?,
                        tags: vec![],
                        jump_id: r.get(10)?,
                        tunnels: vec![],
                    },
                    has_secret: r.get(9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut tag_stmt = self.conn.prepare(
            "SELECT tag FROM session_tags WHERE session_id = ?1 ORDER BY tag COLLATE NOCASE",
        )?;
        let mut tunnel_stmt = self.conn.prepare(
            "SELECT kind, bind_host, bind_port, dest_host, dest_port, auto_start
             FROM session_tunnels WHERE session_id = ?1 ORDER BY position",
        )?;
        for s in &mut sessions {
            s.input.tags = tag_stmt
                .query_map([s.id], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?;
            s.input.tunnels = tunnel_stmt
                .query_map([s.id], |r| {
                    Ok(TunnelSpec {
                        kind: match r.get::<_, String>(0)?.as_str() {
                            "remote" => TunnelKind::Remote,
                            "dynamic" => TunnelKind::Dynamic,
                            _ => TunnelKind::Local,
                        },
                        bind_host: r.get(1)?,
                        bind_port: r.get(2)?,
                        dest_host: r.get(3)?,
                        dest_port: r.get(4)?,
                        auto_start: r.get(5)?,
                    })
                })?
                .collect::<std::result::Result<_, _>>()?;
        }
        Ok(sessions)
    }

    // ---- folders ----

    pub fn create_folder(&mut self, name: &str, parent_id: Option<i64>) -> Result<Folder> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("a folder needs a name".into()));
        }
        self.conn.execute(
            "INSERT INTO folders (name, parent_id) VALUES (?1, ?2)",
            params![name, parent_id],
        )?;
        Ok(Folder {
            id: self.conn.last_insert_rowid(),
            name: name.to_string(),
            parent_id,
        })
    }

    pub fn rename_folder(&mut self, id: i64, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("a folder needs a name".into()));
        }
        self.conn.execute(
            "UPDATE folders SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        Ok(())
    }

    /// Move a session into a folder (`None` = top level).
    pub fn move_session(&mut self, id: i64, folder_id: Option<i64>) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE sessions SET folder_id = ?2 WHERE id = ?1",
            params![id, folder_id],
        )?;
        if changed == 0 {
            return Err(Error::Invalid("that saved session no longer exists".into()));
        }
        Ok(())
    }

    /// Move a folder under another (`None` = top level). A folder cannot go
    /// inside itself or one of its own subfolders.
    pub fn move_folder(&mut self, id: i64, parent_id: Option<i64>) -> Result<()> {
        let mut cursor = parent_id;
        while let Some(p) = cursor {
            if p == id {
                return Err(Error::Invalid(
                    "A folder cannot be moved into itself or one of its subfolders.".into(),
                ));
            }
            cursor = self
                .conn
                .query_row("SELECT parent_id FROM folders WHERE id = ?1", [p], |r| {
                    r.get(0)
                })
                .optional()?
                .flatten();
        }
        let changed = self.conn.execute(
            "UPDATE folders SET parent_id = ?2 WHERE id = ?1",
            params![id, parent_id],
        )?;
        if changed == 0 {
            return Err(Error::Invalid("that folder no longer exists".into()));
        }
        Ok(())
    }

    /// Delete a folder. Its sessions and subfolders move up to its parent
    /// rather than being deleted.
    pub fn delete_folder(&mut self, id: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        let parent: Option<i64> = tx
            .query_row("SELECT parent_id FROM folders WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?
            .flatten();
        tx.execute(
            "UPDATE sessions SET folder_id = ?2 WHERE folder_id = ?1",
            params![id, parent],
        )?;
        tx.execute(
            "UPDATE folders SET parent_id = ?2 WHERE parent_id = ?1",
            params![id, parent],
        )?;
        tx.execute("DELETE FROM folders WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn list_folders(&self) -> Result<Vec<Folder>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, parent_id FROM folders ORDER BY name COLLATE NOCASE, id")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Folder {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    parent_id: r.get(2)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(rows)
    }

    // ---- ssh_config import ----

    /// Save hosts from an ssh_config as sessions in `folder_id`. Hosts that
    /// are already saved are skipped, so importing twice is harmless.
    /// Returns the summary and the sessions that were added.
    pub fn import_hosts(
        &mut self,
        hosts: &[ImportedHost],
        folder_id: Option<i64>,
    ) -> Result<(ImportSummary, Vec<SavedSession>)> {
        let mut summary = ImportSummary::default();
        let mut added: Vec<SavedSession> = Vec::new();
        let existing = self.list_sessions()?;
        for h in hosts {
            let input = h.to_input(folder_id);
            let dup = existing.iter().chain(added.iter()).any(|s| {
                s.input.name == input.name
                    && s.input.host == input.host
                    && s.input.port == input.port
                    && s.input.user == input.user
            });
            if dup {
                summary.skipped += 1;
            } else {
                added.push(self.create_session(&input)?);
                summary.added += 1;
            }
        }
        // Wire up ProxyJump now that every host has an id.
        let everyone: Vec<SavedSession> = existing.iter().chain(added.iter()).cloned().collect();
        for h in hosts {
            let Some(spec) = &h.proxy_jump else { continue };
            let Some(me) = added.iter().find(|s| s.input.name == h.alias).map(|s| s.id) else {
                continue;
            };
            let hops: Option<Vec<i64>> = spec
                .split(',')
                .map(|hop| resolve_hop(&everyone, hop.trim()))
                .collect();
            let Some(hops) = hops.filter(|h| !h.is_empty()) else {
                continue; // keeps its "proxyjump" tag so the gap is visible
            };
            // a,b means: target via b, b via a (unless b already says otherwise).
            let link = |who: i64, via: i64| -> Result<()> {
                if self
                    .get_session(who)?
                    .is_some_and(|s| s.input.jump_id.is_none())
                    && self.check_jump(Some(who), Some(via)).is_ok()
                {
                    self.conn.execute(
                        "UPDATE sessions SET jump_id = ?2 WHERE id = ?1",
                        params![who, via],
                    )?;
                }
                Ok(())
            };
            for pair in hops.windows(2) {
                link(pair[1], pair[0])?;
            }
            link(me, *hops.last().unwrap())?;
            self.conn.execute(
                "DELETE FROM session_tags WHERE session_id = ?1 AND tag = 'proxyjump'",
                [me],
            )?;
        }
        let added = added
            .iter()
            .filter_map(|s| self.get_session(s.id).transpose())
            .collect::<Result<Vec<_>>>()?;
        Ok((summary, added))
    }
}

/// Find the saved session a ProxyJump hop names: by alias, else `[user@]host[:port]`.
fn resolve_hop(sessions: &[SavedSession], hop: &str) -> Option<i64> {
    if let Some(s) = sessions.iter().find(|s| s.input.name == hop) {
        return Some(s.id);
    }
    let (user, rest) = match hop.split_once('@') {
        Some((u, r)) => (Some(u), r),
        None => (None, hop),
    };
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().ok()?),
        None => (rest, 22),
    };
    sessions
        .iter()
        .find(|s| {
            s.input.host == host && s.input.port == port && user.map_or(true, |u| s.input.user == u)
        })
        .map(|s| s.id)
}

fn clean(input: &SessionInput) -> Result<SessionInput> {
    let mut out = input.clone();
    out.host = out.host.trim().to_string();
    out.user = out.user.trim().to_string();
    out.name = out.name.trim().to_string();
    if out.host.is_empty() {
        return Err(Error::Invalid("a host is required".into()));
    }
    if out.user.is_empty() {
        return Err(Error::Invalid("a user name is required".into()));
    }
    if out.port == 0 {
        return Err(Error::Invalid(
            "the port must be between 1 and 65535".into(),
        ));
    }
    if out.name.is_empty() {
        out.name = format!("{}@{}", out.user, out.host);
    }
    out.key_path = out
        .key_path
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty());
    if out.auth == AuthKind::Key && out.key_path.is_none() {
        return Err(Error::Invalid("choose a private key file".into()));
    }
    // Tags: trimmed, no empties, no case-insensitive duplicates.
    let mut tags: Vec<String> = Vec::new();
    for t in out.tags.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
        if !tags.iter().any(|x| x.eq_ignore_ascii_case(t)) {
            tags.push(t.to_string());
        }
    }
    out.tags = tags;
    for t in &mut out.tunnels {
        t.bind_host = t.bind_host.trim().to_string();
        t.dest_host = t.dest_host.trim().to_string();
        if t.bind_host.is_empty() {
            return Err(Error::Invalid(
                "a tunnel needs an address to listen on".into(),
            ));
        }
        match t.kind {
            TunnelKind::Dynamic => {
                // Nothing to point at: the SOCKS client names the destination.
                t.dest_host.clear();
                t.dest_port = 0;
            }
            _ if t.dest_host.is_empty() || t.dest_port == 0 => {
                return Err(Error::Invalid(
                    "a tunnel needs a destination host and port".into(),
                ));
            }
            _ => {}
        }
    }
    Ok(out)
}

fn write_tunnels(tx: &rusqlite::Transaction<'_>, id: i64, tunnels: &[TunnelSpec]) -> Result<()> {
    tx.execute("DELETE FROM session_tunnels WHERE session_id = ?1", [id])?;
    for (i, t) in tunnels.iter().enumerate() {
        let kind = match t.kind {
            TunnelKind::Local => "local",
            TunnelKind::Remote => "remote",
            TunnelKind::Dynamic => "dynamic",
        };
        tx.execute(
            "INSERT INTO session_tunnels
                 (session_id, position, kind, bind_host, bind_port, dest_host, dest_port, auto_start)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, i as i64, kind, t.bind_host, t.bind_port, t.dest_host, t.dest_port, t.auto_start],
        )?;
    }
    Ok(())
}

fn write_tags(tx: &rusqlite::Transaction<'_>, id: i64, tags: &[String]) -> Result<()> {
    tx.execute("DELETE FROM session_tags WHERE session_id = ?1", [id])?;
    for t in tags {
        tx.execute(
            "INSERT INTO session_tags (session_id, tag) VALUES (?1, ?2)",
            params![id, t],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sshconfig::parse_ssh_config;

    fn input(name: &str, host: &str) -> SessionInput {
        SessionInput {
            name: name.into(),
            folder_id: None,
            host: host.into(),
            port: 22,
            user: "me".into(),
            auth: AuthKind::Agent,
            key_path: None,
            color: None,
            tags: vec![],
            jump_id: None,
            tunnels: vec![],
        }
    }

    #[test]
    fn unknown_auth_method_is_an_error_not_agent() {
        let mut s = SessionStore::open_in_memory().unwrap();
        let made = s.create_session(&input("web", "web.example.com")).unwrap();
        s.conn
            .execute(
                "UPDATE sessions SET auth = 'smartcard' WHERE id = ?1",
                [made.id],
            )
            .unwrap();
        let err = s.list_sessions().unwrap_err().to_string();
        assert!(err.contains("smartcard"), "{err}");
    }

    #[test]
    fn create_get_update_delete() {
        let mut s = SessionStore::open_in_memory().unwrap();
        let mut i = input("web", "web.example.com");
        i.tags = vec!["prod".into(), "Prod".into(), " ".into(), "eu".into()];
        let made = s.create_session(&i).unwrap();
        assert_eq!(
            made.input.tags,
            ["prod", "eu"],
            "tags are trimmed and de-duplicated"
        );

        let got = s.get_session(made.id).unwrap().unwrap();
        assert!(!got.has_secret);
        s.set_has_secret(made.id, true).unwrap();
        assert!(s.get_session(made.id).unwrap().unwrap().has_secret);
        assert_eq!(got.input.tags, ["eu", "prod"]); // listed alphabetically
        assert_eq!(got.input.host, "web.example.com");

        let mut changed = i.clone();
        changed.port = 2200;
        changed.auth = AuthKind::Key;
        changed.key_path = Some("~/.ssh/id".into());
        changed.tags = vec!["staging".into()];
        s.update_session(made.id, &changed).unwrap();
        let got = s.get_session(made.id).unwrap().unwrap();
        assert_eq!((got.input.port, got.input.auth), (2200, AuthKind::Key));
        assert_eq!(got.input.tags, ["staging"]);

        s.delete_session(made.id).unwrap();
        assert!(s.get_session(made.id).unwrap().is_none());
        assert!(s.list_sessions().unwrap().is_empty());
        // Tags went with it.
        let n: i64 = s
            .conn
            .query_row("SELECT COUNT(*) FROM session_tags", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn validation() {
        let mut s = SessionStore::open_in_memory().unwrap();
        assert!(s.create_session(&input("x", "  ")).is_err());
        let mut no_user = input("x", "h");
        no_user.user = "".into();
        assert!(s.create_session(&no_user).is_err());
        let mut key = input("x", "h");
        key.auth = AuthKind::Key;
        assert!(s.create_session(&key).is_err(), "key auth needs a key path");
        assert!(s.update_session(999, &input("x", "h")).is_err());
        // A blank name falls back to user@host.
        assert_eq!(
            s.create_session(&input("", "h")).unwrap().input.name,
            "me@h"
        );
    }

    #[test]
    fn search_matches_name_host_user_and_tags() {
        let mut s = SessionStore::open_in_memory().unwrap();
        let mut a = input("Build box", "ci.example.com");
        a.tags = vec!["ci".into(), "linux".into()];
        let mut b = input("Database", "db-1.internal");
        b.user = "postgres".into();
        s.create_session(&a).unwrap();
        s.create_session(&b).unwrap();

        let names = |q: &str| -> Vec<String> {
            s.search_sessions(q)
                .unwrap()
                .into_iter()
                .map(|x| x.input.name)
                .collect()
        };
        assert_eq!(names(""), ["Build box", "Database"]);
        assert_eq!(names("BUILD"), ["Build box"]);
        assert_eq!(names("internal"), ["Database"]);
        assert_eq!(names("postgres"), ["Database"]);
        assert_eq!(names("linux"), ["Build box"]);
        assert_eq!(names("ci linux"), ["Build box"], "every word must match");
        assert_eq!(names("ci postgres"), Vec::<String>::new());
        assert_eq!(names("100%"), Vec::<String>::new(), "wildcards are literal");
    }

    #[test]
    fn deleting_a_folder_moves_contents_up() {
        let mut s = SessionStore::open_in_memory().unwrap();
        let top = s.create_folder("Work", None).unwrap();
        let mid = s.create_folder("Prod", Some(top.id)).unwrap();
        let leaf = s.create_folder("EU", Some(mid.id)).unwrap();
        let mut i = input("a", "h");
        i.folder_id = Some(mid.id);
        let sess = s.create_session(&i).unwrap();

        s.delete_folder(mid.id).unwrap();
        let folders = s.list_folders().unwrap();
        assert_eq!(folders.len(), 2);
        assert_eq!(
            folders.iter().find(|f| f.id == leaf.id).unwrap().parent_id,
            Some(top.id)
        );
        assert_eq!(
            s.get_session(sess.id).unwrap().unwrap().input.folder_id,
            Some(top.id)
        );

        s.rename_folder(top.id, " Office ").unwrap();
        assert!(s.list_folders().unwrap().iter().any(|f| f.name == "Office"));
        assert!(s.create_folder("  ", None).is_err());
    }

    #[test]
    fn moving_sessions_and_folders() {
        let mut s = SessionStore::open_in_memory().unwrap();
        let work = s.create_folder("Work", None).unwrap();
        let prod = s.create_folder("Prod", Some(work.id)).unwrap();
        let sess = s.create_session(&input("a", "h")).unwrap();

        s.move_session(sess.id, Some(prod.id)).unwrap();
        assert_eq!(
            s.get_session(sess.id).unwrap().unwrap().input.folder_id,
            Some(prod.id)
        );
        s.move_session(sess.id, None).unwrap();
        assert_eq!(
            s.get_session(sess.id).unwrap().unwrap().input.folder_id,
            None
        );
        assert!(s.move_session(999, None).is_err());

        // Re-parent, and back to the top level.
        let other = s.create_folder("Other", None).unwrap();
        s.move_folder(prod.id, Some(other.id)).unwrap();
        let parent_of = |s: &SessionStore, id| {
            s.list_folders()
                .unwrap()
                .into_iter()
                .find(|f| f.id == id)
                .unwrap()
                .parent_id
        };
        assert_eq!(parent_of(&s, prod.id), Some(other.id));
        s.move_folder(prod.id, None).unwrap();
        assert_eq!(parent_of(&s, prod.id), None);
    }

    #[test]
    fn folders_cannot_move_into_themselves_or_descendants() {
        let mut s = SessionStore::open_in_memory().unwrap();
        let a = s.create_folder("A", None).unwrap();
        let b = s.create_folder("B", Some(a.id)).unwrap();
        let c = s.create_folder("C", Some(b.id)).unwrap();
        assert!(s.move_folder(a.id, Some(a.id)).is_err());
        assert!(s.move_folder(a.id, Some(c.id)).is_err());
        assert!(s.move_folder(b.id, Some(c.id)).is_err());
        // Nothing changed.
        let folders = s.list_folders().unwrap();
        assert_eq!(
            folders.iter().find(|f| f.id == a.id).unwrap().parent_id,
            None
        );
        assert!(s.move_folder(c.id, Some(a.id)).is_ok());
    }

    #[test]
    fn jump_hosts_chain_and_reject_loops() {
        let mut s = SessionStore::open_in_memory().unwrap();
        let a = s.create_session(&input("a", "a.example")).unwrap();
        let mut ib = input("b", "b.example");
        ib.jump_id = Some(a.id);
        let b = s.create_session(&ib).unwrap();
        let mut ic = input("c", "c.example");
        ic.jump_id = Some(b.id);
        let c = s.create_session(&ic).unwrap();

        let names = |v: Vec<SavedSession>| v.into_iter().map(|s| s.input.name).collect::<Vec<_>>();
        assert_eq!(names(s.jump_chain(c.id).unwrap()), ["a", "b"]);
        assert!(s.jump_chain(a.id).unwrap().is_empty());

        // a -> c would close the loop a, c, b, a.
        let mut ia = input("a", "a.example");
        ia.jump_id = Some(c.id);
        let err = s.update_session(a.id, &ia).unwrap_err().to_string();
        assert!(err.contains("loop"), "{err}");
        ia.jump_id = Some(a.id);
        assert!(s.update_session(a.id, &ia).is_err(), "not through itself");
        ia.jump_id = Some(9999);
        assert!(s.update_session(a.id, &ia).is_err(), "must exist");

        // Deleting a jump host leaves its dependants direct, not dangling.
        s.delete_session(a.id).unwrap();
        assert_eq!(s.get_session(b.id).unwrap().unwrap().input.jump_id, None);
    }

    #[test]
    fn existing_database_gains_the_jump_column() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sessions.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE folders (id INTEGER PRIMARY KEY, name TEXT NOT NULL,
                     parent_id INTEGER REFERENCES folders(id) ON DELETE SET NULL);
                 CREATE TABLE sessions (id INTEGER PRIMARY KEY, name TEXT NOT NULL,
                     folder_id INTEGER REFERENCES folders(id) ON DELETE SET NULL,
                     host TEXT NOT NULL, port INTEGER NOT NULL, user TEXT NOT NULL,
                     auth TEXT NOT NULL, key_path TEXT, color TEXT,
                     has_secret INTEGER NOT NULL DEFAULT 0);
                 CREATE TABLE session_tags (
                     session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                     tag TEXT NOT NULL COLLATE NOCASE, PRIMARY KEY (session_id, tag));
                 INSERT INTO sessions (name, host, port, user, auth) VALUES ('old','h',22,'u','agent');
                 PRAGMA user_version = 1;",
            )
            .unwrap();
        }
        let s = SessionStore::open(&path).unwrap();
        let all = s.list_sessions().unwrap();
        assert_eq!((all.len(), all[0].input.jump_id), (1, None));
    }

    #[test]
    fn import_links_proxy_jump_to_known_hosts() {
        let mut s = SessionStore::open_in_memory().unwrap();
        let hosts = parse_ssh_config(
            "Host bastion\n  HostName b.example\n  User u\n\
             Host inner\n  HostName i.example\n  User u\n  ProxyJump bastion\n\
             Host far\n  HostName f.example\n  User u\n  ProxyJump bastion,inner\n\
             Host odd\n  HostName o.example\n  User u\n  ProxyJump nobody-knows\n",
        );
        let (_, added) = s.import_hosts(&hosts, None).unwrap();
        let by = |n: &str| added.iter().find(|x| x.input.name == n).unwrap().clone();
        assert_eq!(by("inner").input.jump_id, Some(by("bastion").id));
        assert_eq!(by("far").input.jump_id, Some(by("inner").id));
        assert!(!by("inner").input.tags.contains(&"proxyjump".to_string()));
        // An unresolvable jump stays visible as a tag instead of silently connecting direct.
        assert_eq!(by("odd").input.jump_id, None);
        assert!(by("odd").input.tags.contains(&"proxyjump".to_string()));
    }

    #[test]
    fn tunnels_are_saved_in_order_and_validated() {
        let mut s = SessionStore::open_in_memory().unwrap();
        let mut i = input("web", "web.example.com");
        i.tunnels = vec![
            TunnelSpec {
                kind: TunnelKind::Local,
                bind_port: 5433,
                dest_host: " db ".into(),
                dest_port: 5432,
                auto_start: true,
                ..TunnelSpec::default()
            },
            TunnelSpec {
                kind: TunnelKind::Dynamic,
                bind_port: 1080,
                dest_host: "ignored".into(),
                dest_port: 9,
                ..TunnelSpec::default()
            },
            TunnelSpec {
                kind: TunnelKind::Remote,
                bind_port: 0,
                dest_host: "127.0.0.1".into(),
                dest_port: 3000,
                ..TunnelSpec::default()
            },
        ];
        let made = s.create_session(&i).unwrap();
        let got = s.get_session(made.id).unwrap().unwrap().input.tunnels;
        assert_eq!(got.len(), 3);
        assert_eq!(
            got.iter().map(|t| t.kind).collect::<Vec<_>>(),
            [TunnelKind::Local, TunnelKind::Dynamic, TunnelKind::Remote]
        );
        assert_eq!(got[0].dest_host, "db", "trimmed");
        assert!(got[0].auto_start && !got[1].auto_start);
        assert_eq!((got[1].dest_host.as_str(), got[1].dest_port), ("", 0));

        // Saving replaces the list; a bad one is refused and changes nothing.
        let mut edit = i.clone();
        edit.tunnels.truncate(1);
        s.update_session(made.id, &edit).unwrap();
        assert_eq!(
            s.get_session(made.id).unwrap().unwrap().input.tunnels.len(),
            1
        );
        edit.tunnels[0].dest_port = 0;
        assert!(s.update_session(made.id, &edit).is_err());
        assert_eq!(
            s.get_session(made.id).unwrap().unwrap().input.tunnels.len(),
            1
        );

        // Deleting the session takes its tunnels with it.
        s.delete_session(made.id).unwrap();
        let n: i64 = s
            .conn
            .query_row("SELECT COUNT(*) FROM session_tunnels", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn import_is_idempotent() {
        let mut s = SessionStore::open_in_memory().unwrap();
        let hosts = parse_ssh_config("Host a\n  HostName a.example\n  User u\nHost b\n  User u\n");
        let (sum, added) = s.import_hosts(&hosts, None).unwrap();
        assert_eq!((sum.added, sum.skipped, added.len()), (2, 0, 2));
        let (sum, added) = s.import_hosts(&hosts, None).unwrap();
        assert_eq!((sum.added, sum.skipped, added.len()), (0, 2, 0));
        assert_eq!(s.list_sessions().unwrap().len(), 2);
    }

    #[test]
    fn data_persists_on_disk_and_future_schemas_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("sessions.db");
        {
            let mut s = SessionStore::open(&path).unwrap();
            s.create_session(&input("keep", "h")).unwrap();
        }
        let s = SessionStore::open(&path).unwrap();
        assert_eq!(s.list_sessions().unwrap().len(), 1);
        drop(s);

        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        drop(conn);
        assert!(matches!(SessionStore::open(&path), Err(Error::Invalid(_))));
    }
}
