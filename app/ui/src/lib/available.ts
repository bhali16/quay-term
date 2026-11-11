/**
 * Which actions make sense in the tab that is in front. An action that cannot run says why,
 * in the palette (greyed out) and in a toast when its shortcut is pressed, instead of doing nothing.
 */
import type { Action } from "./keys";

/** `toggle_tunnels` has no shortcut of its own, but it is offered in the palette. */
export type Checked = Action | "toggle_tunnels";

export type TabContext = {
  /** The front tab is an SSH tab (quick connect or a saved session). */
  remote: boolean;
  /** Panes in the front tab. */
  panes: number;
  /** Open tabs. */
  tabs: number;
};

const NEEDS_SSH = "Only in an SSH tab. This one is a local shell.";
const NEEDS_SPLIT = "This tab has one pane. Split it first.";
const NEEDS_TABS = "Only one tab is open.";

/** Why `action` cannot run right now, as one sentence, or null when it can. */
export function unavailable(action: Checked, c: TabContext): string | null {
  switch (action) {
    case "toggle_files":
    case "toggle_tunnels":
      return c.remote ? null : NEEDS_SSH;
    case "zoom_pane":
    case "focus_left":
    case "focus_right":
    case "focus_up":
    case "focus_down":
    case "resize_left":
    case "resize_right":
    case "resize_up":
    case "resize_down":
      return c.panes > 1 ? null : NEEDS_SPLIT;
    case "next_tab":
    case "prev_tab":
      return c.tabs > 1 ? null : NEEDS_TABS;
    default:
      return null;
  }
}

/** Put `id` first in a most-recent-first list of at most `max`. */
export function pushRecent<T>(list: readonly T[], id: T, max: number): T[] {
  return [id, ...list.filter((x) => x !== id)].slice(0, max);
}
