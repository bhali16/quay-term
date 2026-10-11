# Changelog

All notable changes to this project are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Initial repository setup: license, README, contribution and security docs, CI workflow.
- M0 foundations: Cargo workspace with `quay-*` crates, Tauri 2 + Svelte + xterm.js app skeleton, typed IPC via ts-rs.
- M1 local terminal: `quay-pty` PtySession (portable-pty) with batched output, Tauri session commands, tabbed xterm.js UI with open/close/rename and resize.
- M2 SSH: `quay-ssh` (ssh2) with password, key-file and ssh-agent auth, known_hosts verification with a trust prompt (changed keys are refused), `SshSession` implementing `Session`, SSH and TCP keepalive, user-readable connect errors, reconnect on Enter, SSH connect dialog; OpenSSH-in-Docker integration tests.
- M3 sessions and vault: `quay-sessions` (SQLite store with folders, tags, search, `~/.ssh/config` import incl. `Include`/wildcards), `quay-vault` (OS keychain via keyring, secrets never stored in the DB), saved-session connect that keeps secrets in Rust, sidebar UI with search, session editor and password prompt.
- Sidebar: drag and drop sessions and folders (into folders, nested, or back to the top level), nested folders, new session / new subfolder from a folder, folder tree in the editor; Escape closes dialogs even after clicking the backdrop.
- M4 SFTP: `quay-sftp` (own SSH connection per Files panel, listing, mkdir/rename/recursive delete, file and folder upload/download with cancel), a two-worker transfer queue with throttled progress, and a Files panel with picker/drag-and-drop upload; global autocapitalize/autocorrect/spellcheck off on text fields.
- M5 polish: settings file (`settings.json`, tolerant loading, atomic save), five built-in themes, terminal font family/size/scrollback/cursor settings, configurable keyboard shortcuts with live recording and conflict detection, font zoom shortcuts, file logging with rotation, a diagnostics panel, tag-triggered release workflow for macOS, Linux and Windows installers.
