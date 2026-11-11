/// <reference types="vite/client" />
import { describe, expect, it } from "vitest";

const raw = import.meta.glob<string>("../**/*.{svelte,css}", { query: "?raw", import: "default", eager: true });

describe("sizes", () => {
  // Radii and font sizes come from the tokens in app.css, so the UI keeps to one scale.
  it("use the radius and text tokens, not raw px", () => {
    const bad = Object.entries(raw).flatMap(([p, text]) =>
      [...text.matchAll(/^\s*(?:border-radius|font-size|font):[^;]*\dpx[^;]*;/gm)].map((m) => `${p}: ${m[0].trim()}`),
    );
    expect(bad).toEqual([]);
  });

  // Gaps, padding and margins sit on the 4px step. 1px to 3px nudges may stay raw.
  it("use the spacing tokens, not raw px", () => {
    const bad = Object.entries(raw).flatMap(([p, text]) =>
      [...text.matchAll(/^\s*(?:(?:row-|column-)?gap|padding[a-z-]*|margin[a-z-]*):[^;]*;/gm)]
        .filter((m) => [...m[0].matchAll(/(\d+(?:\.\d+)?)px/g)].some((n) => Number(n[1]) > 3))
        .map((m) => `${p}: ${m[0].trim()}`),
    );
    expect(bad).toEqual([]);
  });
});
