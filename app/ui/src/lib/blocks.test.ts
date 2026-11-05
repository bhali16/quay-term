import { describe, expect, it } from "vitest";
import { BlockTracker, duration, parseExit, type Mark } from "./blocks";

const mk = (line: number, isDisposed = false): Mark => ({ line, isDisposed });

function run(t: BlockTracker, at: number, exit = 0) {
  t.promptStart(mk(at));
  t.promptEnd(mk(at), 2);
  t.commandStart(mk(at + 1), 1000);
  t.commandEnd(mk(at + 3), exit, 3500);
}

describe("BlockTracker", () => {
  it("builds a block from A B C D", () => {
    const t = new BlockTracker();
    run(t, 10, 2);
    const b = t.lastFinished()!;
    expect(b.exit).toBe(2);
    expect(b.out!.line).toBe(11);
    expect(b.end!.line).toBe(13);
    expect(b.inputCol).toBe(2);
    expect(b.endedAt! - b.startedAt!).toBe(2500);
  });

  it("ignores D with no running command (first prompt, empty Enter)", () => {
    const t = new BlockTracker();
    expect(t.commandEnd(mk(0), 0, 1)).toBeNull();
    t.promptStart(mk(1));
    expect(t.commandEnd(mk(2), 0, 1)).toBeNull();
    expect(t.ran()).toHaveLength(0);
  });

  it("replaces a prompt that never ran a command", () => {
    const t = new BlockTracker();
    t.promptStart(mk(1));
    t.promptStart(mk(2));
    expect(t.blocks).toHaveLength(1);
    expect(t.blocks[0].prompt!.line).toBe(2);
  });

  it("ignores a second C while a command runs", () => {
    const t = new BlockTracker();
    t.promptStart(mk(1));
    t.commandStart(mk(2), 1);
    expect(t.commandStart(mk(3), 2)).toBeNull();
    expect(t.blocks).toHaveLength(1);
  });

  it("handles C with no earlier prompt mark", () => {
    const t = new BlockTracker();
    t.commandStart(mk(5), 1);
    t.commandEnd(mk(7), 0, 2);
    expect(t.lastFinished()!.out!.line).toBe(5);
  });

  it("keeps at most `max` blocks", () => {
    const t = new BlockTracker(3);
    for (let i = 0; i < 6; i++) run(t, i * 10);
    expect(t.blocks).toHaveLength(3);
    expect(t.blocks[0].prompt!.line).toBe(30);
  });

  it("drops blocks scrolled out of the buffer", () => {
    const t = new BlockTracker();
    t.promptStart(mk(1, true));
    t.commandStart(mk(2, true), 1);
    t.commandEnd(mk(3, true), 0, 2);
    run(t, 20);
    expect(t.ran()).toHaveLength(1);
    expect(t.ran()[0].prompt!.line).toBe(20);
  });

  it("finds the previous and next prompt", () => {
    const t = new BlockTracker();
    run(t, 10);
    run(t, 20);
    run(t, 30);
    expect(t.prevPrompt(25)).toBe(20);
    expect(t.prevPrompt(20)).toBe(10);
    expect(t.prevPrompt(10)).toBeNull();
    expect(t.nextPrompt(20)).toBe(30);
    expect(t.nextPrompt(30)).toBeNull();
  });

  it("finds the block containing a line", () => {
    const t = new BlockTracker();
    run(t, 10);
    run(t, 20);
    expect(t.at(11, 40)!.prompt!.line).toBe(10);
    expect(t.at(19, 40)).toBeUndefined();
    expect(t.at(22, 40)!.prompt!.line).toBe(20);
  });
});

describe("helpers", () => {
  it("formats durations", () => {
    expect(duration(400)).toBe("400ms");
    expect(duration(5000)).toBe("5s");
    expect(duration(125000)).toBe("2m 05s");
  });
  it("parses the exit code", () => {
    expect(parseExit("0")).toBe(0);
    expect(parseExit("127")).toBe(127);
    expect(parseExit("")).toBeNull();
    expect(parseExit(undefined)).toBeNull();
    expect(parseExit("x")).toBeNull();
  });
});
