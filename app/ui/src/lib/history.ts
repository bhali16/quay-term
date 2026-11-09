/**
 * Cross-session command history: what may be saved, and how the search view filters and groups
 * entries. Pure functions only; storage lives in Rust (`history.rs`).
 */
import type { HistoryEntry } from "./bindings/HistoryEntry";

/** Same list as `default_history_deny_patterns` in quay-config; used before settings load. */
export const DEFAULT_DENY = [
  String.raw`pass(word|wd)?\s*[=:]`,
  String.raw`--?(api[-_]?)?(token|secret|password)\b`,
  String.raw`authorization:`,
  String.raw`aws_secret`,
  String.raw`\bexport\s+\w*(key|secret|token|passw)\w*\s*=`,
  String.raw`\b(secret|token|api[-_]?key)\s*=`,
];

export type DenyList = { regexes: RegExp[]; invalid: string[] };

/** Compile the user's deny patterns (case-insensitive). Bad patterns are reported, not fatal. */
export function compileDeny(patterns: string[]): DenyList {
  const regexes: RegExp[] = [];
  const invalid: string[] = [];
  for (const p of patterns) {
    const src = p.trim();
    if (!src) continue;
    try {
      regexes.push(new RegExp(src, "i"));
    } catch {
      invalid.push(src);
    }
  }
  return { regexes, invalid };
}

/**
 * The text to save for a command as typed, or null when it must not be saved: typed with a
 * leading space (the shell convention for "keep this out of history"), empty, or matching
 * the deny-list.
 */
export function recordable(raw: string, deny: DenyList): string | null {
  if (/^\s/.test(raw)) return null;
  const cmd = raw.trim();
  if (!cmd) return null;
  if (deny.regexes.some((r) => r.test(cmd))) return null;
  return cmd;
}

export type Range = "all" | "today" | "week";
export type HistoryFilter = { host: string | null; failed: boolean; range: Range };
export const noFilter: HistoryFilter = { host: null, failed: false, range: "all" };

/** Earliest timestamp a range lets through: local midnight for today, seven days back for week. */
export function rangeStart(range: Range, now: number): number {
  if (range === "all") return -Infinity;
  if (range === "week") return now - 7 * 24 * 60 * 60 * 1000;
  const d = new Date(now);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

const failed = (e: HistoryEntry) => e.exit !== null && e.exit !== 0;

export function applyFilter(entries: HistoryEntry[], f: HistoryFilter, now: number): HistoryEntry[] {
  const from = rangeStart(f.range, now);
  return entries.filter((e) => (f.host === null || e.host === f.host) && (!f.failed || failed(e)) && e.ts >= from);
}

/** One search row per distinct command: its newest run, how many runs, and where it ran. */
export type HistoryRow = { entry: HistoryEntry; count: number; hosts: string[] };

/** Group newest-first entries by command text, keeping newest-first order. */
export function collapse(entries: HistoryEntry[]): HistoryRow[] {
  const rows = new Map<string, HistoryRow>();
  for (const e of entries) {
    const row = rows.get(e.command);
    if (!row) rows.set(e.command, { entry: e, count: 1, hosts: [e.host] });
    else {
      row.count++;
      if (!row.hosts.includes(e.host)) row.hosts.push(e.host);
    }
  }
  return [...rows.values()];
}

/** Hosts in the history, most recently used first. */
export function hostsOf(entries: HistoryEntry[]): string[] {
  return [...new Set(entries.map((e) => e.host))];
}

/**
 * The directory from an OSC 7 report (`file://host/path`, path percent-encoded or not).
 * Null for anything else.
 */
export function parseCwd(data: string): string | null {
  const m = /^file:\/\/[^/]*(\/.*)$/.exec(data.trim());
  if (!m) return null;
  try {
    return decodeURIComponent(m[1]);
  } catch {
    return m[1];
  }
}

/** "3m ago", "2h ago", "yesterday", "12 Mar". */
export function ago(ts: number, now: number): string {
  const s = Math.max(0, Math.round((now - ts) / 1000));
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  if (s < 2 * 86400) return "yesterday";
  if (s < 7 * 86400) return `${Math.floor(s / 86400)}d ago`;
  return new Date(ts).toLocaleDateString(undefined, { day: "numeric", month: "short" });
}
