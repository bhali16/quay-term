# quay-term

Cross-platform SSH client, terminal and SFTP manager for macOS, Linux and Windows. Built with Rust and Tauri.

> **Status:** early planning, pre-0.1.0. Nothing is usable yet.

## Goals

One app to manage terminals, SSH sessions and remote files on any desktop OS.

Planned for v0.1.0:

- Local shell tabs
- SSH connections (password, key, agent) with known-hosts verification
- Saved sessions, folders, tags and `~/.ssh/config` import
- Secrets stored in the OS keychain
- Basic SFTP browser with a transfer queue
- Themes and font settings
- Installers for macOS, Linux and Windows

See [docs/ROADMAP.md](docs/ROADMAP.md) for the milestones.

## Tech stack

- [Tauri 2](https://tauri.app) (Rust backend, system WebView)
- [xterm.js](https://xtermjs.org) for terminal rendering
- [`ssh2`](https://crates.io/crates/ssh2) (libssh2) for SSH and SFTP
- [`portable-pty`](https://crates.io/crates/portable-pty) for local shells

## Independence and clean-room policy

quay-term is an independent project. It is not affiliated with, endorsed by, or derived from MobaXterm or any other product. It is built from public documentation and open standards only. See [docs/CLEAN_ROOM.md](docs/CLEAN_ROOM.md).

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) and the [Code of Conduct](CODE_OF_CONDUCT.md) first. Report security issues as described in [SECURITY.md](SECURITY.md).

## License

GPL-3.0. See [LICENSE](LICENSE).
