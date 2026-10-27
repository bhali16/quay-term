# Roadmap

## v0.1.0

Definition of done: a user can install on macOS, Linux or Windows, open a local shell tab, save and connect to SSH hosts securely, browse and transfer files via SFTP, and customize theme and font.

| Milestone | Scope |
|---|---|
| M0 Foundations | Repo, workspace layout, CI, Tauri skeleton, typed IPC |
| M1 Local terminal | Local PTY, xterm.js bridge, tabs, output batching |
| M2 SSH | Connect, auth (password, key, agent), known_hosts, keepalive |
| M3 Sessions and vault | Saved sessions, `~/.ssh/config` import, keychain storage, sidebar UI |
| M4 SFTP | List, upload, download, mkdir, rename, delete, transfer queue |
| M5 Polish and release | Themes, fonts, logging, installers, tag v0.1.0 |

## v0.2

Definition of done: a user can reach a host through a jump host, forward ports (local, remote, dynamic), split a window into panes, run one command on several sessions at once, keep snippets, and move their saved sessions between machines in an encrypted file.

Order matters: jump hosts and tunnels share one piece of plumbing (a way to open a channel on an existing connection), so they come first.

| Milestone | Scope |
|---|---|
| M6 Hardening | Fix the v0.1 gaps below; set a real CSP; frontend tests; keyboard-interactive prompts for 2FA/OTP (today every prompt gets the password) |
| M7 Connection manager | One owner thread per SSH connection, shared by shell, SFTP and forwards, so a host needs one login. Decided: move to `russh` (see below). Shell and SFTP are done; sharing one login between them comes next |
| M8 Jump hosts | `ProxyJump` chains per saved session and from `~/.ssh/config`; host-key prompts for every hop |
| M9 Tunnels | Local, remote and dynamic (SOCKS5) forwards, saved with the session, start/stop panel, bind-address and port-in-use errors |
| M10 Split panes | Horizontal/vertical splits inside a tab, focus and resize shortcuts, pane layout restored with the session |
| M11 Multi-exec and snippets | Send input to selected tabs or panes; snippet library with variables, folders and search |
| M12 Encrypted vault and release | Export/import of sessions and secrets as one file (Argon2id + AEAD, master password); fallback when no OS keychain exists (headless Linux); Intel macOS build; tag v0.2.0 |

### Known gaps in v0.1 (M6)

- The SFTP panel opens a second SSH login; a host with MFA asks twice.
- Frontend tests cover only shortcut handling; the keychain round-trip test is `#[ignore]`d and never runs in CI.
- No Intel macOS build; installers are unsigned and not notarized.
- No auto-update.

### Decisions needed before M7

1. ~~`ssh2` plus loopback bridge, or `russh`?~~ Decided: `russh`. A spike showed equal throughput on loopback, correct per-hop host keys, and several times better SFTP speed over a 40 ms link; `ssh2` needed a bridge socket and a polling thread per hop.
2. Does the encrypted vault replace the OS keychain or sit beside it? Recommended: beside it, keychain stays the default, the vault is for export and for systems without a keychain.
3. Code signing and notarization (Apple Developer account, Windows certificate) for v0.2 or later?

## Later

- v0.3+: serial, Telnet, X11 forwarding, RDP, VNC, remote editor, plugins

Track progress in [GitHub issues](https://github.com/bhali16/quay-term/issues).
