import { settingsLoad, settingsSave } from "./ipc";
import type { Settings } from "./bindings/Settings";
import { applyChrome, themeById } from "./themes";
import { resolve } from "./keys";

const fallback: Settings = {
  theme: "quay-dark",
  font_family: "Menlo, Consolas, 'DejaVu Sans Mono', monospace",
  font_size: 14,
  cursor_blink: true,
  scrollback: 5000,
  keybindings: {},
};

class Store {
  value = $state<Settings>(fallback);
  theme = $derived(themeById(this.value.theme));
  bindings = $derived(resolve(this.value.keybindings));

  async load() {
    try {
      this.value = await settingsLoad();
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
