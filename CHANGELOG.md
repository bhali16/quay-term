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
