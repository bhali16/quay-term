/** Tab state shown by the dot on a tab. Pure, so it can be tested without the UI. */
export type PaneState = "connecting" | "connected" | "ended";
export type TabState = "connecting" | "connected" | "ended" | "local";

/**
 * One state for a tab from its panes' states (undefined: the pane has not reported yet).
 * A tab is "ended" only when every pane is; "connecting" while any pane still is (remote tabs only).
 */
export function tabState(statuses: (PaneState | undefined)[], remote: boolean): TabState {
  if (statuses.length && statuses.every((s) => s === "ended")) return "ended";
  if (statuses.some((s) => s === "connecting" || s === undefined)) return remote ? "connecting" : "local";
  return remote ? "connected" : "local";
}

/** Forwards that are really running: a tunnel that failed to open does not count. */
export const runningTunnels = (list: { error: string | null }[]): number => list.filter((t) => !t.error).length;
