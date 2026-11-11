import { describe, expect, it } from "vitest";
import type { Entry } from "./bindings/Entry";
import { crumbs, durationText, isHidden, nextRate, nextSort, progressText, sortEntries } from "./files";

const e = (name: string, kind: Entry["kind"], size = 0, modified: number | null = null): Entry => ({ name, path: `/x/${name}`, kind, size, modified, mode: null });
const names = (list: Entry[]) => list.map((x) => x.name);

describe("sortEntries", () => {
  const list = [e("b.txt", "file", 10, 300), e("a10", "dir", 0, 100), e("a2", "dir", 0, 200), e("A.txt", "file", 500, 50), e("c", "file", 10, null)];

  it("puts folders first and sorts names as people read them", () => {
    expect(names(sortEntries(list, { key: "name", asc: true }))).toEqual(["a2", "a10", "A.txt", "b.txt", "c"]);
    expect(names(sortEntries(list, { key: "name", asc: false }))).toEqual(["a10", "a2", "c", "b.txt", "A.txt"]);
  });
  it("sorts files by size, with the name deciding a tie", () => {
    expect(names(sortEntries(list, { key: "size", asc: false }))).toEqual(["a2", "a10", "A.txt", "b.txt", "c"]);
    expect(names(sortEntries(list, { key: "size", asc: true }))).toEqual(["a2", "a10", "b.txt", "c", "A.txt"]);
  });
  it("sorts by date, an unknown date counting as oldest", () => {
    expect(names(sortEntries(list, { key: "date", asc: false }))).toEqual(["a2", "a10", "b.txt", "A.txt", "c"]);
  });
  it("leaves the list it was given alone", () => {
    const before = names(list);
    sortEntries(list, { key: "size", asc: true });
    expect(names(list)).toEqual(before);
  });
});

describe("nextSort", () => {
  it("turns the order round on the same column", () => {
    expect(nextSort({ key: "name", asc: true }, "name")).toEqual({ key: "name", asc: false });
  });
  it("starts size and date with the largest and newest", () => {
    expect(nextSort({ key: "name", asc: true }, "size")).toEqual({ key: "size", asc: false });
    expect(nextSort({ key: "size", asc: false }, "name")).toEqual({ key: "name", asc: true });
  });
});

describe("crumbs", () => {
  it("lists each folder with its path", () => {
    expect(crumbs("/var/log")).toEqual([
      { name: "/", path: "/" },
      { name: "var", path: "/var" },
      { name: "log", path: "/var/log" },
    ]);
    expect(crumbs("/")).toEqual([{ name: "/", path: "/" }]);
  });
});

describe("isHidden", () => {
  it("is a leading dot", () => {
    expect(isHidden(".ssh")).toBe(true);
    expect(isHidden("a.b")).toBe(false);
  });
});

describe("transfer progress", () => {
  it("measures speed over at least a quarter of a second and smooths it", () => {
    const first = nextRate(undefined, 0, 0);
    expect(first.bps).toBe(0);
    expect(nextRate(first, 100, 5000)).toBe(first);
    const second = nextRate(first, 1000, 1000);
    expect(second.bps).toBe(1000);
    const third = nextRate(second, 2000, 4000);
    expect(third.bps).toBeCloseTo(1000 * 0.6 + 3000 * 0.4);
  });
  it("says percent, speed and time left when they are known", () => {
    expect(progressText(0, 1000, 0)).toBe("0%");
    expect(progressText(512 * 1024, 1024 * 1024, 64 * 1024)).toBe("50% · 64.0 KB/s · 8 s left");
    expect(progressText(2048, 0, 0)).toBe("2.0 KB");
  });
  it("words durations", () => {
    expect(durationText(0.2)).toBe("1 s");
    expect(durationText(150)).toBe("3 min");
    expect(durationText(4800)).toBe("1 h 20 min");
  });
});
