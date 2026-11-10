import { describe, expect, it } from "vitest";
import { clamp, DOCK, dockFit, SIDEBAR, TERM_MIN } from "./layout";

describe("clamp", () => {
  it("keeps a width inside its range", () => {
    expect(clamp(50, SIDEBAR)).toBe(SIDEBAR.min);
    expect(clamp(9999, SIDEBAR)).toBe(SIDEBAR.max);
    expect(clamp(300.4, SIDEBAR)).toBe(300);
  });
});

describe("dockFit", () => {
  it("sits beside the terminal at the wanted width when there is room", () => {
    expect(dockFit(340, 840)).toEqual({ width: 340, over: false });
  });
  it("gives up width before the terminal drops under 40 columns", () => {
    const fit = dockFit(500, 700);
    expect(fit).toEqual({ width: 700 - TERM_MIN, over: false });
    expect(fit.width).toBeGreaterThanOrEqual(DOCK.min);
  });
  it("covers the terminal when both cannot fit, and stays inside the space", () => {
    for (const space of [340, 552, 600]) {
      const fit = dockFit(340, space);
      expect(fit.over).toBe(true);
      expect(fit.width).toBeLessThan(space);
    }
  });
  it("never squeezes the terminal at any window size", () => {
    for (let space = 300; space <= 1600; space += 20) {
      const fit = dockFit(DOCK.def, space);
      if (!fit.over) expect(space - fit.width).toBeGreaterThanOrEqual(TERM_MIN);
    }
  });
});
