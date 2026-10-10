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

## Later

- v0.2: tunnels (local, remote, dynamic), split panes, multi-exec, jump hosts, encrypted vault, snippets
- v0.3+: serial, Telnet, X11 forwarding, RDP, VNC, remote editor, plugins

Track progress in [GitHub issues](https://github.com/bhali16/quay-term/issues).
