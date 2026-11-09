/** Subsequence fuzzy matching for the command palette. Pure functions only. */

/**
 * Score `query` against `text`; null when the letters do not appear in order.
 * Higher is better: word starts and consecutive letters score extra, gaps and late matches cost.
 */
export function score(query: string, text: string): number | null {
  const q = query.toLowerCase().replace(/\s+/g, "");
  if (!q) return 0;
  const t = text.toLowerCase();
  let ti = 0;
  let total = 0;
  let run = 0;
  let first = -1;
  for (const ch of q) {
    const at = t.indexOf(ch, ti);
    if (at < 0) return null;
    if (first < 0) first = at;
    const wordStart = at === 0 || /[\s\-_./@:]/.test(t[at - 1]);
    run = at === ti && ti > 0 ? run + 1 : 0;
    total += 1 + (wordStart ? 6 : 0) + run * 3 - Math.min(at - ti, 8) * 0.5;
    ti = at + 1;
  }
  // An exact substring beats a scattered match of the same letters.
  if (t.includes(q)) total += 10;
  // Prefer matches that start early and short targets.
  return total - first * 0.2 - t.length * 0.02;
}

export type Scored<T> = { item: T; score: number };

/** Items matching `query` best-first; the original order is kept for ties and for an empty query. */
export function rank<T>(items: T[], query: string, fields: (item: T) => string[]): T[] {
  if (!query.trim()) return items;
  const scored: Scored<T>[] = [];
  items.forEach((item) => {
    let best: number | null = null;
    for (const f of fields(item)) {
      const s = score(query, f);
      if (s !== null && (best === null || s > best)) best = s;
    }
    if (best !== null) scored.push({ item, score: best });
  });
  // Array.sort is stable, so equal scores keep their input order.
  return scored.sort((a, b) => b.score - a.score).map((s) => s.item);
}

/** Split a leading mode character (`>` actions, `@` sessions, `!` snippets, `/` tabs, `$` history) off the query. */
export function parseQuery(raw: string): { mode: string | null; query: string } {
  const m = raw.match(/^([>@!/$])\s*(.*)$/s);
  return m ? { mode: m[1], query: m[2] } : { mode: null, query: raw };
}
