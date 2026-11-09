import { describe, expect, it } from "vitest";
import { byShortName, expansionAt, fill, group, initialValues, matches, slug, toInput, variables } from "./snippets";

const s = (id: number, name: string, folder = "", body = "ls") => ({ id, name, folder, body, defaults: {} });

describe("variables", () => {
  it("lists each once, in order, with defaults", () => {
    expect(variables("scp {{file}} {{ host:prod1 }}:/tmp && echo {{file}}")).toEqual([
      { name: "file", fallback: "", choices: [] },
      { name: "host", fallback: "prod1", choices: [] },
    ]);
  });
  it("ignores things that are not variables", () => {
    expect(variables("echo ${HOME} {{ }} {{1x}} {x}")).toEqual([]);
  });
});

describe("defaults and choices", () => {
  it("reads | as a default and several | as a choice list", () => {
    expect(variables("{{a|x}} {{b|dev|staging|prod}} {{c|}}")).toEqual([
      { name: "a", fallback: "x", choices: [] },
      { name: "b", fallback: "dev", choices: ["dev", "staging", "prod"] },
      { name: "c", fallback: "", choices: [] },
    ]);
  });
  it("keeps everything after : as the default, as older snippets expect", () => {
    expect(variables("{{f:grep x | wc -l}}")).toEqual([{ name: "f", fallback: "grep x | wc -l", choices: [] }]);
    expect(fill("{{f:grep x | wc -l}}", {})).toBe("grep x | wc -l");
  });
  it("fills a choice variable with its first option by default", () => {
    expect(fill("deploy {{env|dev|prod}}", {})).toBe("deploy dev");
    expect(fill("deploy {{env|dev|prod}}", { env: "prod" })).toBe("deploy prod");
  });
  it("prefers remembered values over defaults, ignoring unknown names", () => {
    expect(initialValues("{{a|1}} {{b}}", { a: "9", zz: "x" })).toEqual({ a: "9", b: "" });
  });
});

describe("escaped braces", () => {
  it("turns \\{{ into a literal {{ and does not ask for it", () => {
    expect(variables("echo \\{{x}} {{y}}")).toEqual([{ name: "y", fallback: "", choices: [] }]);
    expect(fill("echo \\{{x}} {{y}}", { y: "1" })).toBe("echo {{x}} 1");
  });
});

describe("short names", () => {
  const list = [s(1, "Restart nginx"), s(2, "Restart php"), s(3, "Disk usage")];
  it("slugs names", () => expect(slug("  Restart  Nginx! ")).toBe("restart-nginx"));
  it("finds an exact short name or a unique prefix", () => {
    expect(byShortName(list, "restart-php")?.id).toBe(2);
    expect(byShortName(list, "disk")?.id).toBe(3);
    expect(byShortName(list, "restart")).toBeNull();
    expect(byShortName(list, "")).toBeNull();
  });
  it("finds ;name at the end of the typed line only", () => {
    expect(expansionAt("ls ;disk")).toBe("disk");
    expect(expansionAt(";disk")).toBe("disk");
    expect(expansionAt("echo a;disk")).toBeNull();
    expect(expansionAt("ls ;disk now")).toBeNull();
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
  it("wraps several lines as a bracketed paste when not pressing Enter", () => {
    expect(toInput("a\nb", false, true)).toBe("\x1b[200~a\rb\x1b[201~");
    expect(toInput("a", false, true)).toBe("a");
    expect(toInput("a\nb", true, true)).toBe("a\rb\r");
    expect(toInput("a\x1b[201~\nb", false, true)).toBe("\x1b[200~a\rb\x1b[201~");
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
