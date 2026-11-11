/** The file panel's listing and transfer maths. Pure, so it can be tested without a server. */
import type { Entry } from "./bindings/Entry";

export type SortKey = "name" | "size" | "date";
export type Sort = { key: SortKey; asc: boolean };

export const DEFAULT_SORT: Sort = { key: "name", asc: true };

const byName = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/** Folders first, then by the chosen column; equal values fall back to the name. */
export function sortEntries(list: readonly Entry[], sort: Sort): Entry[] {
  const dir = sort.asc ? 1 : -1;
  const isDir = (e: Entry) => (e.kind === "dir" ? 0 : 1);
  return [...list].sort((a, b) => {
    if (isDir(a) !== isDir(b)) return isDir(a) - isDir(b);
    let d = 0;
    // A folder has no size of its own, so folders stay in name order under "size".
    if (sort.key === "size" && a.kind !== "dir") d = a.size - b.size;
    else if (sort.key === "date") d = (a.modified ?? 0) - (b.modified ?? 0);
    return d ? d * dir : byName.compare(a.name, b.name) * (sort.key === "name" ? dir : 1);
  });
}

/** Clicking a column sorts by it; clicking it again turns the order round. Size and date start with the largest and newest. */
export function nextSort(now: Sort, key: SortKey): Sort {
  return now.key === key ? { key, asc: !now.asc } : { key, asc: key === "name" };
}

export const isHidden = (name: string) => name.startsWith(".");

/** The folders of an absolute path, root first: "/var/log" gives /, /var, /var/log. */
export function crumbs(path: string): { name: string; path: string }[] {
  const out = [{ name: "/", path: "/" }];
  let at = "";
  for (const part of path.split("/").filter(Boolean)) {
    at += `/${part}`;
    out.push({ name: part, path: at });
  }
  return out;
}

export function sizeText(n: number): string {
  return n < 1024 ? `${n} B` : n < 1024 ** 2 ? `${(n / 1024).toFixed(1)} KB` : n < 1024 ** 3 ? `${(n / 1024 ** 2).toFixed(1)} MB` : `${(n / 1024 ** 3).toFixed(1)} GB`;
}

/** What is known about a running transfer's speed: when it was last measured, how far it was, bytes per second. */
export type Rate = { at: number; done: number; bps: number };

/** Fold a progress report (`done` bytes at `at` ms) into the speed, smoothed so the number does not jump. */
export function nextRate(prev: Rate | undefined, at: number, done: number): Rate {
  if (!prev) return { at, done, bps: 0 };
  const dt = (at - prev.at) / 1000;
  // Reports can come many times a second; measure over at least a quarter of one.
  if (dt < 0.25) return prev;
  const now = Math.max(0, done - prev.done) / dt;
  return { at, done, bps: prev.bps ? prev.bps * 0.6 + now * 0.4 : now };
}

/** "8 s", "3 min", "1 h 20 min". */
export function durationText(seconds: number): string {
  const s = Math.max(1, Math.round(seconds));
  if (s < 60) return `${s} s`;
  if (s < 3600) return `${Math.round(s / 60)} min`;
  const h = Math.floor(s / 3600);
  const m = Math.round((s % 3600) / 60);
  return m ? `${h} h ${m} min` : `${h} h`;
}

/** Progress of a running transfer in words: "42% · 1.2 MB/s · 8 s left". Parts that are not known yet are left out. */
export function progressText(done: number, total: number, bps: number): string {
  const parts: string[] = [];
  if (total > 0) parts.push(`${Math.min(100, Math.floor((done / total) * 100))}%`);
  else if (done > 0) parts.push(sizeText(done));
  if (bps > 0) {
    parts.push(`${sizeText(Math.round(bps))}/s`);
    if (total > done) parts.push(`${durationText((total - done) / bps)} left`);
  }
  return parts.join(" · ");
}
