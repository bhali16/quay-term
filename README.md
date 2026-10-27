# quay-term

Cross-platform SSH client, terminal and SFTP manager for macOS, Linux and Windows. Built with Rust and Tauri.

> **Status:** early (0.1.x). Local terminals, SSH, saved sessions, SFTP, themes and shortcuts work. Installers are on the [releases page](https://github.com/bhali16/quay-term/releases); macOS builds are Apple silicon only.

## Installing

The installers are not code-signed or notarized yet, so your OS will warn on first launch.

- **macOS:** drag Quay Term to Applications, then right-click it, choose Open and confirm. If macOS says the app is damaged, run `xattr -dr com.apple.quarantine "/Applications/Quay Term.app"` and open it again.
- **Windows:** SmartScreen may show "Windows protected your PC"; choose More info, then Run anyway.
- **Linux:** make the `.AppImage` executable (`chmod +x`) or install the `.deb`/`.rpm`.

## Goals

One app to manage terminals, SSH sessions and remote files on any desktop OS.

What it does today (since v0.1.0):

- Local shell tabs
- SSH connections (password, key, agent) with known-hosts verification
- Saved sessions, folders, tags and `~/.ssh/config` import
- Secrets stored in the OS keychain
- Basic SFTP browser with a transfer queue
- Tabbed window with a collapsible session sidebar
- Five built-in themes, bundled fonts and font settings
- Installers for macOS, Linux and Windows

See [docs/ROADMAP.md](docs/ROADMAP.md) for the milestones.

## Tech stack

- [Tauri 2](https://tauri.app) (Rust backend, system WebView)
- [xterm.js](https://xtermjs.org) for terminal rendering
- [`russh`](https://crates.io/crates/russh) and [`russh-sftp`](https://crates.io/crates/russh-sftp) (pure Rust) for SSH and SFTP
- [`portable-pty`](https://crates.io/crates/portable-pty) for local shells
- [DM Sans](https://fonts.google.com/specimen/DM+Sans) and [JetBrains Mono](https://www.jetbrains.com/lp/mono/), bundled via [Fontsource](https://fontsource.org) (SIL Open Font License 1.1)

## Independence and clean-room policy

quay-term is an independent project. It is not affiliated with, endorsed by, or derived from MobaXterm or any other product. It is built from public documentation and open standards only. See [docs/CLEAN_ROOM.md](docs/CLEAN_ROOM.md).

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) and the [Code of Conduct](CODE_OF_CONDUCT.md) first. Report security issues as described in [SECURITY.md](SECURITY.md).

## License

GPL-3.0. See [LICENSE](LICENSE).
