/** Rules and wording for desktop notifications. Pure, so they can be tested without the window. */
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";

export function formatDuration(ms: number): string {
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  return m < 60 ? `${m}m ${s % 60}s` : `${Math.floor(m / 60)}h ${m % 60}m`;
}

/**
 * Whether to alert. Never when the user is looking at that very tab (window focused and tab selected),
 * never for a muted pane, never when the feature is off.
 */
export function shouldNotify(o: { enabled: boolean; windowFocused: boolean; tabActive: boolean; muted: boolean }): boolean {
  return o.enabled && !o.muted && !(o.windowFocused && o.tabActive);
}

/** "<command, first 60 chars> finished (exit 0) in 2m 10s on <session>". Never contains output. */
export function commandBody(o: { command: string; exit: number | null; ms: number; session: string; showCommand: boolean }): string {
  const first = o.command.replace(/\s+/g, " ").trim();
  const what = o.showCommand && first ? (first.length > 60 ? `${first.slice(0, 60)}…` : first) : "Command";
  const verb = o.exit === null || o.exit === 0 ? "finished" : "failed";
  const code = o.exit === null ? "" : ` (exit ${o.exit})`;
  return `${what} ${verb}${code} in ${formatDuration(o.ms)} on ${o.session}`;
}

/** Whether a command ran long enough to be worth an alert. */
export const isLong = (ms: number, afterSecs: number) => ms >= afterSecs * 1000;

export type Permission = "granted" | "denied" | "unknown";

/** Ask for permission once; "denied" so the settings page can say so. Never throws. */
export async function ensurePermission(): Promise<Permission> {
  try {
    if (await isPermissionGranted()) return "granted";
    return (await requestPermission()) === "granted" ? "granted" : "denied";
  } catch {
    return "unknown";
  }
}
