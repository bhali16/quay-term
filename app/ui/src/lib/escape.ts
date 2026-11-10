/**
 * One Escape key for all dialogs. Every open dialog registers here, and only the one on top
 * gets the key, so a prompt shown over another dialog never closes both in one press.
 *
 * "On top" is the highest `level`, and among equals the one opened last. Prompts that are
 * always drawn above the rest (host key, sign-in) register at level 1.
 */
type Entry = { handler: () => void; level: number };

const stack: Entry[] = [];
let listening = false;

/** The entry that should receive Escape, or undefined when no dialog is open. */
export function top(entries: readonly Entry[] = stack): Entry | undefined {
  let best: Entry | undefined;
  for (const e of entries) if (!best || e.level >= best.level) best = e;
  return best;
}

function onKey(e: KeyboardEvent) {
  if (e.key !== "Escape") return;
  const entry = top();
  if (!entry) return;
  e.preventDefault();
  e.stopPropagation();
  entry.handler();
}

/** Register a dialog's Escape handler; call the returned function when the dialog closes. */
export function onEscape(handler: () => void, level = 0): () => void {
  const entry: Entry = { handler, level };
  stack.push(entry);
  if (!listening && typeof window !== "undefined") {
    // Capture phase: the dialog on top decides before any field or older listener sees the key.
    window.addEventListener("keydown", onKey, true);
    listening = true;
  }
  return () => {
    const i = stack.indexOf(entry);
    if (i >= 0) stack.splice(i, 1);
  };
}
