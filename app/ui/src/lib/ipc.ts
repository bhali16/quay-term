import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppInfo } from "./bindings/AppInfo";
import type { HostKeyPrompt } from "./bindings/HostKeyPrompt";
import type { SessionExit } from "./bindings/SessionExit";
import type { SshTarget } from "./bindings/SshTarget";

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

export const sshTrustResponse = (promptId: number, trust: boolean) =>
  invoke<void>("ssh_trust_response", { promptId, trust });

export const onHostKeyPrompt = (cb: (p: HostKeyPrompt) => void) =>
  listen<HostKeyPrompt>("ssh-host-key-prompt", (e) => cb(e.payload));

export const sessionWrite = (id: number, data: Uint8Array) =>
  invoke<void>("session_write", { id, data: Array.from(data) });

export const sessionResize = (id: number, cols: number, rows: number) =>
  invoke<void>("session_resize", { id, cols, rows });

export const sessionClose = (id: number) => invoke<void>("session_close", { id });

export const onSessionExit = (cb: (exit: SessionExit) => void) =>
  listen<SessionExit>("session-exit", (e) => cb(e.payload));
