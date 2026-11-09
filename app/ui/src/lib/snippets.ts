/** Snippet helpers: `{{name}}` variables, search and folder grouping. Pure functions only. */
import type { Snippet } from "./bindings/Snippet";

// `\{{` is a literal `{{`. Otherwise `{{name}}`, `{{name:default}}` (everything after `:` is the
// default, as in older snippets), `{{name|default}}`, or `{{name|a|b|c}}` for a choice list.
const VAR = /\\(\{\{)|\{\{\s*([A-Za-z_][A-Za-z0-9_]*)\s*(?:(:|\|)([^}]*))?\}\}/g;

export type Variable = { name: string; fallback: string; choices: string[] };

function parse(sep: string | undefined, rest: string | undefined): { fallback: string; choices: string[] } {
  if (rest === undefined) return { fallback: "", choices: [] };
  if (sep === ":") return { fallback: rest.trim(), choices: [] };
  const parts = rest.split("|").map((x) => x.trim());
  return parts.length > 1 ? { fallback: parts[0], choices: [...new Set(parts.filter(Boolean))] } : { fallback: parts[0], choices: [] };
}

/** Variables in order of first use; the first use decides the default and choices. */
export function variables(body: string): Variable[] {
  const seen = new Map<string, Variable>();
  for (const m of body.matchAll(VAR)) {
    if (m[1] || seen.has(m[2])) continue;
    seen.set(m[2], { name: m[2], ...parse(m[3], m[4]) });
  }
  return [...seen.values()];
}

/** Starting values for the form: this session's remembered values, then the snippet's defaults. */
export function initialValues(body: string, remembered: Record<string, string> = {}): Record<string, string> {
  return Object.fromEntries(variables(body).map((v) => [v.name, remembered[v.name] ?? v.fallback]));
}

/** Replace every variable; one with no value falls back to its default. `\{{` becomes `{{`. */
export function fill(body: string, values: Record<string, string>): string {
  return body.replace(VAR, (_, esc: string | undefined, name: string, sep?: string, rest?: string) =>
    esc ? esc : (values[name] ?? parse(sep, rest).fallback),
  );
}

/** A snippet's short name for `;name` expansion: lower case, words joined with dashes. */
export function slug(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

/** The snippet a `;name` typed at the prompt refers to: an exact short name wins, then a unique prefix. */
export function byShortName(list: Snippet[], typed: string): Snippet | null {
  const t = typed.toLowerCase();
  if (!t) return null;
  const exact = list.filter((s) => slug(s.name) === t);
  if (exact.length) return exact[0];
  const pre = list.filter((s) => slug(s.name).startsWith(t));
  return pre.length === 1 ? pre[0] : null;
}

/** The `;name` at the end of what was typed at the prompt, or null. */
export function expansionAt(line: string): string | null {
  const m = /(?:^|\s);([a-z0-9][a-z0-9-]*)$/i.exec(line);
  return m ? m[1] : null;
}

/**
 * What a terminal receives: lines joined with CR, as typing Enter does, plus a final Enter on request.
 * Without the final Enter, several lines are wrapped as a bracketed paste when the shell asked for that,
 * so the earlier lines wait in the prompt instead of running one by one.
 */
export function toInput(text: string, enter: boolean, bracketed = false): string {
  const body = text.replace(/\r?\n/g, "\r").replace(/\r+$/, "");
  if (enter) return body + "\r";
  if (bracketed && body.includes("\r")) return `\x1b[200~${body.replace(/\x1b\[20[01]~/g, "")}\x1b[201~`;
  return body;
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
