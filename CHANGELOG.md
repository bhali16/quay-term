# Changelog

All notable changes to this project are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.2] - 2026-10-12

### Added
- Bundled fonts: DM Sans for the interface and JetBrains Mono for the terminal, so the app looks the same on every OS. JetBrains Mono is the new default terminal font; settings still on the old default font are moved to it, custom fonts are left alone.
- A visible keyboard focus ring on buttons.

### Changed
- Window layout: the title-bar strip is gone. The sidebar runs the full height of the window and the tab strip starts after it. On macOS the traffic lights sit over the sidebar header with the collapse toggle right after them; the toggle does not move when the sidebar collapses to the rail.
- New tabs: a two-button pair in the sidebar header (local shell, SSH connection) replaces the tab strip's "+" menu. In the collapsed rail the pair is stacked vertically.
- Sidebar: the search box moved to the top and was restyled; a session row shows its first tag plus "+N" so a name is never pushed out by its tags; folder icons are neutral; the rail's initials keep a trailing number (`web-01` and `web-02` read W1 and W2).
- Terminal: line height 1.5 and more padding around the text.
- Themes: all five themes define status colours, a full 16-colour ANSI palette and readable text on the accent colour.
- Dialogs share one set of styles, including the SSH connection and host-key prompts; the host-key prompt always appears above other dialogs. The Settings theme swatches fit on one row.
- Files panel: path bar, file and folder icons, hover actions and a transfer list with progress; its width scales with the window (280-380px).

### Fixed
- The session editor's Save and Connect buttons sat below the visible area at the default window size; the action bar is now pinned to the bottom of every dialog.
- Light themes: the selected colour swatch ring was invisible, and fields used a heavy grey instead of the theme's input colour. Dracula showed white text on its light accent.

## [0.1.1] - 2026-10-11

### Fixed
- macOS: the app bundle is now ad-hoc signed as a whole, so Apple Silicon no longer reports "Quay Term is damaged and can't be opened". The app is still not notarized; see the README for the first-launch steps.

## [0.1.0] - 2026-10-11

### Added
- Initial repository setup: license, README, contribution and security docs, CI workflow.
- M0 foundations: Cargo workspace with `quay-*` crates, Tauri 2 + Svelte + xterm.js app skeleton, typed IPC via ts-rs.
- M1 local terminal: `quay-pty` PtySession (portable-pty) with batched output, Tauri session commands, tabbed xterm.js UI with open/close/rename and resize.
- M2 SSH: `quay-ssh` (ssh2) with password, key-file and ssh-agent auth, known_hosts verification with a trust prompt (changed keys are refused), `SshSession` implementing `Session`, SSH and TCP keepalive, user-readable connect errors, reconnect on Enter, SSH connect dialog; OpenSSH-in-Docker integration tests.
- M3 sessions and vault: `quay-sessions` (SQLite store with folders, tags, search, `~/.ssh/config` import incl. `Include`/wildcards), `quay-vault` (OS keychain via keyring, secrets never stored in the DB), saved-session connect that keeps secrets in Rust, sidebar UI with search, session editor and password prompt.
- Sidebar: drag and drop sessions and folders (into folders, nested, or back to the top level), nested folders, new session / new subfolder from a folder, folder tree in the editor; Escape closes dialogs even after clicking the backdrop.
- M4 SFTP: `quay-sftp` (own SSH connection per Files panel, listing, mkdir/rename/recursive delete, file and folder upload/download with cancel), a two-worker transfer queue with throttled progress, and a Files panel with picker/drag-and-drop upload; global autocapitalize/autocorrect/spellcheck off on text fields.
- M5 polish: settings file (`settings.json`, tolerant loading, atomic save), five built-in themes, terminal font family/size/scrollback/cursor settings, configurable keyboard shortcuts with live recording and conflict detection, font zoom shortcuts, file logging with rotation, a diagnostics panel, tag-triggered release workflow for macOS, Linux and Windows installers.
- UI polish: SVG icons for folders (open/closed, nested), sessions and actions; labelled Session/Folder/Import toolbar in the sidebar; larger labelled Settings button; app renamed "Quay Term" (window title, bundle name); new app icon.
