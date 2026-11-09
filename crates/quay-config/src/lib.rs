//! User settings for quay-term, stored as a small JSON file.
//!
//! Loading never fails the app: a missing, unreadable or malformed file yields
//! the defaults, and unknown fields are ignored so older and newer versions can
//! share a file. Keybindings are stored as overrides only; the frontend merges
//! them over its built-in defaults, so new default shortcuts reach existing users.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const MIN_FONT_SIZE: u32 = 8;
pub const MAX_FONT_SIZE: u32 = 36;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not write settings: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not encode settings: {0}")]
    Json(#[from] serde_json::Error),
}

/// Where the tab list sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../app/ui/src/lib/bindings/")]
pub enum TabLayout {
    /// A rail down the left side, for many sessions.
    Vertical,
    /// A strip above the terminal. Also what any unknown value in a hand-edited or newer file reads
    /// as (`other` must be the last variant).
    #[default]
    #[serde(other)]
    Horizontal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../app/ui/src/lib/bindings/")]
pub struct Settings {
    /// Theme id; unknown ids fall back to the default theme in the UI.
    pub theme: String,
    /// CSS font-family list for the terminal.
    pub font_family: String,
    pub font_size: u32,
    pub cursor_blink: bool,
    pub scrollback: u32,
    /// Copy selected text to the clipboard as soon as the mouse is released.
    pub copy_on_select: bool,
    /// Look for a newer release shortly after launch. Nothing is installed without a click.
    pub check_for_updates: bool,
    /// Show a hint bar with shortcuts when a session has ended.
    pub show_hints: bool,
    /// Mark prompts and commands in local shells (OSC 133) so the terminal can jump between
    /// them and copy their output. Remote shells are never touched without a request.
    pub shell_integration: bool,
    /// Draw a green or red bar beside each finished command.
    pub block_gutter: bool,
    /// Send a desktop notification when a long command finishes or a session drops while the
    /// window or the tab is in the background. Off until the user opts in.
    pub notifications: bool,
    /// A command must run at least this many seconds to count as long.
    pub notify_after_secs: u32,
    /// Put the first words of the command in the notification. Off shows only "Command finished".
    pub notify_show_command: bool,
    /// Action id -> shortcut such as `Cmd+T`. Only user overrides are stored.
    pub keybindings: BTreeMap<String, String>,
    /// Focus the pane under the mouse pointer without clicking it.
    pub focus_follows_mouse: bool,
    pub tab_layout: TabLayout,
    /// Experimental: typing `;name` and Tab at a prompt opens the snippet with that short name.
    /// Needs shell integration marks, so it does nothing in a shell without them.
    pub snippet_expand: bool,
    /// Save finished commands (from shell integration marks) so they can be searched across
    /// sessions. Off until the user opts in; nothing is written while it is off.
    pub record_history: bool,
    /// Regular expressions (case-insensitive); a command matching any of them is never saved.
    pub history_deny_patterns: Vec<String>,
}

/// Commands that look like they carry a secret. Users can edit the list in Settings.
pub fn default_history_deny_patterns() -> Vec<String> {
    [
        r"pass(word|wd)?\s*[=:]",
        r"--?(api[-_]?)?(token|secret|password)\b",
        r"authorization:",
        r"aws_secret",
        r"\bexport\s+\w*(key|secret|token|passw)\w*\s*=",
        r"\b(secret|token|api[-_]?key)\s*=",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// Most deny patterns kept, and the longest one, so a hand-edited file cannot stall the UI.
pub const MAX_DENY_PATTERNS: usize = 100;
pub const MAX_DENY_PATTERN_LEN: usize = 500;

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "quay-dark".into(),
            font_family:
                "'JetBrains Mono', Menlo, Consolas, 'DejaVu Sans Mono', ui-monospace, monospace"
                    .into(),
            font_size: 14,
            cursor_blink: true,
            scrollback: 5000,
            copy_on_select: true,
            check_for_updates: true,
            show_hints: true,
            shell_integration: true,
            block_gutter: true,
            notifications: false,
            notify_after_secs: 15,
            notify_show_command: true,
            keybindings: BTreeMap::new(),
            focus_follows_mouse: false,
            tab_layout: TabLayout::Horizontal,
            snippet_expand: false,
            record_history: false,
            history_deny_patterns: default_history_deny_patterns(),
        }
    }
}

impl Settings {
    /// Clamp values into the range the UI can render sensibly.
    pub fn sanitized(mut self) -> Self {
        self.font_size = self.font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        self.scrollback = self.scrollback.min(100_000);
        if self.font_family.trim().is_empty() {
            self.font_family = Settings::default().font_family;
        }
        self.keybindings.retain(|_, v| !v.trim().is_empty());
        self.history_deny_patterns = self
            .history_deny_patterns
            .iter()
            .map(|p| p.trim())
            .filter(|p| !p.is_empty() && p.len() <= MAX_DENY_PATTERN_LEN)
            .take(MAX_DENY_PATTERNS)
            .map(String::from)
            .collect();
        self
    }

    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
            .unwrap_or_default()
            .sanitized()
    }

    /// Write atomically (temp file + rename) so a crash cannot truncate the file.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            Settings::load(&dir.path().join("nope.json")),
            Settings::default()
        );
    }

    #[test]
    fn malformed_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.json");
        std::fs::write(&p, "{ not json").unwrap();
        assert_eq!(Settings::load(&p), Settings::default());
    }

    #[test]
    fn partial_and_unknown_fields_are_tolerated() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.json");
        std::fs::write(&p, r#"{"font_size": 18, "from_the_future": true}"#).unwrap();
        let s = Settings::load(&p);
        assert_eq!(s.font_size, 18);
        assert_eq!(s.theme, Settings::default().theme);
    }

    #[test]
    fn update_checks_are_on_for_settings_saved_before_they_existed() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.json");
        std::fs::write(&p, r#"{"font_size": 18}"#).unwrap();
        assert!(Settings::load(&p).check_for_updates);
        std::fs::write(&p, r#"{"check_for_updates": false}"#).unwrap();
        assert!(!Settings::load(&p).check_for_updates);
    }

    #[test]
    fn focus_follows_mouse_defaults_off_for_old_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.json");
        std::fs::write(&p, r#"{"font_size": 18}"#).unwrap();
        assert!(!Settings::load(&p).focus_follows_mouse);
        std::fs::write(&p, r#"{"focus_follows_mouse": true}"#).unwrap();
        assert!(Settings::load(&p).focus_follows_mouse);
    }

    #[test]
    fn tab_layout_defaults_to_horizontal_and_tolerates_unknown_values() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.json");
        std::fs::write(&p, r#"{"font_size": 18}"#).unwrap();
        assert_eq!(Settings::load(&p).tab_layout, TabLayout::Horizontal);
        std::fs::write(&p, r#"{"tab_layout": "vertical", "font_size": 18}"#).unwrap();
        assert_eq!(Settings::load(&p).tab_layout, TabLayout::Vertical);
        std::fs::write(&p, r#"{"tab_layout": "sideways", "font_size": 18}"#).unwrap();
        let s = Settings::load(&p);
        assert_eq!(s.tab_layout, TabLayout::Horizontal);
        assert_eq!(s.font_size, 18);
    }

    #[test]
    fn notifications_are_opt_in_and_show_the_command_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.json");
        std::fs::write(&p, r#"{"font_size": 18}"#).unwrap();
        let s = Settings::load(&p);
        assert!(!s.notifications);
        assert!(s.notify_show_command);
        std::fs::write(
            &p,
            r#"{"notifications": true, "notify_show_command": false}"#,
        )
        .unwrap();
        let s = Settings::load(&p);
        assert!(s.notifications && !s.notify_show_command);
    }

    #[test]
    fn history_is_off_with_default_deny_patterns_for_old_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.json");
        std::fs::write(&p, r#"{"font_size": 18}"#).unwrap();
        let s = Settings::load(&p);
        assert!(!s.record_history);
        assert_eq!(s.history_deny_patterns, default_history_deny_patterns());
        std::fs::write(
            &p,
            r#"{"record_history": true, "history_deny_patterns": [" x= ", "", "   "]}"#,
        )
        .unwrap();
        let s = Settings::load(&p);
        assert!(s.record_history);
        assert_eq!(s.history_deny_patterns, vec!["x=".to_string()]);
    }

    #[test]
    fn save_round_trips_and_clamps() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sub").join("s.json");
        let mut s = Settings {
            theme: "dracula".into(),
            font_size: 200,
            ..Settings::default()
        };
        s.keybindings.insert("new_tab".into(), "Cmd+N".into());
        s.keybindings.insert("close_tab".into(), "  ".into());
        s.save(&p).unwrap();
        let back = Settings::load(&p);
        assert_eq!(back.theme, "dracula");
        assert_eq!(back.font_size, MAX_FONT_SIZE);
        assert_eq!(back.keybindings.len(), 1);
        assert!(!p.with_extension("json.tmp").exists());
    }
}
