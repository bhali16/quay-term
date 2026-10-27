import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Settings } from "./bindings/Settings";
import type { Diagnostics } from "./bindings/Diagnostics";
import type { AppInfo } from "./bindings/AppInfo";
import type { AuthPrompt } from "./bindings/AuthPrompt";
import type { HostKeyPrompt } from "./bindings/HostKeyPrompt";
import type { SessionExit } from "./bindings/SessionExit";
import type { SshTarget } from "./bindings/SshTarget";
import type { Folder } from "./bindings/Folder";
import type { ImportResult } from "./bindings/ImportResult";
import type { LibraryView } from "./bindings/LibraryView";
import type { SavedSession } from "./bindings/SavedSession";
import type { SecretUpdate } from "./bindings/SecretUpdate";
import type { SessionInput } from "./bindings/SessionInput";
import type { Entry } from "./bindings/Entry";
import type { SftpOpened } from "./bindings/SftpOpened";
import type { SftpTransfer } from "./bindings/SftpTransfer";

/** Typed wrappers around Tauri commands. Types are generated from Rust by ts-rs. */
export const getAppInfo = () => invoke<AppInfo>("get_app_info");

/** Start a local shell; `onOutput` receives raw bytes from the PTY. */
export function openLocalSession(
  cols: number,
  rows: number,
  onOutput: (data: Uint8Array) => void,
): Promise<number> {
  const channel = new Channel<ArrayBuffer>();
  channel.onmessage = (buf) => onOutput(new Uint8Array(buf));
  return invoke<number>("open_local_session", { cols, rows, onOutput: channel });
}

/** Connect over SSH; rejects with a user-readable message. */
export function openSshSession(
  target: SshTarget,
  cols: number,
  rows: number,
  onOutput: (data: Uint8Array) => void,
): Promise<number> {
  const channel = new Channel<ArrayBuffer>();
  channel.onmessage = (buf) => onOutput(new Uint8Array(buf));
  return invoke<number>("open_ssh_session", { target, cols, rows, onOutput: channel });
}

/**
 * Connect to a saved session. The stored secret is read from the keychain in
 * Rust; `secret` is only for one the user just typed.
 */
export function openSavedSession(
  id: number,
  secret: string | null,
  remember: boolean,
  cols: number,
  rows: number,
  onOutput: (data: Uint8Array) => void,
): Promise<number> {
  const channel = new Channel<ArrayBuffer>();
  channel.onmessage = (buf) => onOutput(new Uint8Array(buf));
  return invoke<number>("open_saved_session", { id, secret, remember, cols, rows, onOutput: channel });
}

export const libraryLoad = (query?: string) => invoke<LibraryView>("library_load", { query: query ?? null });
export const sessionSave = (id: number | null, input: SessionInput, secret: SecretUpdate) =>
  invoke<SavedSession>("session_save", { id, input, secret });
export const sessionDelete = (id: number) => invoke<void>("session_delete", { id });
export const folderCreate = (name: string, parentId: number | null = null) =>
  invoke<Folder>("folder_create", { name, parentId });
export const folderRename = (id: number, name: string) => invoke<void>("folder_rename", { id, name });
export const sessionMove = (id: number, folderId: number | null) =>
  invoke<void>("session_move", { id, folderId });
export const folderMove = (id: number, parentId: number | null) =>
  invoke<void>("folder_move", { id, parentId });
export const folderDelete = (id: number) => invoke<void>("folder_delete", { id });
export const importSshConfig = () => invoke<ImportResult>("import_ssh_config");

export const sshTrustResponse = (promptId: number, trust: boolean) =>
  invoke<void>("ssh_trust_response", { promptId, trust });

export const onHostKeyPrompt = (cb: (p: HostKeyPrompt) => void) =>
  listen<HostKeyPrompt>("ssh-host-key-prompt", (e) => cb(e.payload));

export const sshAuthResponse = (promptId: number, answers: string[] | null) =>
  invoke<void>("ssh_auth_response", { promptId, answers });

export const onAuthPrompt = (cb: (p: AuthPrompt) => void) =>
  listen<AuthPrompt>("ssh-auth-prompt", (e) => cb(e.payload));

export const sessionWrite = (id: number, data: Uint8Array) =>
  invoke<void>("session_write", { id, data: Array.from(data) });

export const sessionResize = (id: number, cols: number, rows: number) =>
  invoke<void>("session_resize", { id, cols, rows });

export const sessionClose = (id: number) => invoke<void>("session_close", { id });

export const onSessionExit = (cb: (exit: SessionExit) => void) =>
  listen<SessionExit>("session-exit", (e) => cb(e.payload));

// ---- SFTP file panel ----
export const sftpOpen = (target: SshTarget) => invoke<SftpOpened>("sftp_open", { target });
export const sftpOpenSaved = (id: number, secret: string | null) =>
  invoke<SftpOpened>("sftp_open_saved", { id, secret });
/** Open SFTP on the connection behind a terminal tab, so the host needs no second login. */
export const sftpOpenShared = (session: number) => invoke<SftpOpened>("sftp_open_shared", { session });
export const sftpList = (id: number, path: string) => invoke<Entry[]>("sftp_list", { id, path });
export const sftpCanonical = (id: number, path: string) => invoke<string>("sftp_canonical", { id, path });
export const sftpMkdir = (id: number, path: string) => invoke<void>("sftp_mkdir", { id, path });
export const sftpRename = (id: number, from: string, to: string) => invoke<void>("sftp_rename", { id, from, to });
export const sftpDelete = (id: number, path: string) => invoke<void>("sftp_delete", { id, path });
export const sftpUpload = (id: number, locals: string[], dir: string) =>
  invoke<number[]>("sftp_upload", { id, locals, dir });
/** Resolves to the local path the file is being saved to. */
export const sftpDownload = (id: number, remote: string) => invoke<string>("sftp_download", { id, remote });
export const sftpCancel = (id: number, transfer: number) => invoke<void>("sftp_cancel", { id, transfer });
export const sftpClose = (id: number) => invoke<void>("sftp_close", { id });
export const onSftpTransfer = (cb: (t: SftpTransfer) => void) =>
  listen<SftpTransfer>("sftp-transfer", (e) => cb(e.payload));

// ---- settings and diagnostics ----
export const settingsLoad = () => invoke<Settings>("settings_load");
export const settingsSave = (settings: Settings) => invoke<Settings>("settings_save", { settings });
export const getDiagnostics = () => invoke<Diagnostics>("get_diagnostics");
