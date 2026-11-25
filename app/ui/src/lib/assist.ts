/**
 * Input assist (F9): host suggestions after `ssh`, `scp` and `sftp`, and remote path completion.
 * Pure functions only. They read the text typed at a marked prompt (OSC 133 B up to the cursor)
 * and say what to offer and what to type when an offer is taken. Nothing here sends keys.
 */

export type Tool = "ssh" | "scp" | "sftp";

/** Where the cursor is when a host can be typed. */
export type HostContext = {
  tool: Tool;
  /** The whole word under the cursor, as typed (`root@we`). */
  word: string;
  /** `root` in `root@we`; null when the word has no `@`. */
  user: string | null;
  /** The host part typed so far (`we`). */
  fragment: string;
  /** The word is the value of `-J` (a jump host), which takes `user@host:port`. */
  jump: boolean;
};

export type HostCandidate = {
  /** Shown in bold. A config alias, or the saved session's name. */
  label: string;
  /** Shown dimmed: what it points at. */
  detail: string;
  /** `config` (from ~/.ssh/config) or `saved` (a saved session). */
  source: "config" | "saved";
  host: string;
  /** The saved session's user; null for a config alias (the config supplies it). */
  user: string | null;
  port: number;
};

/** What to type to take an offer: erase `erase` characters, then type `text`. */
export type Edit = { erase: number; text: string };

/** Options that take a value, per tool (so `ssh -p 22 host` knows `22` is not the host). */
const VALUE_OPTS: Record<Tool, string> = {
  ssh: "BbcDEeFIiJLlmOoPpQRSWw",
  scp: "cDFiJloPSX",
  sftp: "BbcDFiJloPRSsX",
};

/** Characters that make the line too clever to parse safely: quotes, pipes, substitutions. */
const UNSAFE = /['"`\\|;&$()<>]/;

/**
 * The host being typed at the end of `line`, or null when the cursor is not on a host.
 * Only the simple case is handled: the line starts with the tool, and holds no quotes,
 * escapes, pipes or substitutions. Anything else is left to the shell.
 */
export function hostContext(line: string): HostContext | null {
  if (UNSAFE.test(line)) return null;
  const trimmed = line.replace(/^\s+/, "");
  const words = trimmed.split(/\s+/);
  if (words.length < 2) return null;
  const tool = words[0] as Tool;
  if (!(tool in VALUE_OPTS)) return null;
  const word = words[words.length - 1];
  const before = words.slice(1, -1);
  const opts = VALUE_OPTS[tool];

  let positional = 0;
  let pending: string | null = null;
  let endOfOpts = false;
  for (const w of before) {
    if (pending !== null) {
      pending = null;
      continue;
    }
    if (!endOfOpts && w === "--") {
      endOfOpts = true;
      continue;
    }
    if (!endOfOpts && w.startsWith("-") && w.length > 1) {
      for (let i = 1; i < w.length; i++) {
        if (opts.includes(w[i])) {
          if (i === w.length - 1) pending = w[i];
          break;
        }
      }
      continue;
    }
    positional++;
  }

  const jump = pending === "J";
  if (pending !== null && !jump) return null;
  if (!jump && !endOfOpts && word.startsWith("-")) return null;
  // ssh and sftp take one destination, first; for scp every argument can be remote.
  if (!jump && tool !== "scp" && positional > 0) return null;

  const at = word.lastIndexOf("@");
  const user = at >= 0 ? word.slice(0, at) : null;
  const fragment = at >= 0 ? word.slice(at + 1) : word;
  // Past the host already (`host:path`), or clearly a local path.
  if (/[:/]/.test(fragment) || /^[.~]/.test(fragment)) return null;
  if (user !== null && (user === "" || /[:/]/.test(user))) return null;
  return { tool, word, user, fragment, jump };
}

/** Hosts from ~/.ssh/config first, then saved sessions that do not point at the same place. */
export function buildHosts(
  config: { alias: string; host_name: string; user: string | null; port: number }[],
  saved: { name: string; host: string; user: string; port: number }[],
): HostCandidate[] {
  const out: HostCandidate[] = [];
  const seen = new Set<string>();
  for (const c of config) {
    if (seen.has(c.alias.toLowerCase())) continue;
    seen.add(c.alias.toLowerCase());
    const target = `${c.user ? `${c.user}@` : ""}${c.host_name}${c.port !== 22 ? `:${c.port}` : ""}`;
    out.push({ label: c.alias, detail: target === c.alias ? "ssh config" : target, source: "config", host: c.alias, user: null, port: 22 });
  }
  // A session imported from the config has the alias as its name: the alias already covers it.
  const aliases = new Set(config.map((c) => c.alias.toLowerCase()));
  const targets = new Set<string>();
  for (const s of saved) {
    if (aliases.has(s.name.toLowerCase()) || !s.host) continue;
    const key = `${s.user}@${s.host}:${s.port}`.toLowerCase();
    if (targets.has(key)) continue;
    targets.add(key);
    out.push({
      label: s.name,
      detail: `${s.user}@${s.host}${s.port !== 22 ? `:${s.port}` : ""}`,
      source: "saved",
      host: s.host,
      user: s.user || null,
      port: s.port,
    });
  }
  return out;
}

/**
 * The hosts that fit what was typed, best first: a prefix of the alias, name or host name,
 * then anywhere in them. Case-insensitive. Empty when the fragment is already a full match.
 */
export function matchHosts(ctx: HostContext, hosts: HostCandidate[], max = 8): HostCandidate[] {
  const f = ctx.fragment.toLowerCase();
  const fields = (h: HostCandidate) => [h.label, h.host].map((s) => s.toLowerCase());
  const prefix = hosts.filter((h) => fields(h).some((s) => s.startsWith(f)));
  const inside = f ? hosts.filter((h) => !prefix.includes(h) && fields(h).some((s) => s.includes(f))) : [];
  const list = [...prefix, ...inside].slice(0, max);
  if (list.length === 1 && hostWord(ctx, list[0]) === ctx.word) return [];
  return list;
}

/** The word a host turns into: the alias, or `user@host` for a saved session. */
function hostWord(ctx: HostContext, h: HostCandidate): string {
  const user = ctx.user ?? h.user;
  let word = `${user ? `${user}@` : ""}${h.host}`;
  if (ctx.jump && h.port !== 22) word += `:${h.port}`;
  return word;
}

/** Characters both strings start with. */
function common(a: string, b: string): number {
  let i = 0;
  while (i < a.length && i < b.length && a[i] === b[i]) i++;
  return i;
}

/**
 * What to type to take `h`. Only the part after what both words share is erased and retyped,
 * so the shell's line stays as the user left it. A saved session on another port gets the
 * port option in front (`-p 2222 user@host` for ssh, `-P` for scp and sftp).
 */
export function hostEdit(ctx: HostContext, h: HostCandidate): Edit {
  let word = hostWord(ctx, h);
  if (!ctx.jump && h.port !== 22) word = `${ctx.tool === "ssh" ? "-p" : "-P"} ${h.port} ${word}`;
  const keep = common(ctx.word, word);
  const after = ctx.tool === "scp" && !ctx.jump ? ":" : " ";
  return { erase: ctx.word.length - keep, text: word.slice(keep) + after };
}

// ---- remote paths ----

/** The path being typed at the end of `line`, resolved to a remote folder and a name prefix. */
export type PathContext = {
  /** Remote folder to list, absolute and without a trailing slash (except `/`). */
  dir: string;
  /** Start of the name typed in that folder (unescaped). */
  prefix: string;
  /** Typed before any name: `/` after a bare `~`. */
  lead: string;
};

export type PathResult = { ok: true; ctx: PathContext } | { ok: false; reason: string };

/** The last shell word of `line`, unescaped; null if it is inside quotes or ends in a lone `\`. */
export function lastWord(line: string): string | null {
  let word = "";
  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (ch === "\\") {
      if (i + 1 >= line.length) return null;
      word += line[++i];
    } else if (ch === "'" || ch === '"') return null;
    else if (/\s/.test(ch)) word = "";
    else word += ch;
  }
  return word;
}

/** Join and tidy a remote path: `a/./b/../c` becomes `a/c`. */
export function joinPath(base: string, rel: string): string {
  const parts = (rel.startsWith("/") ? rel : `${base}/${rel}`).split("/");
  const out: string[] = [];
  for (const p of parts) {
    if (p === "" || p === ".") continue;
    if (p === "..") out.pop();
    else out.push(p);
  }
  return "/" + out.join("/");
}

/**
 * Where to look for the path at the end of `line`. `cwd` is the remote shell's folder when it
 * reported one (OSC 7), `home` the SFTP home. A relative path needs `cwd`: guessing the home
 * folder would offer names from the wrong place.
 */
export function pathContext(line: string, cwd: string | null, home: string): PathResult {
  const word = lastWord(line);
  if (word === null) return { ok: false, reason: "Paths inside quotes are not completed." };
  // `--file=/etc/ho` completes the part after `=`.
  const value = word.includes("=") && !word.startsWith("/") ? word.slice(word.indexOf("=") + 1) : word;
  if (/^[^/]*:/.test(value) && !value.startsWith("/")) return { ok: false, reason: "That path is on another host." };
  let base: string;
  let rel: string;
  if (value === "~" || value.startsWith("~/")) {
    base = home;
    rel = value === "~" ? "" : value.slice(2);
  } else if (value.startsWith("/")) {
    base = "/";
    rel = value;
  } else if (value.startsWith("~")) {
    return { ok: false, reason: "Other users' home folders are not completed." };
  } else {
    if (cwd === null) return { ok: false, reason: "The shell has not said which folder it is in. Start the path with / or ~/." };
    base = cwd;
    rel = value;
  }
  const slash = rel.lastIndexOf("/");
  const dirPart = slash >= 0 ? rel.slice(0, slash + 1) : "";
  const prefix = slash >= 0 ? rel.slice(slash + 1) : rel;
  return { ok: true, ctx: { dir: joinPath(base, dirPart), prefix, lead: value === "~" ? "/" : "" } };
}

export type PathEntry = { name: string; dir: boolean };

/** Names in the folder that start with the prefix, sorted. Hidden ones only when the prefix starts with a dot. */
export function matchPaths(prefix: string, entries: PathEntry[], max = 50): PathEntry[] {
  return entries
    .filter((e) => e.name !== "." && e.name !== ".." && e.name.startsWith(prefix) && (prefix.startsWith(".") || !e.name.startsWith(".")))
    .sort((a, b) => a.name.localeCompare(b.name))
    .slice(0, max);
}

/** Backslash the characters a shell would read as something other than part of a name. */
export function shellEscape(s: string): string {
  return s.replace(/[\s\\'"`$!&;|()<>*?[\]{}#~^]/g, "\\$&");
}

/** What to type to take entry `e`: the rest of its name, escaped, with `/` after a folder. */
export function pathEdit(ctx: PathContext, e: PathEntry): Edit {
  return { erase: 0, text: ctx.lead + shellEscape(e.name.slice(ctx.prefix.length)) + (e.dir ? "/" : "") };
}

/** Longest start that every name shares (what a shell types before it shows a list). */
export function sharedStart(names: string[]): string {
  if (!names.length) return "";
  let s = names[0];
  for (const n of names.slice(1)) s = s.slice(0, common(s, n));
  return s;
}
