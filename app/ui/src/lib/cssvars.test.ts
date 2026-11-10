/// <reference types="vite/client" />
import { describe, expect, it } from "vitest";
import { chromeVars, themes } from "./themes";

// Every .svelte, .css and .ts file under src, as text.
const raw = import.meta.glob<string>("../**/*.{svelte,css,ts}", { query: "?raw", import: "default", eager: true });

describe("CSS variables", () => {
  // A var(--name) that nothing defines silently takes its fallback (or nothing), in every theme.
  it("are all defined by a theme or in a stylesheet", () => {
    const files = Object.entries(raw)
      .filter(([p]) => !p.endsWith(".test.ts") && !p.includes("/bindings/"))
      .map(([p, text]) => ({ p, text }));
    expect(files.length).toBeGreaterThan(20);
    const defined = new Set(Object.keys(chromeVars(themes[0])));
    for (const { text } of files) {
      // "--name: value" in a stylesheet or style attribute, and Svelte's style:--name={...}.
      for (const m of text.matchAll(/(?:^|[\s;{"'`])--([\w-]+)\s*:/g)) defined.add(m[1]);
      for (const m of text.matchAll(/style:--([\w-]+)/g)) defined.add(m[1]);
    }
    const unknown = files.flatMap(({ p, text }) =>
      [...text.matchAll(/var\(\s*--([\w-]+)/g)].filter((m) => !defined.has(m[1])).map((m) => `${p}: --${m[1]}`),
    );
    expect(unknown).toEqual([]);
  });
});
