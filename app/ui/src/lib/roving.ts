/**
 * Arrow-key movement inside one widget (a tab list, a menu), so the widget is a single Tab stop.
 */

/**
 * The index focus moves to for `key`, or null when the key does not navigate.
 * Arrows wrap around; Home and End jump to the ends.
 */
export function step(key: string, index: number, count: number, vertical = false): number | null {
  if (count === 0) return null;
  switch (key) {
    case vertical ? "ArrowDown" : "ArrowRight":
      return (index + 1) % count;
    case vertical ? "ArrowUp" : "ArrowLeft":
      return (index - 1 + count) % count;
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}

/** The next item after `index` whose text starts with `typed` (case-insensitive), or -1. */
export function typeAhead(labels: readonly string[], index: number, typed: string): number {
  const want = typed.toLowerCase();
  for (let n = 1; n <= labels.length; n++) {
    const i = (index + n) % labels.length;
    if (labels[i].trim().toLowerCase().startsWith(want)) return i;
  }
  return -1;
}

type Options = {
  /** Matches the items, in order, inside the node. */
  selector: string;
  /** Up and Down move instead of Left and Right. */
  vertical?: boolean;
  /** Also click the item focus lands on (tabs that select on focus). */
  activate?: boolean;
};

/** Svelte action: arrow keys, Home and End move focus between the items of `node`. */
export function roving(node: HTMLElement, options: Options) {
  let o = options;
  function onKey(e: KeyboardEvent) {
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    const items = [...node.querySelectorAll<HTMLElement>(o.selector)];
    // Only when an item itself has focus: a rename field inside a tab keeps its own arrow keys.
    const at = items.indexOf(e.target as HTMLElement);
    const to = at < 0 ? null : step(e.key, at, items.length, o.vertical);
    if (to === null) return;
    e.preventDefault();
    items[to].focus();
    if (o.activate) items[to].click();
  }
  node.addEventListener("keydown", onKey);
  return {
    update(next: Options) {
      o = next;
    },
    destroy() {
      node.removeEventListener("keydown", onKey);
    },
  };
}
