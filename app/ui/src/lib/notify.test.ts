import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/plugin-notification", () => ({ isPermissionGranted: vi.fn(), requestPermission: vi.fn() }));

import { commandBody, formatDuration, isLong, shouldNotify } from "./notify";

const on = { enabled: true, windowFocused: false, tabActive: true, muted: false };

describe("shouldNotify", () => {
  it("alerts when the window is in the background", () => {
    expect(shouldNotify(on)).toBe(true);
  });
  it("alerts for a background tab even when the window is focused", () => {
    expect(shouldNotify({ ...on, windowFocused: true, tabActive: false })).toBe(true);
  });
  it("stays quiet when the user is looking at that tab", () => {
    expect(shouldNotify({ ...on, windowFocused: true, tabActive: true })).toBe(false);
  });
  it("stays quiet when off or muted", () => {
    expect(shouldNotify({ ...on, enabled: false })).toBe(false);
    expect(shouldNotify({ ...on, muted: true })).toBe(false);
  });
});

describe("commandBody", () => {
  const base = { command: "rsync -a src/ host:dst/", exit: 0, ms: 130_000, session: "prod-db", showCommand: true };
  it("follows the documented wording", () => {
    expect(commandBody(base)).toBe("rsync -a src/ host:dst/ finished (exit 0) in 2m 10s on prod-db");
  });
  it("reports failures with the exit code", () => {
    expect(commandBody({ ...base, exit: 2 })).toBe("rsync -a src/ host:dst/ failed (exit 2) in 2m 10s on prod-db");
  });
  it("leaves the exit code out when unknown", () => {
    expect(commandBody({ ...base, exit: null })).toBe("rsync -a src/ host:dst/ finished in 2m 10s on prod-db");
  });
  it("cuts the command at 60 characters and collapses whitespace", () => {
    const body = commandBody({ ...base, command: `echo   ${"x".repeat(100)}` });
    expect(body.startsWith(`echo ${"x".repeat(55)}… finished`)).toBe(true);
  });
  it("can hide the command", () => {
    expect(commandBody({ ...base, showCommand: false })).toBe("Command finished (exit 0) in 2m 10s on prod-db");
    expect(commandBody({ ...base, command: "  " })).toMatch(/^Command finished/);
  });
});

describe("formatDuration and isLong", () => {
  it("formats seconds, minutes and hours", () => {
    expect(formatDuration(42_000)).toBe("42s");
    expect(formatDuration(130_000)).toBe("2m 10s");
    expect(formatDuration(3_900_000)).toBe("1h 5m");
  });
  it("compares against the threshold in seconds", () => {
    expect(isLong(15_000, 15)).toBe(true);
    expect(isLong(14_999, 15)).toBe(false);
  });
});
