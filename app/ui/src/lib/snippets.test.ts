import { describe, expect, it } from "vitest";
import { fill, group, matches, toInput, variables } from "./snippets";

const s = (id: number, name: string, folder = "", body = "ls") => ({ id, name, folder, body });

describe("variables", () => {
  it("lists each once, in order, with defaults", () => {
    expect(variables("scp {{file}} {{ host:prod1 }}:/tmp && echo {{file}}")).toEqual([
      { name: "file", fallback: "" },
      { name: "host", fallback: "prod1" },
    ]);
  });
  it("ignores things that are not variables", () => {
    expect(variables("echo ${HOME} {{ }} {{1x}} {x}")).toEqual([]);
  });
});

describe("fill", () => {
  it("uses values, then defaults, then nothing", () => {
    expect(fill("a {{x}} b {{y:dflt}} c {{z}}", { x: "1" })).toBe("a 1 b dflt c ");
  });
  it("does not expand a value that itself looks like a variable", () => {
    expect(fill("{{a}} {{b}}", { a: "{{b}}", b: "2" })).toBe("{{b}} 2");
  });
});

describe("toInput", () => {
  it("turns newlines into Enter and optionally ends with one", () => {
    expect(toInput("cd /srv\nls\n", true)).toBe("cd /srv\rls\r");
    expect(toInput("cd /srv\r\nls\n\n", false)).toBe("cd /srv\rls");
  });
});

describe("matches and group", () => {
  it("needs every word, in name, folder or body", () => {
    const x = s(1, "Restart", "ops/web", "systemctl restart nginx");
    expect(matches(x, "ops nginx")).toBe(true);
    expect(matches(x, "ops apache")).toBe(false);
    expect(matches(x, "")).toBe(true);
  });
  it("puts unfiled first and sorts the rest", () => {
    const g = group([s(1, "b", "zz"), s(2, "z"), s(3, "a", "aa"), s(4, "A")]);
    expect(g.map((x) => x.folder)).toEqual(["", "aa", "zz"]);
    expect(g[0].snippets.map((x) => x.name)).toEqual(["A", "z"]);
  });
});
