/** Snippet helpers: `{{name}}` variables, search and folder grouping. Pure functions only. */
import type { Snippet } from "./bindings/Snippet";

const VAR = /\{\{\s*([A-Za-z_][A-Za-z0-9_]*)\s*(?::([^}]*))?\}\}/g;

export type Variable = { name: string; fallback: string };

/** Variables in order of first use. `{{name:default}}` gives a default. */
export function variables(body: string): Variable[] {
  const seen = new Map<string, Variable>();
  for (const m of body.matchAll(VAR)) if (!seen.has(m[1])) seen.set(m[1], { name: m[1], fallback: m[2]?.trim() ?? "" });
  return [...seen.values()];
}

/** Replace every variable; one with no value falls back to its default. */
export function fill(body: string, values: Record<string, string>): string {
  return body.replace(VAR, (_, name: string, fallback?: string) => values[name] ?? fallback?.trim() ?? "");
}

/** What a terminal receives: lines joined with CR, as typing Enter does, plus a final Enter on request. */
export function toInput(text: string, enter: boolean): string {
  const body = text.replace(/\r?\n/g, "\r").replace(/\r+$/, "");
  return enter ? body + "\r" : body;
}

/** Case-insensitive match on name, folder and body; every word must appear. */
export function matches(s: Snippet, query: string): boolean {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const hay = `${s.name}\n${s.folder}\n${s.body}`.toLowerCase();
  return words.every((w) => hay.includes(w));
}

export type Group = { folder: string; snippets: Snippet[] };

/** Snippets grouped by folder: the unfiled ones first, then folders A-Z; names A-Z inside. */
export function group(list: Snippet[]): Group[] {
  const by = new Map<string, Snippet[]>();
  for (const s of list) by.set(s.folder, [...(by.get(s.folder) ?? []), s]);
  const byName = (a: Snippet, b: Snippet) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
  return [...by.entries()]
    .sort(([a], [b]) => (a === "" ? -1 : b === "" ? 1 : a.localeCompare(b, undefined, { sensitivity: "base" })))
    .map(([folder, snippets]) => ({ folder, snippets: snippets.sort(byName) }));
}
