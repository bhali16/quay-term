# Changelog

All notable changes to this project are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Initial repository setup: license, README, contribution and security docs, CI workflow.
- M0 foundations: Cargo workspace with `quay-*` crates, Tauri 2 + Svelte + xterm.js app skeleton, typed IPC via ts-rs.
- M1 local terminal: `quay-pty` PtySession (portable-pty) with batched output, Tauri session commands, tabbed xterm.js UI with open/close/rename and resize.
