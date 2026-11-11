import { settingsLoad, settingsSave } from "./ipc";
import type { Settings } from "./bindings/Settings";
import { applyChrome, themeById } from "./themes";
import { resolve } from "./keys";
import { DEFAULT_DENY } from "./history";

const OLD_FONT = "Menlo, Consolas, 'DejaVu Sans Mono', monospace";
const FONT = "'JetBrains Mono', Menlo, Consolas, 'DejaVu Sans Mono', ui-monospace, monospace";

const fallback: Settings = {
  theme: "quay-dark",
  theme_match_system: false,
  theme_light: "quay-light",
  theme_dark: "quay-dark",
  font_family: FONT,
  font_size: 14,
  cursor_blink: true,
  scrollback: 5000,
  copy_on_select: true,
  check_for_updates: true,
  show_hints: true,
  shell_integration: true,
  block_gutter: true,
  notifications: false,
  notify_after_secs: 15,
  notify_show_command: true,
  keybindings: {},
  focus_follows_mouse: false,
  confirm_close: true,
  tab_layout: "horizontal",
  dim_unfocused_panes: true,
  snippet_expand: false,
  screen_reader: false,
  record_history: false,
  history_deny_patterns: DEFAULT_DENY,
};

/** The theme id in use: the chosen one, or the light or dark choice when following the system. */
export function activeThemeId(v: Pick<Settings, "theme" | "theme_match_system" | "theme_light" | "theme_dark">, systemDark: boolean): string {
  return v.theme_match_system ? (systemDark ? v.theme_dark : v.theme_light) : v.theme;
}

const darkQuery = typeof matchMedia === "function" ? matchMedia("(prefers-color-scheme: dark)") : null;

class Store {
  value = $state<Settings>(fallback);
  /** The system is in dark mode right now. */
  systemDark = $state(darkQuery?.matches ?? true);
  theme = $derived(themeById(activeThemeId(this.value, this.systemDark)));

  constructor() {
    // The system switched between light and dark: follow it at once if the user asked for that.
    darkQuery?.addEventListener("change", (e) => {
      this.systemDark = e.matches;
      applyChrome(this.theme);
    });
  }
  bindings = $derived(resolve(this.value.keybindings));

  async load() {
    try {
      const loaded = await settingsLoad();
      // Move anyone still on the old built-in stack to the bundled font.
      this.value = loaded.font_family === OLD_FONT ? { ...loaded, font_family: FONT } : loaded;
    } catch {
      /* defaults are fine */
    }
    applyChrome(this.theme);
  }

  /** Apply immediately, then persist. Returns an error message if saving failed. */
  async update(next: Settings): Promise<string | null> {
    this.value = next;
    applyChrome(this.theme);
    try {
      this.value = await settingsSave(next);
      return null;
    } catch (e) {
      return `${e}`;
    }
  }
}

export const settings = new Store();
