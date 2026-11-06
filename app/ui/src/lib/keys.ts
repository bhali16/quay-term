/** Shortcut handling. A shortcut is "Mod+Shift+T"-style; matching is by physical-ish key name. */
export type Action =
  | "new_tab"
  | "close_tab"
  | "broadcast_pane"
  | "broadcast_tab"
  | "snippets"
  | "split_right"
  | "split_down"
  | "zoom_pane"
  | "mute_pane"
  | "focus_left"
  | "focus_right"
  | "focus_up"
  | "focus_down"
  | "resize_left"
  | "resize_right"
  | "resize_up"
  | "resize_down"
  | "next_tab"
  | "prev_tab"
  | "toggle_sidebar"
  | "toggle_files"
  | "settings"
  | "zoom_in"
  | "zoom_out"
  | "zoom_reset"
  | "palette"
  | "prev_prompt"
  | "next_prompt"
  | "copy_last_output"
  | "select_last_output"
  | "copy_last_command"
  | "rerun_last"
  | "select_block"
  | "enable_integration";

export const actionLabels: Record<Action, string> = {
  new_tab: "New local tab",
  close_tab: "Close pane or tab",
  broadcast_pane: "Send typing to this pane too (toggle)",
  broadcast_tab: "Send typing to all panes of this tab (toggle)",
  snippets: "Open snippets",
  split_right: "Split pane right",
  split_down: "Split pane down",
  zoom_pane: "Zoom pane to fill the tab (toggle)",
  mute_pane: "Mute alerts from this pane (toggle)",
  focus_left: "Focus pane to the left",
  focus_right: "Focus pane to the right",
  focus_up: "Focus pane above",
  focus_down: "Focus pane below",
  resize_left: "Move divider left",
  resize_right: "Move divider right",
  resize_up: "Move divider up",
  resize_down: "Move divider down",
  next_tab: "Next tab",
  prev_tab: "Previous tab",
  toggle_sidebar: "Toggle sessions sidebar",
  toggle_files: "Toggle files panel",
  settings: "Open settings",
  zoom_in: "Increase font size",
  zoom_out: "Decrease font size",
  zoom_reset: "Reset font size",
  palette: "Command palette",
  prev_prompt: "Jump to previous prompt",
  next_prompt: "Jump to next prompt",
  copy_last_output: "Copy output of last command",
  select_last_output: "Select output of last command",
  copy_last_command: "Copy last command",
  rerun_last: "Type last command again (no Enter)",
  select_block: "Select output of the command in view",
  enable_integration: "Enable command marks in this remote shell",
};

export const isMac = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform);

// On macOS the app uses Cmd. Elsewhere Ctrl+Shift, so the shell still gets plain Ctrl+letter.
const M = isMac ? "Cmd" : "Ctrl+Shift";
export const defaultBindings: Record<Action, string> = {
  new_tab: `${M}+T`,
  close_tab: `${M}+W`,
  broadcast_pane: isMac ? "Alt+Cmd+B" : "Ctrl+Alt+B",
  broadcast_tab: isMac ? "Alt+Cmd+I" : "Ctrl+Alt+I",
  snippets: isMac ? "Shift+Cmd+S" : "Ctrl+Alt+S",
  split_right: `${M}+D`,
  split_down: isMac ? "Shift+Cmd+D" : "Ctrl+Alt+D",
  zoom_pane: isMac ? "Shift+Cmd+Enter" : "Ctrl+Shift+Enter",
  mute_pane: isMac ? "Alt+Shift+Cmd+M" : "Ctrl+Alt+Shift+M",
  focus_left: isMac ? "Alt+Cmd+ArrowLeft" : "Ctrl+Alt+ArrowLeft",
  focus_right: isMac ? "Alt+Cmd+ArrowRight" : "Ctrl+Alt+ArrowRight",
  focus_up: isMac ? "Alt+Cmd+ArrowUp" : "Ctrl+Alt+ArrowUp",
  focus_down: isMac ? "Alt+Cmd+ArrowDown" : "Ctrl+Alt+ArrowDown",
  resize_left: isMac ? "Ctrl+Cmd+ArrowLeft" : "Ctrl+Alt+Shift+ArrowLeft",
  resize_right: isMac ? "Ctrl+Cmd+ArrowRight" : "Ctrl+Alt+Shift+ArrowRight",
  resize_up: isMac ? "Ctrl+Cmd+ArrowUp" : "Ctrl+Alt+Shift+ArrowUp",
  resize_down: isMac ? "Ctrl+Cmd+ArrowDown" : "Ctrl+Alt+Shift+ArrowDown",
  next_tab: "Ctrl+Tab",
  prev_tab: "Ctrl+Shift+Tab",
  toggle_sidebar: `${M}+B`,
  toggle_files: `${M}+E`,
  settings: isMac ? "Cmd+," : "Ctrl+Shift+,",
  zoom_in: isMac ? "Cmd+=" : "Ctrl+Shift+=",
  zoom_out: isMac ? "Cmd+-" : "Ctrl+Shift+-",
  zoom_reset: isMac ? "Cmd+0" : "Ctrl+Shift+0",
  palette: isMac ? "Cmd+K" : "Ctrl+Shift+P",
  prev_prompt: isMac ? "Cmd+ArrowUp" : "Ctrl+Shift+ArrowUp",
  next_prompt: isMac ? "Cmd+ArrowDown" : "Ctrl+Shift+ArrowDown",
  copy_last_output: isMac ? "Shift+Cmd+O" : "Ctrl+Alt+O",
  select_last_output: isMac ? "Alt+Cmd+O" : "Ctrl+Alt+Shift+O",
  copy_last_command: isMac ? "Alt+Shift+Cmd+C" : "Ctrl+Alt+Shift+C",
  rerun_last: isMac ? "Shift+Cmd+R" : "Ctrl+Alt+R",
  select_block: isMac ? "Alt+Cmd+J" : "Ctrl+Alt+J",
  enable_integration: isMac ? "Alt+Shift+Cmd+I" : "Ctrl+Alt+Shift+I",
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
