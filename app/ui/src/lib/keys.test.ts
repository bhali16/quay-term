import { describe as suite, expect, it } from "vitest";
import { actionFor, defaultBindings, describe, normalize, resolve } from "./keys";

const ev = (init: Partial<KeyboardEvent>) => init as KeyboardEvent;

suite("normalize", () => {
  it("canonicalises case and modifier order", () => {
    expect(normalize("shift+cmd+t")).toBe("Shift+Cmd+T");
    expect(normalize(" ctrl + tab ")).toBe("Ctrl+tab");
  });
  it("rejects an empty shortcut", () => {
    expect(normalize("")).toBe("");
    expect(normalize("+ +")).toBe("");
  });
});

suite("describe", () => {
  it("ignores a bare modifier", () => {
    expect(describe(ev({ key: "Shift", code: "ShiftLeft", shiftKey: true }))).toBeNull();
  });
  it("names letters by physical key, so Shift does not change the name", () => {
    expect(describe(ev({ key: "T", code: "KeyT", ctrlKey: true, shiftKey: true }))).toBe("Ctrl+Shift+T");
  });
  it("names punctuation by code", () => {
    expect(describe(ev({ key: "+", code: "Equal", metaKey: true, shiftKey: true }))).toBe("Shift+Cmd+=");
  });
});

suite("resolve and actionFor", () => {
  it("overrides replace defaults, the rest keep theirs", () => {
    const b = resolve({ new_tab: "alt+n" });
    expect(b.new_tab).toBe("Alt+N");
    expect(b.close_tab).toBe(defaultBindings.close_tab);
  });
  it("finds the action for a key event", () => {
    const b = resolve({ new_tab: "alt+n" });
    expect(actionFor(ev({ key: "n", code: "KeyN", altKey: true }), b)).toBe("new_tab");
    expect(actionFor(ev({ key: "q", code: "KeyQ" }), b)).toBeNull();
  });
});
