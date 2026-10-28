import { describe, expect, it } from "vitest";
import { fromShape, layout, leaves, neighbor, nudge, remove, setRatio, shapeOf, split, type Node } from "./panes";

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
