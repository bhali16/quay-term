/** Split-pane layout for one tab: a binary tree whose leaves are terminals. Pure functions only. */
export type Axis = "row" | "col"; // row: side by side, col: stacked
export type Dir = "left" | "right" | "up" | "down";
export type Node = { leaf: number } | { axis: Axis; ratio: number; a: Node; b: Node };
export type Rect = { x: number; y: number; w: number; h: number }; // fractions of the tab area

export const MIN_RATIO = 0.1;
const clamp = (r: number) => Math.min(1 - MIN_RATIO, Math.max(MIN_RATIO, r));

export const isLeaf = (n: Node): n is { leaf: number } => "leaf" in n;

export function leaves(n: Node): number[] {
  return isLeaf(n) ? [n.leaf] : [...leaves(n.a), ...leaves(n.b)];
}

/** Replace leaf `id` with a split holding it and `newId` (the new pane goes right or below). */
export function split(n: Node, id: number, newId: number, axis: Axis): Node {
  if (isLeaf(n)) return n.leaf === id ? { axis, ratio: 0.5, a: n, b: { leaf: newId } } : n;
  return { ...n, a: split(n.a, id, newId, axis), b: split(n.b, id, newId, axis) };
}

/** Remove leaf `id`; its sibling takes the space. Null when it was the only leaf. */
export function remove(n: Node, id: number): Node | null {
  if (isLeaf(n)) return n.leaf === id ? null : n;
  const a = remove(n.a, id);
  const b = remove(n.b, id);
  if (!a) return b;
  if (!b) return a;
  return { ...n, a, b };
}

export type Divider = { path: string; axis: Axis; rect: Rect; parent: Rect };
export type Layout = { panes: { id: number; rect: Rect }[]; dividers: Divider[] };

/** Rectangles for every leaf and a draggable divider for every split. `path` is "a"/"b" steps from the root. */
export function layout(n: Node, rect: Rect = { x: 0, y: 0, w: 1, h: 1 }, path = ""): Layout {
  if (isLeaf(n)) return { panes: [{ id: n.leaf, rect }], dividers: [] };
  const row = n.axis === "row";
  const ra = row ? { ...rect, w: rect.w * n.ratio } : { ...rect, h: rect.h * n.ratio };
  const rb = row
    ? { ...rect, x: rect.x + ra.w, w: rect.w - ra.w }
    : { ...rect, y: rect.y + ra.h, h: rect.h - ra.h };
  const la = layout(n.a, ra, path + "a");
  const lb = layout(n.b, rb, path + "b");
  const line: Rect = row ? { x: rb.x, y: rect.y, w: 0, h: rect.h } : { x: rect.x, y: rb.y, w: rect.w, h: 0 };
  return {
    panes: [...la.panes, ...lb.panes],
    dividers: [{ path, axis: n.axis, rect: line, parent: rect }, ...la.dividers, ...lb.dividers],
  };
}

function update(n: Node, path: string, f: (s: Extract<Node, { axis: Axis }>) => Node): Node {
  if (isLeaf(n)) return n;
  if (path === "") return f(n);
  return path[0] === "a" ? { ...n, a: update(n.a, path.slice(1), f) } : { ...n, b: update(n.b, path.slice(1), f) };
}

export function setRatio(n: Node, path: string, ratio: number): Node {
  return update(n, path, (s) => ({ ...s, ratio: clamp(ratio) }));
}

/** Move the divider nearest to `id` on the axis of `dir` by `step` (a fraction of that split). */
export function nudge(n: Node, id: number, dir: Dir, step = 0.05): Node {
  const axis: Axis = dir === "left" || dir === "right" ? "row" : "col";
  const sign = dir === "right" || dir === "down" ? 1 : -1;
  let best = "";
  let found = false;
  const walk = (m: Node, path: string): boolean => {
    if (isLeaf(m)) return m.leaf === id;
    const inA = walk(m.a, path + "a");
    const inB = !inA && walk(m.b, path + "b");
    if ((inA || inB) && m.axis === axis && !found) {
      best = path;
      found = true; // deepest first: the recursion reaches the innermost match before returning here
    }
    return inA || inB;
  };
  walk(n, "");
  return found ? update(n, best, (s) => ({ ...s, ratio: clamp(s.ratio + sign * step) })) : n;
}

/** The pane next to `id` in direction `dir`, judged by the centre of its rectangle. */
export function neighbor(l: Layout, id: number, dir: Dir): number | null {
  const me = l.panes.find((p) => p.id === id)?.rect;
  if (!me) return null;
  const eps = 1e-6;
  const cx = me.x + me.w / 2;
  const cy = me.y + me.h / 2;
  let best: { id: number; d: number } | null = null;
  for (const p of l.panes) {
    if (p.id === id) continue;
    const r = p.rect;
    const touches =
      dir === "right" ? Math.abs(r.x - (me.x + me.w)) < eps && r.y < me.y + me.h - eps && r.y + r.h > me.y + eps
      : dir === "left" ? Math.abs(r.x + r.w - me.x) < eps && r.y < me.y + me.h - eps && r.y + r.h > me.y + eps
      : dir === "down" ? Math.abs(r.y - (me.y + me.h)) < eps && r.x < me.x + me.w - eps && r.x + r.w > me.x + eps
      : Math.abs(r.y + r.h - me.y) < eps && r.x < me.x + me.w - eps && r.x + r.w > me.x + eps;
    if (!touches) continue;
    const d = dir === "left" || dir === "right" ? Math.abs(r.y + r.h / 2 - cy) : Math.abs(r.x + r.w / 2 - cx);
    if (!best || d < best.d) best = { id: p.id, d };
  }
  return best?.id ?? null;
}

/** Layout as stored with a saved session: the tree without pane ids. */
export type Shape = null | { axis: Axis; ratio: number; a: Shape; b: Shape };

export function shapeOf(n: Node): Shape {
  return isLeaf(n) ? null : { axis: n.axis, ratio: n.ratio, a: shapeOf(n.a), b: shapeOf(n.b) };
}

/** Rebuild a tree from a stored shape, handing out ids from `next`. Null when the data is not a valid shape. */
export function fromShape(s: unknown, next: () => number, depth = 0): Node | null {
  if (s === null) return { leaf: next() };
  if (typeof s !== "object" || depth > 8) return null;
  const o = s as Record<string, unknown>;
  if ((o.axis !== "row" && o.axis !== "col") || typeof o.ratio !== "number" || !Number.isFinite(o.ratio)) return null;
  const a = fromShape(o.a, next, depth + 1);
  const b = a && fromShape(o.b, next, depth + 1);
  return a && b ? { axis: o.axis, ratio: clamp(o.ratio), a, b } : null;
}
