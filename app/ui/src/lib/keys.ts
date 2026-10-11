/** Shortcut handling. A shortcut is "Mod+Shift+T"-style; matching is by physical-ish key name. */
export type Action =
  | "new_tab"
  | "close_tab"
  | "next_tab"
  | "prev_tab"
  | "toggle_sidebar"
  | "toggle_files"
  | "settings"
  | "zoom_in"
  | "zoom_out"
  | "zoom_reset";

export const actionLabels: Record<Action, string> = {
  new_tab: "New local tab",
  close_tab: "Close tab",
  next_tab: "Next tab",
  prev_tab: "Previous tab",
  toggle_sidebar: "Toggle sessions sidebar",
  toggle_files: "Toggle files panel",
  settings: "Open settings",
  zoom_in: "Increase font size",
  zoom_out: "Decrease font size",
  zoom_reset: "Reset font size",
};

export const isMac = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform);

// On macOS the app uses Cmd. Elsewhere Ctrl+Shift, so the shell still gets plain Ctrl+letter.
const M = isMac ? "Cmd" : "Ctrl+Shift";
export const defaultBindings: Record<Action, string> = {
  new_tab: `${M}+T`,
  close_tab: `${M}+W`,
  next_tab: "Ctrl+Tab",
  prev_tab: "Ctrl+Shift+Tab",
  toggle_sidebar: `${M}+B`,
  toggle_files: `${M}+E`,
  settings: isMac ? "Cmd+," : "Ctrl+Shift+,",
  zoom_in: isMac ? "Cmd+=" : "Ctrl+Shift+=",
  zoom_out: isMac ? "Cmd+-" : "Ctrl+Shift+-",
  zoom_reset: isMac ? "Cmd+0" : "Ctrl+Shift+0",
};

const MODS = ["Ctrl", "Alt", "Shift", "Cmd"] as const;

function keyName(e: KeyboardEvent): string | null {
  if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return null;
  if (e.key === " ") return "Space";
  // With Shift held, e.key for "=" is "+", for letters uppercase; use code for stable names.
  if (/^Key[A-Z]$/.test(e.code)) return e.code.slice(3);
  if (/^Digit\d$/.test(e.code)) return e.code.slice(5);
  const byCode: Record<string, string> = { Equal: "=", Minus: "-", Comma: ",", Period: ".", Slash: "/", Backslash: "\\", Semicolon: ";", Quote: "'", BracketLeft: "[", BracketRight: "]", Backquote: "`" };
  if (byCode[e.code]) return byCode[e.code];
  return e.key.length === 1 ? e.key.toUpperCase() : e.key;
}

/** Canonical string for a key event, or null for a bare modifier press. */
export function describe(e: KeyboardEvent): string | null {
  const k = keyName(e);
  if (!k) return null;
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Cmd"].filter(Boolean) as string[];
  return [...mods, k].join("+");
}

/** Normalize user-typed text ("cmd+shift+t") to the canonical form. */
export function normalize(s: string): string {
  const parts = s.split("+").map((p) => p.trim()).filter(Boolean);
  const key = parts.pop();
  if (!key) return "";
  const have = new Set(parts.map((p) => MODS.find((m) => m.toLowerCase() === p.toLowerCase()) ?? p));
  return [...MODS.filter((m) => have.has(m)), key.length === 1 ? key.toUpperCase() : key].join("+");
}

export function resolve(overrides: Record<string, string>): Record<Action, string> {
  const out = { ...defaultBindings };
  for (const a of Object.keys(out) as Action[]) if (overrides[a]) out[a] = normalize(overrides[a]);
  return out;
}

/** The action bound to this event, if any. */
export function actionFor(e: KeyboardEvent, bindings: Record<Action, string>): Action | null {
  const d = describe(e);
  if (!d) return null;
  return (Object.keys(bindings) as Action[]).find((a) => bindings[a] === d) ?? null;
}
