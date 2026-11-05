# quay-term

Cross-platform SSH client, terminal and SFTP manager for macOS, Linux and Windows. Built with Rust and Tauri.

> **Status:** early (0.2.x). Local terminals, SSH with jump hosts and port forwards, saved sessions, split panes, snippets, SFTP, encrypted export and import, signed in-app updates, themes and shortcuts work. Installers are on the [releases page](https://github.com/bhali16/quay-term/releases); macOS builds are available for Apple silicon and Intel.

## Installing

The installers are not code-signed or notarized yet, so your OS will warn on first launch.

- **macOS:** drag Quay Term to Applications, then right-click it, choose Open and confirm. If macOS says the app is damaged, run `xattr -dr com.apple.quarantine "/Applications/Quay Term.app"` and open it again.
- **Windows:** download `Quay-Term-Setup.exe`. SmartScreen may show "Windows protected your PC"; choose More info, then Run anyway.
- **Linux:** make the `.AppImage` executable (`chmod +x`) or install the `.deb`/`.rpm`.

### Updating

From 0.2.1 on, quay-term can update itself. Open Settings > About and choose Check for updates; if there is a newer version, choose Install and restart. By default it also checks a few seconds after launch and tells you if something newer exists, and nothing is downloaded until you click. Updates are signed, and the app refuses any that do not match the project's key. To stop the launch check, untick Check for updates when quay-term starts in Settings > Terminal. Versions older than 0.2.1 need one manual install first.

## Goals

One app to manage terminals, SSH sessions and remote files on any desktop OS.

What it does today (v0.2):

- **Terminals:** local shell tabs, split panes inside a tab (the layout comes back with a saved session), and typing broadcast to selected panes
- **SSH:** password, key, agent and keyboard-interactive (one-time codes, second factors) sign-in, with known-hosts verification
- **Jump hosts:** chain `ProxyJump` hosts per saved session or from `~/.ssh/config`, with a host-key check at every hop
- **Port forwards:** local, remote and dynamic (SOCKS5) tunnels over a tab's connection, saved with the session and optionally started automatically
- **Sessions:** saved sessions with folders, tags and search, and `~/.ssh/config` import (hosts, `ProxyJump` and forwards)
- **Files:** SFTP browser with a transfer queue, sharing the terminal tab's login so a host asks for a code once
- **Snippets:** a library of reusable commands with `{{variables}}`, folders and search
- **Secrets:** stored in the OS keychain, or in a master-password file on machines without one
- **Backup:** encrypted export and import of sessions, secrets and snippets in one file (Argon2id and ChaCha20-Poly1305)
- **Updates:** signed in-app updates (Settings > About)
- **Command palette:** Cmd/Ctrl+K searches actions, sessions, snippets and tabs
- **Environments:** colour and prod/staging badges on saved sessions, tab connection state, and a reconnect bar
- **Command marks:** prompt jumping, copy last output and exit-code bars for local zsh/bash (and remote ones on request)
- **Clipboard:** copy on select, Ctrl+C copies a selection, right-click pastes (Windows/Linux), Ctrl+click opens links
- Five built-in themes, bundled fonts, configurable shortcuts
- Installers for macOS (Apple silicon and Intel), Linux and Windows

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
