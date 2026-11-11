/** What a toast says and how long it stays. The list itself is in `toastStore.svelte.ts`. */

export type ToastKind = "info" | "success" | "error";

/** Low-level failures that reach the UI as they are, and the sentence shown instead. */
const RULES: [RegExp, string][] = [
  [/timed? ?out|deadline has elapsed/i, "The server did not answer in time. Check the host, the port and your network."],
  [/network (is )?unreachable|no route to host/i, "The network cannot reach that host."],
  [/connection reset|broken pipe|unexpected eof|session closed|channel closed/i, "The connection was closed by the other side."],
  [/secret vault is locked/i, "The secret vault is locked. Unlock it in Settings > Backup."],
  [/keychain/i, "The system keychain could not be used."],
  [/permission denied|os error 13\b/i, "Permission denied."],
  [/no such file or directory|os error 2\b/i, "That file or folder does not exist."],
  [/database/i, "Could not read or save your data on this computer."],
];

const MAX = 160;

/**
 * An error as one plain sentence, with the original kept as `detail` when it says more.
 * Messages the backend already wrote for people (they start with a capital) are left alone;
 * raw ones ("i/o error: ...") are matched against the common causes.
 */
export function plainError(e: unknown): { text: string; detail?: string } {
  const raw = `${e instanceof Error ? e.message : e}`.replace(/^Error:\s*/, "").trim();
  if (!raw) return { text: "Something went wrong." };
  if (/^\p{Lu}/u.test(raw)) {
    if (raw.length <= MAX) return { text: raw };
    // Cut at the end of the first sentence, or at a word.
    const stop = raw.slice(0, MAX).search(/[.!?](\s|$)/);
    const text = stop > 20 ? raw.slice(0, stop + 1) : raw.slice(0, MAX).replace(/\s+\S*$/, "") + "…";
    return { text, detail: raw };
  }
  const rule = RULES.find(([re]) => re.test(raw));
  if (rule) return { text: rule[1], detail: raw };
  const text = raw[0].toUpperCase() + raw.slice(1) + (/[.!?…]$/.test(raw) ? "" : ".");
  return text.length <= MAX ? { text } : { text: text.slice(0, MAX).replace(/\s+\S*$/, "") + "…", detail: raw };
}

/**
 * How long a toast shows, in ms, or null to stay until dismissed. Errors stay. A toast with a
 * button gets time to use it. The rest get reading time for their length.
 */
export function durationFor(kind: ToastKind, text: string, hasAction: boolean): number | null {
  if (kind === "error") return null;
  const reading = Math.min(10000, Math.max(4500, text.length * 60));
  return hasAction ? Math.max(8000, reading) : reading;
}
