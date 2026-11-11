/**
 * Widths of the parts around the terminal, and what happens when the window is too small for them.
 * The window can be as small as 600 by 400, so nothing here may assume there is room.
 */

export type Range = { min: number; max: number; def: number };

export const SIDEBAR: Range = { min: 200, max: 420, def: 260 };
/** The panel on the right that holds Files and Tunnels. */
export const DOCK: Range = { min: 260, max: 640, def: 340 };
/** About 40 columns at the default font size, with the pane's padding. */
export const TERM_MIN = 380;
/** Below this window width the sidebar starts as the rail. */
export const NARROW = 720;
/** Below this strip width the tool buttons show icons only. */
export const TOOLS_COMPACT = 700;

export const clamp = (v: number, r: Range) => Math.min(r.max, Math.max(r.min, Math.round(v)));

/**
 * Where the dock goes in `space` pixels: beside the terminal when both fit (shrinking the dock
 * first), and on top of it when they do not, so the terminal never ends up a few columns wide.
 */
export function dockFit(want: number, space: number): { width: number; over: boolean } {
  if (space - want >= TERM_MIN) return { width: want, over: false };
  if (space - DOCK.min >= TERM_MIN) return { width: space - TERM_MIN, over: false };
  return { width: Math.max(200, Math.min(want, space - 48)), over: true };
}

export function readWidth(key: string, r: Range): number {
  try {
    const v = Number(localStorage.getItem(key));
    return Number.isFinite(v) && v > 0 ? clamp(v, r) : r.def;
  } catch {
    return r.def;
  }
}

export function writeWidth(key: string, value: number) {
  try {
    localStorage.setItem(key, String(value));
  } catch {
    /* the preference is optional */
  }
}

/**
 * Drag a width with the pointer. `sign` is 1 when moving right makes it wider (the sidebar's edge)
 * and -1 when moving left does (the dock's edge). `done` gets the final width once.
 */
export function dragWidth(e: PointerEvent, start: number, sign: 1 | -1, r: Range, set: (w: number) => void, done: (w: number) => void) {
  if (e.button !== 0) return;
  e.preventDefault();
  const x0 = e.clientX;
  let w = start;
  const move = (m: PointerEvent) => set((w = clamp(start + sign * (m.clientX - x0), r)));
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", up);
    done(w);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", up);
}
