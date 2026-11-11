import { describe, expect, it } from "vitest";
import { EDGE_RATIO, TEXT_RATIO, chromeVars, contrast, inkOn, legible, readableOn, themes } from "./themes";

describe("contrast", () => {
  it("is 21 for black on white and 1 for a colour on itself", () => {
    expect(contrast("#000000", "#ffffff")).toBeCloseTo(21, 5);
    expect(contrast("#3b6fd8", "#3b6fd8")).toBe(1);
  });
});

describe("readableOn", () => {
  it("picks the side with more contrast", () => {
    expect(readableOn("#ffffff")).toBe("#16171b");
    expect(readableOn("#000000")).toBe("#ffffff");
    expect(readableOn("#d98e04")).toBe("#16171b");
  });
  it("falls back to white for a CSS variable", () => {
    expect(readableOn("var(--accent)")).toBe("#ffffff");
  });
  it("uses the theme's own choice for a session without a fixed colour", () => {
    expect(inkOn(null)).toBe("var(--on-accent)");
    expect(inkOn("var(--accent)")).toBe("var(--on-accent)");
    expect(inkOn("#d98e04")).toBe("#16171b");
  });
});

describe("legible", () => {
  it("leaves a colour that already passes alone", () => {
    expect(legible("#e6e6e6", ["#1e1f24"], TEXT_RATIO, true)).toBe("#e6e6e6");
  });
  it("lightens on a dark theme and darkens on a light one", () => {
    const onDark = legible("#dc322f", ["#0b3a47"], TEXT_RATIO, true);
    const onLight = legible("#268bd2", ["#ffffff"], TEXT_RATIO, false);
    expect(contrast(onDark, "#0b3a47")).toBeGreaterThanOrEqual(TEXT_RATIO);
    expect(contrast(onLight, "#ffffff")).toBeGreaterThanOrEqual(TEXT_RATIO);
  });
});

// A new theme only has to list its palette: these are the pairs the chrome relies on.
describe.each(themes)("$name", (t) => {
  const v = chromeVars(t);
  const fields = ["bg", "bg-alt", "bg-deep", "surface", "input"];
  const backgrounds = [...fields, "hover"];

  it.each(["fg", "dim", "accent-text", "red-text", "green-text", "yellow-text"])("%s is readable as text on every background", (name) => {
    for (const bg of backgrounds) expect(contrast(v[name], v[bg]), `${name} on ${bg}`).toBeGreaterThanOrEqual(TEXT_RATIO);
  });
  it.each(["border-strong", "focus"])("%s is visible around a control on every background", (name) => {
    for (const bg of fields) expect(contrast(v[name], v[bg]), `${name} on ${bg}`).toBeGreaterThanOrEqual(EDGE_RATIO);
  });
  it("text on the accent and red fills is readable", () => {
    expect(contrast(v["on-accent"], v.accent)).toBeGreaterThanOrEqual(TEXT_RATIO);
    expect(contrast(v["on-red"], v.red)).toBeGreaterThanOrEqual(TEXT_RATIO);
  });
  it("secondary text stays quieter than body text", () => {
    expect(contrast(v.dim, v.bg)).toBeLessThanOrEqual(contrast(v.fg, v.bg));
  });
});
