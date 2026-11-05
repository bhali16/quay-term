# Changelog

All notable changes to this project are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed
- Windows now ships a single branded setup `.exe` (NSIS) in the app's colors and icon, installing per user with no admin prompt. The separate `.msi` is no longer built.

## [0.3.0] - 2026-11-05

### Changed
- Copying a selection (copy on select, Ctrl/Cmd+C) now shows a short "Copied" label in the top right of the pane, with the line count for several lines.

### Added
- Desktop notifications: when quay-term is in the background, you are told when a command that ran longer than a threshold finishes (15 seconds by default) and when a session disconnects. Turn it off or change the threshold under Settings > Terminal. Long-command alerts need command marks.
- Command palette (Cmd+K, or Ctrl+Shift+P on Windows and Linux): one search box for actions, saved sessions, snippets and open tabs. Type `>`, `@`, `!` or `/` first to search only actions, sessions, snippets or tabs.
- Saved sessions can carry an environment label (prod, staging, dev, or your own, up to 8 characters). The label shows as a badge on the tab and the pane, the session colour underlines the tab and the top of the pane, and pasting several lines into a `prod` session asks first.
- Tabs show connection state: a pulsing dot while connecting, green when connected, a hollow red dot when disconnected, and a blue dot when a background tab has new output.
- When a session ends, a bar offers Reconnect (or press Enter or R), Files and Dismiss. A local shell that exits can be restarted the same way. Turn the bar off under Settings > Terminal.
- Command marks: local zsh and bash shells now mark prompts and commands (OSC 133), without changing your dotfiles. A green or red bar shows beside each finished command, and new actions jump to the previous or next prompt, copy or select the last command's output, copy the last command, and type it again without running it. For a remote bash or zsh, run "Enable command marks in this remote shell" from the palette; it types a one-line hook that defines functions for that session only and writes no files. Each finished command also gets a small copy button on its first row: click it to copy the command and its output, or Alt-click to copy the output only. Turn the feature off under Settings > Terminal. bash older than 4.4 (the macOS system bash) gets no command marks, and fish 4 and newer emits them itself.

### Fixed
- A stray % no longer appears when a session opens: the current terminal size is sent once the session is ready, so a resize during startup is not lost.

## [0.2.3] - 2026-11-04

### Added
- Copy on select: text you select in a terminal is copied as soon as you release the mouse. Turn it off under Settings > Terminal.
- Ctrl+C (Cmd+C on macOS) copies the selected text; with nothing selected it still interrupts the running command. Ctrl+Shift+C copies and Ctrl+Shift+V pastes. On Windows and Linux, right-click copies a selection or pastes.
- Ctrl+click (Cmd+click on macOS) opens http and https links in your browser.

### Changed
- Tighter line spacing, a clearer selection highlight, and a minimum contrast so dim colours stay readable.

## [0.2.2] - 2026-11-04

### Changed
- No feature changes. This release exists to confirm that the in-app updater takes an installed 0.2.1 to a newer version.

## [0.2.1] - 2026-11-04

### Added
- Updates from inside the app. About in Settings shows the version, checks for a newer release and installs it with one click (Install and restart). Updates are signed with a project key, and the app refuses anything that does not match it. By default it also checks a few seconds after launch and shows a message if there is a newer version; turn that off under Settings > Terminal > Check for updates when quay-term starts. Nothing is downloaded until you click. Installs from before this version need one manual update first.

### Added
- Importing `~/.ssh/config` now brings in `LocalForward`, `RemoteForward` and `DynamicForward` lines as saved tunnels, including those from `Host *` blocks and `Include` files. They are saved with Auto start off, so nothing opens a port until you start it. Forms that cannot be represented (Unix sockets, a remote forward with no destination) are skipped.

### Changed
- A jump host with a password but none saved no longer stops the connection. You are asked for its password before the tab opens, with the same "save in the keychain" choice, one prompt per such hop. The Files panel reuses it if it has to sign in again.

## [0.2.0] - 2026-11-04

### Added
- Encrypted export and import (Settings > Backup). Sessions, folders, jump hosts, tunnels and snippets, and optionally saved passwords and key passphrases, go into one file sealed with a master password (Argon2id key derivation, ChaCha20-Poly1305 encryption). Importing only adds what is missing: a session with the same name, host, port and user is kept as it is, and links between sessions and folders are rebuilt.
- A secret vault for computers with no system keychain (a headless Linux box). Saved passwords go into a file locked with a master password, asked for once per launch. The system keychain is still used wherever it exists.
- An Intel macOS installer, next to the Apple silicon one.
- Multi-exec. Turn on broadcast for the focused pane (Alt+Cmd+B; Ctrl+Alt+B elsewhere) or for every pane of a tab (Alt+Cmd+I; Ctrl+Alt+I), in any tabs, and what you type or paste into one selected pane is typed into the others too. Selected panes show a Broadcast badge, and a button in the tab strip shows the count and turns it all off.
- Snippets. A library of named commands with folders (`ops/web`) and search over name, folder and text, kept in `snippets.json` in the app data folder. `{{name}}` and `{{name:default}}` are asked for when sending. A snippet goes to the focused pane, every pane of the tab, or the broadcast panes, with or without a final Enter. Open it from the tab strip or with Shift+Cmd+S (Ctrl+Alt+S elsewhere).
- Split panes. A tab can be split side by side (Cmd+D) or stacked (Shift+Cmd+D; Ctrl+Shift+D and Ctrl+Alt+D elsewhere), repeatedly. Drag a divider or use the shortcuts to move focus (Alt+Cmd+arrows) and resize (Ctrl+Cmd+arrows); all are rebindable in Settings. Each pane is its own shell, and for an SSH tab its own login. Cmd+W closes the focused pane, and the tab when it is the last one. A saved session reopens with the layout it was closed with (split shape and sizes; every pane connects again). Files and Tunnels follow the focused pane.
- Tunnels (port forwards) over a terminal tab's SSH connection, from a new Tunnels panel next to Files: local (a port here leads to a host the server can reach), remote (a port on the server leads back to a host you can reach) and dynamic (a SOCKS5 proxy). Each shows open connections and bytes moved, and a busy port or a server that refuses remote forwarding is explained. They listen on loopback unless you allow other devices, which the panel flags. Tunnels can be saved with a session in the session editor, and the ones marked Auto start when it connects. A tunnel stops when its tab's shell ends.
- Jump hosts. A saved session can connect through another saved session ("Connect via" in the session editor), and chains work: each hop signs in with its own saved credentials, and each host key is checked and remembered under its own name. The host-key and sign-in prompts say which hop is asking, and an error names the jump host that failed. Importing `~/.ssh/config` links `ProxyJump` (aliases, `user@host:port`, and comma-separated chains) to the imported sessions; a jump it cannot resolve keeps the `proxyjump` tag. Loops and chains longer than 8 are refused.
- Sign-in prompts for servers that ask for more than a password (one-time codes, a second factor). The password is still sent automatically to a lone password prompt; everything else is shown in a dialog. This works for the terminal and the Files panel.
- Frontend unit tests (vitest) for keyboard shortcut handling, run in CI.

### Changed
- The Files panel reuses its terminal tab's SSH login instead of signing in again, so a host with a password prompt, a one-time code or a host-key question asks once. If the tab's connection is gone, the panel signs in on its own as before. The connection stays open until both the shell and the panel are done with it.
- SSH and SFTP now run on russh and russh-sftp (pure Rust) instead of libssh2; there is no OpenSSL or C SSH library in the build any more.
- Terminal SSH sessions use the new connection layer. Host keys are checked per connection, so jump hosts and tunnels can follow. A host recorded in known_hosts with a key of another type is still refused, and the recorded key type is now requested first, as OpenSSH does.
- The webview now has a Content Security Policy (scripts, styles, fonts and images from the app only).
- Logs keep the five most recent rotated files instead of one.

### Fixed
- SFTP transfers use 256 KB chunks instead of 32 KB, which is several times faster on high-latency connections.
- A saved session with an auth method this version does not know is reported as an error instead of silently turning into ssh-agent auth.

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
