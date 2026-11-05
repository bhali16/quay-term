import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type Info = { version: string; defaultDir: string; existing: boolean; hasPayload: boolean };
export type Stage = "extract" | "install";

// Outside the Tauri shell (a plain browser during `npm run setup:dev`) there is no
// backend, so the preview gets a stand-in that behaves like a successful install.
const inShell = "__TAURI_INTERNALS__" in window;
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export const api = {
  info: (): Promise<Info> =>
    inShell ? invoke("info") : Promise.resolve({ version: "0.3.0", defaultDir: "C:\\Users\\you\\AppData\\Local\\Quay Term", existing: false, hasPayload: true }),
  isInstalled: (dir: string): Promise<boolean> => (inShell ? invoke("is_installed", { dir }) : Promise.resolve(false)),
  pickFolder: (current: string): Promise<string | null> => (inShell ? invoke("pick_folder", { current }) : Promise.resolve(null)),
  install: async (dir: string): Promise<string> => (inShell ? invoke("install", { dir }) : (await sleep(6000), dir + "\\Quay Term.exe")),
  launch: (exe: string): Promise<void> => (inShell ? invoke("launch", { exe }) : Promise.resolve()),
  onStage: (fn: (s: Stage) => void) => (inShell ? listen<Stage>("stage", (e) => fn(e.payload)) : Promise.resolve(() => {})),
  minimize: () => (inShell ? getCurrentWindow().minimize() : undefined),
  close: () => (inShell ? getCurrentWindow().close() : undefined),
};
