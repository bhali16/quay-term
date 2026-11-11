import { describe, expect, it } from "vitest";
import { onEscape, top } from "./escape";

describe("escape stack", () => {
  it("gives the key to the dialog opened last", () => {
    const calls: string[] = [];
    const a = onEscape(() => calls.push("a"));
    const b = onEscape(() => calls.push("b"));
    top()?.handler();
    b();
    top()?.handler();
    a();
    expect(calls).toEqual(["b", "a"]);
    expect(top()).toBeUndefined();
  });
  it("keeps a higher level on top of dialogs opened after it", () => {
    const calls: string[] = [];
    const prompt = onEscape(() => calls.push("prompt"), 1);
    const settings = onEscape(() => calls.push("settings"));
    top()?.handler();
    prompt();
    top()?.handler();
    settings();
    expect(calls).toEqual(["prompt", "settings"]);
  });
  it("unregistering twice is harmless", () => {
    const off = onEscape(() => {});
    off();
    off();
    expect(top()).toBeUndefined();
  });
});
