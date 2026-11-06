import { describe, expect, it } from "vitest";
import { dropTarget, edgeAt, edgeRect, fromShape, layout, leaves, move, neighbor, nudge, remove, setRatio, shapeOf, split, zoomView, type Node } from "./panes";

const two: Node = split({ leaf: 1 }, 1, 2, "row");
const grid: Node = split(split({ leaf: 1 }, 1, 2, "row"), 2, 3, "col"); // 1 | (2 over 3)

describe("split and remove", () => {
  it("puts the new pane after the old one", () => {
    expect(leaves(two)).toEqual([1, 2]);
    expect(leaves(grid)).toEqual([1, 2, 3]);
  });
  it("gives the space of a closed pane to its sibling", () => {
    expect(remove(grid, 3)).toEqual(two);
    expect(remove(two, 1)).toEqual({ leaf: 2 });
    expect(remove({ leaf: 1 }, 1)).toBeNull();
  });
});

describe("layout", () => {
  it("tiles the area", () => {
    const l = layout(grid);
    const area = l.panes.reduce((s, p) => s + p.rect.w * p.rect.h, 0);
    expect(area).toBeCloseTo(1);
    expect(l.dividers).toHaveLength(2);
    expect(l.panes.find((p) => p.id === 3)!.rect).toEqual({ x: 0.5, y: 0.5, w: 0.5, h: 0.5 });
  });
});

describe("resize", () => {
  it("clamps a drag so no pane vanishes", () => {
    expect((setRatio(two, "", 0) as any).ratio).toBe(0.1);
    expect((setRatio(two, "", 5) as any).ratio).toBe(0.9);
  });
  it("nudges the divider next to a pane on the requested axis", () => {
    expect((nudge(two, 1, "right") as any).ratio).toBeCloseTo(0.55);
    expect((nudge(two, 2, "left") as any).ratio).toBeCloseTo(0.45);
    // pane 3 has no side-by-side split above it except the root, so "right" moves the root
    expect((nudge(grid, 3, "right") as any).ratio).toBeCloseTo(0.55);
    expect((nudge(grid, 3, "up") as any).b.ratio).toBeCloseTo(0.45);
    expect(nudge({ leaf: 1 }, 1, "left")).toEqual({ leaf: 1 });
  });
});

describe("neighbor", () => {
  const l = layout(grid);
  it("finds the pane across an edge", () => {
    expect(neighbor(l, 1, "right")).toBe(2);
    expect(neighbor(l, 3, "left")).toBe(1);
    expect(neighbor(l, 2, "down")).toBe(3);
    expect(neighbor(l, 3, "up")).toBe(2);
  });
  it("returns null at the edge of the window", () => {
    expect(neighbor(l, 1, "left")).toBeNull();
    expect(neighbor(l, 3, "down")).toBeNull();
  });
});

describe("stored shape", () => {
  it("round-trips without pane ids", () => {
    let n = 10;
    const back = fromShape(JSON.parse(JSON.stringify(shapeOf(grid))), () => ++n)!;
    expect(leaves(back)).toEqual([11, 12, 13]);
    expect(shapeOf(back)).toEqual(shapeOf(grid));
  });
  it("rejects junk", () => {
    expect(fromShape({ axis: "diag", ratio: 0.5, a: null, b: null }, () => 1)).toBeNull();
    expect(fromShape({ axis: "row", ratio: "x", a: null, b: null }, () => 1)).toBeNull();
    expect(fromShape({ axis: "row", ratio: 0.5, a: null }, () => 1)).toBeNull();
  });
});

describe("move", () => {
  const rectOf = (n: Node, id: number) => layout(n).panes.find((p) => p.id === id)!.rect;
  it("places the pane on each edge of the target", () => {
    const left = move(two, 2, 1, "left");
    expect(leaves(left)).toEqual([2, 1]);
    expect(left).toMatchObject({ axis: "row", ratio: 0.5 });
    expect(leaves(move(two, 1, 2, "right"))).toEqual([2, 1]);
    const top = move(two, 2, 1, "up");
    expect(top).toMatchObject({ axis: "col" });
    expect(leaves(top)).toEqual([2, 1]);
    const bottom = move(two, 1, 2, "down");
    expect(bottom).toMatchObject({ axis: "col" });
    expect(leaves(bottom)).toEqual([2, 1]);
  });
  it("collapses the old parent so the sibling takes the space", () => {
    // grid is 1 | (2 over 3). Moving 3 under 1 leaves 2 alone in the right column.
    const n = move(grid, 3, 1, "down");
    expect(leaves(n).sort()).toEqual([1, 2, 3]);
    expect(rectOf(n, 2)).toEqual({ x: 0.5, y: 0, w: 0.5, h: 1 });
    expect(rectOf(n, 1)).toEqual({ x: 0, y: 0, w: 0.5, h: 0.5 });
    expect(rectOf(n, 3)).toEqual({ x: 0, y: 0.5, w: 0.5, h: 0.5 });
  });
  it("keeps every pane and tiles the area", () => {
    for (const edge of ["left", "right", "up", "down"] as const) {
      const n = move(grid, 1, 3, edge);
      expect(leaves(n).sort()).toEqual([1, 2, 3]);
      expect(layout(n).panes.reduce((s, p) => s + p.rect.w * p.rect.h, 0)).toBeCloseTo(1);
    }
  });
  it("does nothing for a single pane, for itself, or for unknown ids", () => {
    const one: Node = { leaf: 1 };
    expect(move(one, 1, 1, "left")).toBe(one);
    expect(move(two, 1, 1, "right")).toBe(two);
    expect(move(two, 9, 1, "left")).toBe(two);
    expect(move(two, 1, 9, "left")).toBe(two);
    expect(move(one, 1, 2, "left")).toBe(one);
  });
  it("does not change the tree when the pane already sits there", () => {
    expect(shapeOf(move(two, 2, 1, "right"))).toEqual(shapeOf(two));
    expect(leaves(move(two, 2, 1, "right"))).toEqual([1, 2]);
  });
  it("survives the saved-session round trip", () => {
    const moved = move(grid, 3, 1, "left");
    let n = 20;
    const back = fromShape(JSON.parse(JSON.stringify(shapeOf(moved))), () => ++n)!;
    expect(shapeOf(back)).toEqual(shapeOf(moved));
    expect(layout(back).panes.map((p) => p.rect)).toEqual(layout(moved).panes.map((p) => p.rect));
  });
});

describe("edgeAt", () => {
  it("picks the nearest edge", () => {
    expect(edgeAt(0.05, 0.5)).toBe("left");
    expect(edgeAt(0.95, 0.5)).toBe("right");
    expect(edgeAt(0.5, 0.1)).toBe("up");
    expect(edgeAt(0.5, 0.9)).toBe("down");
  });
  it("resolves the centre without throwing", () => {
    expect(["left", "right", "up", "down"]).toContain(edgeAt(0.5, 0.5));
  });
});

describe("zoomView", () => {
  const r = { x: 0.5, y: 0, w: 0.5, h: 1 };
  it("leaves rectangles alone when nothing is zoomed", () => {
    expect(zoomView(r, 2, null)).toEqual({ rect: r, hidden: false });
  });
  it("fills the tab for the zoomed pane and only hides the others", () => {
    expect(zoomView(r, 2, 2)).toEqual({ rect: { x: 0, y: 0, w: 1, h: 1 }, hidden: false });
    expect(zoomView(r, 1, 2)).toEqual({ rect: r, hidden: true });
  });
});

describe("drop target", () => {
  const l = layout(two); // 1 on the left half, 2 on the right
  it("finds the pane and edge under the pointer", () => {
    expect(dropTarget(l, 0.9, 0.5, 1)).toEqual({ id: 2, edge: "right" });
    expect(dropTarget(l, 0.55, 0.5, 1)).toEqual({ id: 2, edge: "left" });
    expect(dropTarget(l, 0.75, 0.05, 1)).toEqual({ id: 2, edge: "up" });
  });
  it("ignores the dragged pane itself and points outside", () => {
    expect(dropTarget(l, 0.2, 0.5, 1)).toBeNull();
    expect(dropTarget(l, 1.5, 0.5, 1)).toBeNull();
  });
  it("draws the half that the moved pane would take", () => {
    const r = { x: 0.5, y: 0, w: 0.5, h: 1 };
    expect(edgeRect(r, "left")).toEqual({ x: 0.5, y: 0, w: 0.25, h: 1 });
    expect(edgeRect(r, "right")).toEqual({ x: 0.75, y: 0, w: 0.25, h: 1 });
    expect(edgeRect(r, "up")).toEqual({ x: 0.5, y: 0, w: 0.5, h: 0.5 });
    expect(edgeRect(r, "down")).toEqual({ x: 0.5, y: 0.5, w: 0.5, h: 0.5 });
  });
});
