import { invoke } from "@tauri-apps/api/core";
import type { AppInfo } from "./bindings/AppInfo";

/** Typed wrappers around Tauri commands. Types are generated from Rust by ts-rs. */
export const getAppInfo = () => invoke<AppInfo>("get_app_info");
