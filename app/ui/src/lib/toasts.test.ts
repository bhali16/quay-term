import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { durationFor, plainError } from "./toasts";
import { toasts } from "./toastStore.svelte";

describe("plainError", () => {
  it("leaves a message written for people alone", () => {
    const msg = 'Authentication failed: the server rejected the password for "root".';
    expect(plainError(msg)).toEqual({ text: msg });
    expect(plainError(new Error(msg))).toEqual({ text: msg });
  });
  it("maps raw failures to a sentence and keeps the original", () => {
    expect(plainError("i/o error: connection timed out")).toEqual({
      text: "The server did not answer in time. Check the host, the port and your network.",
      detail: "i/o error: connection timed out",
    });
    expect(plainError("session closed").text).toBe("The connection was closed by the other side.");
    expect(plainError("session database error: disk I/O").detail).toBe("session database error: disk I/O");
  });
  it("tidies anything else", () => {
    expect(plainError("Error: could not write settings: disk full")).toEqual({ text: "Could not write settings: disk full." });
    expect(plainError("")).toEqual({ text: "Something went wrong." });
  });
  it("cuts a long message to its first sentence", () => {
    const long = `Could not connect to web-01:22. ${"More words about it. ".repeat(12)}`.trim();
    const r = plainError(long);
    expect(r.text).toBe("Could not connect to web-01:22.");
    expect(r.detail).toBe(long);
  });
});

describe("durationFor", () => {
  it("keeps errors until dismissed", () => expect(durationFor("error", "x", false)).toBeNull());
  it("gives a longer message more time, within limits", () => {
    expect(durationFor("info", "Short.", false)).toBe(4500);
    expect(durationFor("info", "x".repeat(100), false)).toBe(6000);
    expect(durationFor("info", "x".repeat(900), false)).toBe(10000);
  });
  it("gives a button at least eight seconds", () => expect(durationFor("info", "Deleted.", true)).toBe(8000));
});

describe("toasts", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    toasts.flush();
    toasts.resume();
    vi.useRealTimers();
  });

  it("times out info and keeps errors", () => {
    toasts.show("Saved.");
    toasts.error("session closed");
    vi.advanceTimersByTime(4500);
    expect(toasts.list.map((t) => t.kind)).toEqual(["error"]);
    vi.advanceTimersByTime(60000);
    expect(toasts.list).toHaveLength(1);
  });

  it("tells onclose whether the button was used", () => {
    const closed: boolean[] = [];
    const run = vi.fn();
    const a = toasts.show("Deleted a.", "info", { action: { label: "Undo", run }, onclose: (acted) => closed.push(acted) });
    toasts.show("Deleted b.", "info", { action: { label: "Undo", run }, onclose: (acted) => closed.push(acted) });
    toasts.act(a);
    expect(run).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(8000);
    expect(closed).toEqual([true, false]);
    expect(toasts.list).toEqual([]);
  });

  it("stops the clock while paused", () => {
    toasts.show("Saved.");
    vi.advanceTimersByTime(4000);
    toasts.pause();
    vi.advanceTimersByTime(20000);
    expect(toasts.list).toHaveLength(1);
    toasts.resume();
    vi.advanceTimersByTime(1499);
    expect(toasts.list).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(toasts.list).toEqual([]);
  });

  it("replaces a repeated message and drops the oldest past the limit", () => {
    toasts.error("session closed");
    toasts.error("session closed");
    expect(toasts.list).toHaveLength(1);
    const closed = vi.fn();
    toasts.show("one", "info", { sticky: true, onclose: closed });
    for (const t of ["two", "three", "four", "five"]) toasts.show(t, "info", { sticky: true });
    expect(toasts.list.map((t) => t.text)).toEqual(["two", "three", "four", "five"]);
    expect(closed).toHaveBeenCalledWith(false);
  });

  it("flush closes everything as not acted on", () => {
    const closed = vi.fn();
    toasts.show("Deleted.", "info", { action: { label: "Undo", run: () => {} }, onclose: closed });
    toasts.flush();
    expect(closed).toHaveBeenCalledWith(false);
  });
});
