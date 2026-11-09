import { describe, expect, it } from "vitest";
import { parseQuery, rank, score } from "./fuzzy";

describe("score", () => {
  it("needs the letters in order", () => {
    expect(score("abc", "a-b-c")).not.toBeNull();
    expect(score("cba", "abc")).toBeNull();
  });
  it("ignores case and spaces in the query", () => {
    expect(score("PROD ssh", "prod-ssh")).not.toBeNull();
  });
  it("scores an empty query as a match", () => {
    expect(score("", "anything")).toBe(0);
  });
  it("prefers word starts and contiguous matches", () => {
    expect(score("ps", "prod-ssh")!).toBeGreaterThan(score("ps", "wasp-xs")!);
    expect(score("ssh", "ssh server")!).toBeGreaterThan(score("ssh", "s-s-h server")!);
  });
});

describe("rank", () => {
  const names = ["staging-db", "prod-ssh", "dev box", "prod-db"];
  const fields = (s: string) => [s];
  it("keeps the original order for an empty query", () => {
    expect(rank(names, "", fields)).toEqual(names);
  });
  it("puts the best match first and drops non-matches", () => {
    const r = rank(names, "prod ssh", fields);
    expect(r[0]).toBe("prod-ssh");
    expect(r).not.toContain("dev box");
  });
  it("returns nothing when nothing matches", () => {
    expect(rank(names, "zzz", fields)).toEqual([]);
  });
  it("matches on any field", () => {
    const items = [{ n: "web", host: "10.0.0.5" }, { n: "db", host: "10.0.0.9" }];
    expect(rank(items, "0.9", (i) => [i.n, i.host])).toEqual([items[1]]);
  });
});

describe("parseQuery", () => {
  it("reads a mode prefix", () => {
    expect(parseQuery("@ prod")).toEqual({ mode: "@", query: "prod" });
    expect(parseQuery(">split")).toEqual({ mode: ">", query: "split" });
    expect(parseQuery("$ rsync")).toEqual({ mode: "$", query: "rsync" });
  });
  it("has no mode for plain text", () => {
    expect(parseQuery("prod")).toEqual({ mode: null, query: "prod" });
  });
});
