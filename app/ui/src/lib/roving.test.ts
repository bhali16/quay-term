import { describe, expect, it } from "vitest";
import { step, typeAhead } from "./roving";

describe("step", () => {
  it("moves with Left and Right in a row and wraps", () => {
    expect(step("ArrowRight", 0, 3)).toBe(1);
    expect(step("ArrowRight", 2, 3)).toBe(0);
    expect(step("ArrowLeft", 0, 3)).toBe(2);
  });
  it("uses Up and Down in a column and ignores the other pair", () => {
    expect(step("ArrowDown", 1, 3, true)).toBe(2);
    expect(step("ArrowUp", 0, 3, true)).toBe(2);
    expect(step("ArrowRight", 1, 3, true)).toBeNull();
    expect(step("ArrowDown", 1, 3)).toBeNull();
  });
  it("jumps to the ends", () => {
    expect(step("Home", 2, 4)).toBe(0);
    expect(step("End", 0, 4)).toBe(3);
  });
  it("returns null for other keys and for an empty list", () => {
    expect(step("a", 0, 3)).toBeNull();
    expect(step("ArrowRight", 0, 0)).toBeNull();
  });
});

describe("typeAhead", () => {
  const labels = ["Copy command", "Copy output", " Select output", "Paste"];
  it("finds the next match after the current item", () => {
    expect(typeAhead(labels, 0, "c")).toBe(1);
    expect(typeAhead(labels, 1, "c")).toBe(0);
    expect(typeAhead(labels, 0, "S")).toBe(2);
  });
  it("returns -1 when nothing matches", () => {
    expect(typeAhead(labels, 0, "z")).toBe(-1);
  });
});
