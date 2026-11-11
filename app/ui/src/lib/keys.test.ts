import { describe as suite, expect, it } from "vitest";
import { actionFor, actionGroups, actionLabels, defaultBindings, describe, formatShortcut, normalize, resolve, UNBOUND } from "./keys";

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

suite("an action without a shortcut", () => {
  it("has no binding and matches no key", () => {
    const b = resolve({ new_tab: UNBOUND, close_tab: defaultBindings.new_tab });
    expect(b.new_tab).toBe("");
    expect(formatShortcut(b.new_tab)).toBe("");
    const combo = defaultBindings.new_tab.split("+");
    const key = combo[combo.length - 1];
    const e = ev({ key: key.toLowerCase(), code: `Key${key}`, metaKey: combo.includes("Cmd"), ctrlKey: combo.includes("Ctrl"), shiftKey: combo.includes("Shift"), altKey: combo.includes("Alt") });
    expect(actionFor(e, b)).toBe("close_tab");
  });
});

suite("shortcut groups", () => {
  it("list every action exactly once", () => {
    const listed = actionGroups.flatMap((g) => g.actions).sort();
    expect(listed).toEqual(Object.keys(actionLabels).sort());
  });
});

suite("default bindings", () => {
  it("are written in canonical modifier order, or they could never match", () => {
    for (const [a, combo] of Object.entries(defaultBindings)) expect(normalize(combo), a).toBe(combo);
  });
  it("are unique", () => {
    const all = Object.values(defaultBindings);
    expect(new Set(all).size).toBe(all.length);
  });
});

suite("formatShortcut", () => {
  it("uses symbols on macOS, modifiers first", () => {
    expect(formatShortcut("Alt+Shift+Cmd+M", true)).toBe("⌥⇧⌘M");
    expect(formatShortcut("Ctrl+Cmd+ArrowLeft", true)).toBe("⌃⌘←");
    expect(formatShortcut("Shift+Cmd+Enter", true)).toBe("⇧⌘↩");
    expect(formatShortcut("Cmd+,", true)).toBe("⌘,");
  });
  it("uses words elsewhere", () => {
    expect(formatShortcut("Ctrl+Alt+Shift+ArrowLeft", false)).toBe("Ctrl+Alt+Shift+Left");
    expect(formatShortcut("Ctrl+Shift+P", false)).toBe("Ctrl+Shift+P");
    expect(formatShortcut("Shift+Delete", false)).toBe("Shift+Del");
  });
  it("keeps a plus key and an empty binding", () => {
    expect(formatShortcut("Ctrl++", false)).toBe("Ctrl++");
    expect(formatShortcut("Cmd++", true)).toBe("⌘+");
    expect(formatShortcut("", true)).toBe("");
  });
});
