//! Read hosts out of an OpenSSH client config (`~/.ssh/config`).
//!
//! Supports `Host` blocks (wildcards and `!` negation), `Match` blocks (their
//! options are ignored), `Include`, and the options quay-term understands:
//! `HostName`, `Port`, `User`, `IdentityFile` and `ProxyJump`. As in OpenSSH,
//! the first value found for an option wins, so a trailing `Host *` block
//! supplies defaults for the hosts above it.

use std::path::{Path, PathBuf};

use crate::model::{AuthKind, SessionInput};

/// One concrete host alias from the config, with options resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedHost {
    pub alias: String,
    pub host_name: String,
    pub port: u16,
    pub user: Option<String>,
    pub identity_file: Option<String>,
    pub proxy_jump: Option<String>,
}

impl ImportedHost {
    pub(crate) fn to_input(&self, folder_id: Option<i64>) -> SessionInput {
        let mut tags = Vec::new();
        if self.proxy_jump.is_some() {
            // Jump hosts are not supported yet; make that visible.
            tags.push("proxyjump".to_string());
        }
        SessionInput {
            name: self.alias.clone(),
            folder_id,
            host: self.host_name.clone(),
            port: self.port,
            user: self.user.clone().unwrap_or_else(current_user),
            auth: if self.identity_file.is_some() {
                AuthKind::Key
            } else {
                AuthKind::Agent
            },
            key_path: self.identity_file.clone(),
            color: None,
            tags,
        }
    }
}

fn current_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "user".to_string())
}

struct Block {
    /// Patterns from the `Host` line; empty for `Match` blocks.
    patterns: Vec<String>,
    opts: Vec<(String, String)>,
}

/// Parse config text. `Include` lines are ignored; use [`read_ssh_config`]
/// to follow them.
pub fn parse_ssh_config(text: &str) -> Vec<ImportedHost> {
    // Options before the first Host line apply to every host.
    let mut blocks = vec![Block {
        patterns: vec!["*".into()],
        opts: vec![],
    }];
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, args)) = split_keyword(line) else {
            continue;
        };
        match key.as_str() {
            "host" => blocks.push(Block {
                patterns: args,
                opts: vec![],
            }),
            "match" => blocks.push(Block {
                patterns: vec![],
                opts: vec![],
            }),
            _ => {
                if let (Some(b), Some(v)) = (blocks.last_mut(), args.first()) {
                    b.opts.push((key, v.clone()));
                }
            }
        }
    }

    let mut aliases: Vec<String> = Vec::new();
    for b in &blocks {
        for p in &b.patterns {
            if !p.contains(['*', '?', '!']) && !aliases.iter().any(|a| a.eq_ignore_ascii_case(p)) {
                aliases.push(p.clone());
            }
        }
    }
    aliases.iter().map(|a| resolve(a, &blocks)).collect()
}

fn resolve(alias: &str, blocks: &[Block]) -> ImportedHost {
    let first = |key: &str| {
        blocks
            .iter()
            .filter(|b| applies(&b.patterns, alias))
            .flat_map(|b| b.opts.iter())
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };
    let host_name = first("hostname")
        .map(|h| h.replace("%h", alias))
        .unwrap_or_else(|| alias.to_string());
    ImportedHost {
        alias: alias.to_string(),
        host_name,
        port: first("port").and_then(|p| p.parse().ok()).unwrap_or(22),
        user: first("user"),
        identity_file: first("identityfile"),
        proxy_jump: first("proxyjump").filter(|p| !p.eq_ignore_ascii_case("none")),
    }
}

/// Does a `Host` line with these patterns apply to `alias`?
fn applies(patterns: &[String], alias: &str) -> bool {
    let mut matched = false;
    for p in patterns {
        if let Some(neg) = p.strip_prefix('!') {
            if glob(neg, alias) {
                return false;
            }
        } else if glob(p, alias) {
            matched = true;
        }
    }
    matched
}

/// `*` and `?` wildcards, case-insensitive.
fn glob(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    fn go(p: &[char], t: &[char]) -> bool {
        match p.split_first() {
            None => t.is_empty(),
            Some(('*', rest)) => (0..=t.len()).any(|i| go(rest, &t[i..])),
            Some(('?', rest)) => !t.is_empty() && go(rest, &t[1..]),
            Some((c, rest)) => t.first() == Some(c) && go(rest, &t[1..]),
        }
    }
    go(&p, &t)
}

/// Split `Keyword value...` or `Keyword=value` into a lower-case keyword and
/// its arguments (double quotes group words).
fn split_keyword(line: &str) -> Option<(String, Vec<String>)> {
    let end = line.find(|c: char| c.is_whitespace() || c == '=')?;
    let key = line[..end].to_lowercase();
    let rest = line[end..].trim_start_matches(|c: char| c.is_whitespace() || c == '=');
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut has = false;
    for c in rest.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                has = true;
            }
            c if c.is_whitespace() && !quoted => {
                if has {
                    args.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            c => {
                cur.push(c);
                has = true;
            }
        }
    }
    if has {
        args.push(cur);
    }
    Some((key, args))
}

/// Read a config file, following `Include` lines (relative paths resolve
/// against the file's directory; `*` wildcards in the file name work).
pub fn read_ssh_config(path: &Path) -> std::io::Result<Vec<ImportedHost>> {
    let base = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut text = String::new();
    expand(path, &base, 0, &mut text)?;
    Ok(parse_ssh_config(&text))
}

fn expand(path: &Path, base: &Path, depth: usize, out: &mut String) -> std::io::Result<()> {
    let text = std::fs::read_to_string(path)?;
    for line in text.lines() {
        match split_keyword(line.trim()) {
            Some((k, args)) if k == "include" && depth < 5 => {
                for pattern in args {
                    for file in include_files(&pattern, base) {
                        // A broken include should not hide the rest of the config.
                        let _ = expand(&file, base, depth + 1, out);
                    }
                }
            }
            _ => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    Ok(())
}

fn include_files(pattern: &str, base: &Path) -> Vec<PathBuf> {
    let expanded = if let Some(rest) = pattern.strip_prefix("~/") {
        match std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            Some(h) => Path::new(&h).join(rest),
            None => return vec![],
        }
    } else {
        PathBuf::from(pattern)
    };
    let full = if expanded.is_absolute() {
        expanded
    } else {
        base.join(expanded)
    };
    let (Some(dir), Some(name)) = (full.parent(), full.file_name().and_then(|n| n.to_str())) else {
        return vec![];
    };
    if !name.contains(['*', '?']) {
        return vec![full];
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(|n| glob(name, n)))
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
# global default
ServerAliveInterval 30

Host web prod-web
    HostName web.example.com
    User deploy
    Port 2222
    IdentityFile ~/.ssh/id_web

Host db
    HostName=db.internal
    ProxyJump bastion

Host bastion
    HostName "bastion.example.com"

Host *.example.com !skip.example.com
    User fallback

Host *
    User everyone
    IdentityFile ~/.ssh/id_default

Match host foo
    User ignored
"#;

    fn find<'a>(hosts: &'a [ImportedHost], alias: &str) -> &'a ImportedHost {
        hosts
            .iter()
            .find(|h| h.alias == alias)
            .unwrap_or_else(|| panic!("no {alias}"))
    }

    #[test]
    fn lists_concrete_aliases_only() {
        let hosts = parse_ssh_config(SAMPLE);
        let names: Vec<_> = hosts.iter().map(|h| h.alias.as_str()).collect();
        assert_eq!(names, ["web", "prod-web", "db", "bastion"]);
    }

    #[test]
    fn resolves_options_first_value_wins() {
        let hosts = parse_ssh_config(SAMPLE);
        let web = find(&hosts, "web");
        assert_eq!(web.host_name, "web.example.com");
        assert_eq!(web.port, 2222);
        assert_eq!(web.user.as_deref(), Some("deploy")); // not "everyone"
        assert_eq!(web.identity_file.as_deref(), Some("~/.ssh/id_web"));

        // Defaults from `Host *` fill the gaps.
        let db = find(&hosts, "db");
        assert_eq!(db.host_name, "db.internal");
        assert_eq!(db.port, 22);
        assert_eq!(db.user.as_deref(), Some("everyone"));
        assert_eq!(db.identity_file.as_deref(), Some("~/.ssh/id_default"));
        assert_eq!(db.proxy_jump.as_deref(), Some("bastion"));

        assert_eq!(find(&hosts, "bastion").host_name, "bastion.example.com");
    }

    #[test]
    fn match_blocks_are_ignored() {
        let hosts = parse_ssh_config(SAMPLE);
        assert!(hosts.iter().all(|h| h.user.as_deref() != Some("ignored")));
    }

    #[test]
    fn negated_patterns_exclude() {
        let blocks = [Block {
            patterns: vec!["*.example.com".into(), "!skip.example.com".into()],
            opts: vec![],
        }];
        assert!(applies(&blocks[0].patterns, "a.example.com"));
        assert!(!applies(&blocks[0].patterns, "skip.example.com"));
        assert!(!applies(&blocks[0].patterns, "other.org"));
    }

    #[test]
    fn globbing() {
        assert!(glob("*", "anything"));
        assert!(glob("web-?", "web-1"));
        assert!(!glob("web-?", "web-12"));
        assert!(glob("*.EXAMPLE.com", "a.example.com"));
    }

    #[test]
    fn hostname_token_and_defaults() {
        let hosts = parse_ssh_config("Host box\n  HostName %h.lan\nHost bare\n");
        assert_eq!(find(&hosts, "box").host_name, "box.lan");
        let bare = find(&hosts, "bare");
        assert_eq!((bare.host_name.as_str(), bare.port), ("bare", 22));
    }

    #[test]
    fn converts_to_session_input() {
        let hosts = parse_ssh_config(SAMPLE);
        let web = find(&hosts, "web").to_input(Some(3));
        assert_eq!(web.auth, AuthKind::Key);
        assert_eq!(web.folder_id, Some(3));
        let db = find(&hosts, "db").to_input(None);
        assert_eq!(db.tags, ["proxyjump"]);
        let plain = parse_ssh_config("Host x\n");
        assert_eq!(plain[0].to_input(None).auth, AuthKind::Agent);
    }

    #[test]
    fn follows_includes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("conf.d")).unwrap();
        std::fs::write(
            dir.path().join("conf.d/a.conf"),
            "Host from-include\n  HostName inc.example\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("conf.d/skip.txt"), "Host not-included\n").unwrap();
        std::fs::write(
            dir.path().join("config"),
            "Include conf.d/*.conf\nHost main\n  HostName main.example\n",
        )
        .unwrap();
        let hosts = read_ssh_config(&dir.path().join("config")).unwrap();
        let names: Vec<_> = hosts.iter().map(|h| h.alias.as_str()).collect();
        assert_eq!(names, ["from-include", "main"]);
    }
}
