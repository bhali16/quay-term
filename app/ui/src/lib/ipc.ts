import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppInfo } from "./bindings/AppInfo";

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

export const sessionWrite = (id: number, data: Uint8Array) =>
  invoke<void>("session_write", { id, data: Array.from(data) });

export const sessionResize = (id: number, cols: number, rows: number) =>
  invoke<void>("session_resize", { id, cols, rows });

export const sessionClose = (id: number) => invoke<void>("session_close", { id });

export const onSessionExit = (cb: (id: number) => void) =>
  listen<number>("session-exit", (e) => cb(e.payload));
