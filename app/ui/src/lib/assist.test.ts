import { describe, expect, it } from "vitest";
import {
  buildHosts,
  hostContext,
  hostEdit,
  joinPath,
  lastWord,
  matchHosts,
  matchPaths,
  pathContext,
  pathEdit,
  sharedStart,
  shellEscape,
  type HostCandidate,
} from "./assist";

const hosts: HostCandidate[] = buildHosts(
  [
    { alias: "web1", host_name: "10.0.0.5", user: "deploy", port: 22 },
    { alias: "db", host_name: "db", user: null, port: 22 },
  ],
  [
    { name: "web1", host: "10.0.0.5", user: "deploy", port: 22 },
    { name: "Staging box", host: "stage.example.com", user: "ubuntu", port: 2222 },
    { name: "Staging copy", host: "stage.example.com", user: "ubuntu", port: 2222 },
  ],
);

describe("hostContext", () => {
  it("finds the destination of ssh, scp and sftp", () => {
    expect(hostContext("ssh ")).toMatchObject({ tool: "ssh", fragment: "", user: null, jump: false });
    expect(hostContext("ssh we")).toMatchObject({ fragment: "we", word: "we" });
    expect(hostContext("  sftp root@we")).toMatchObject({ tool: "sftp", user: "root", fragment: "we", word: "root@we" });
    expect(hostContext("scp file.txt we")).toMatchObject({ tool: "scp", fragment: "we" });
  });
  it("skips options and their values", () => {
    expect(hostContext("ssh -p 2222 -v we")).toMatchObject({ fragment: "we" });
    expect(hostContext("ssh -vp 2222 we")).toMatchObject({ fragment: "we" });
    expect(hostContext("ssh -p2222 we")).toMatchObject({ fragment: "we" });
    expect(hostContext("ssh -p ")).toBeNull();
    expect(hostContext("ssh -i ~/.ssh/id ")).toMatchObject({ fragment: "" });
  });
  it("offers jump hosts after -J", () => {
    expect(hostContext("ssh -J ba")).toMatchObject({ jump: true, fragment: "ba" });
  });
  it("stays out of the way elsewhere", () => {
    expect(hostContext("ssh")).toBeNull();
    expect(hostContext("ls ")).toBeNull();
    expect(hostContext("sshd ")).toBeNull();
    expect(hostContext("ssh -")).toBeNull();
    expect(hostContext("ssh web1 ")).toBeNull(); // the command, not a host
    expect(hostContext("ssh web1 up")).toBeNull();
    expect(hostContext("scp web1:/tm")).toBeNull();
    expect(hostContext("scp ./fi")).toBeNull();
    expect(hostContext("scp ~/fi")).toBeNull();
    expect(hostContext("ssh '")).toBeNull();
    expect(hostContext("ssh $(echo")).toBeNull();
    expect(hostContext("ssh @we")).toBeNull();
  });
});

describe("buildHosts", () => {
  it("lists config aliases, then saved sessions not already covered", () => {
    expect(hosts.map((h) => [h.label, h.source])).toEqual([
      ["web1", "config"],
      ["db", "config"],
      ["Staging box", "saved"],
    ]);
    expect(hosts[0].detail).toBe("deploy@10.0.0.5");
    expect(hosts[1].detail).toBe("ssh config");
    expect(hosts[2].detail).toBe("ubuntu@stage.example.com:2222");
  });
});

describe("matchHosts", () => {
  it("puts prefix matches before matches inside", () => {
    const ctx = hostContext("ssh st")!;
    expect(matchHosts(ctx, hosts).map((h) => h.label)).toEqual(["Staging box"]);
    // Only "web1" has a 1, and not at the start.
    expect(matchHosts(hostContext("ssh 1")!, hosts).map((h) => h.label)).toEqual(["web1"]);
    // No prefix match: "db" and "Staging box" both have a b inside.
    expect(matchHosts(hostContext("ssh b")!, hosts).map((h) => h.label)).toEqual(["web1", "db", "Staging box"]);
    expect(matchHosts(hostContext("ssh example")!, hosts).map((h) => h.label)).toEqual(["Staging box"]);
  });
  it("shows everything for an empty fragment and nothing once the host is typed in full", () => {
    expect(matchHosts(hostContext("ssh ")!, hosts)).toHaveLength(3);
    expect(matchHosts(hostContext("ssh db")!, hosts)).toEqual([]);
  });
});

describe("hostEdit", () => {
  it("types only what is missing", () => {
    expect(hostEdit(hostContext("ssh we")!, hosts[0])).toEqual({ erase: 0, text: "b1 " });
    expect(hostEdit(hostContext("scp f we")!, hosts[0])).toEqual({ erase: 0, text: "b1:" });
  });
  it("keeps a typed user and replaces a fragment matched in the middle", () => {
    expect(hostEdit(hostContext("ssh root@b")!, hosts[1])).toEqual({ erase: 1, text: "db " });
  });
  it("uses user@host and the port option for a saved session", () => {
    expect(hostEdit(hostContext("ssh st")!, hosts[2])).toEqual({ erase: 2, text: "-p 2222 ubuntu@stage.example.com " });
    expect(hostEdit(hostContext("sftp st")!, hosts[2])).toEqual({ erase: 2, text: "-P 2222 ubuntu@stage.example.com " });
    expect(hostEdit(hostContext("ssh -J st")!, hosts[2])).toEqual({ erase: 2, text: "ubuntu@stage.example.com:2222 " });
  });
});

describe("paths", () => {
  it("reads the last word, unescaping and refusing quotes", () => {
    expect(lastWord("cat my\\ fi")).toBe("my fi");
    expect(lastWord("cat ")).toBe("");
    expect(lastWord("cat 'my")).toBeNull();
    expect(lastWord("cat a\\")).toBeNull();
  });
  it("joins and tidies", () => {
    expect(joinPath("/home/u", "a/./b/../c/")).toBe("/home/u/a/c");
    expect(joinPath("/home/u", "/etc")).toBe("/etc");
    expect(joinPath("/", "..")).toBe("/");
  });
  it("resolves absolute, home and relative paths", () => {
    expect(pathContext("cat /etc/ho", null, "/home/u")).toEqual({ ok: true, ctx: { dir: "/etc", prefix: "ho", lead: "" } });
    expect(pathContext("cat ~/pro", null, "/home/u")).toEqual({ ok: true, ctx: { dir: "/home/u", prefix: "pro", lead: "" } });
    expect(pathContext("cd ~", null, "/home/u")).toEqual({ ok: true, ctx: { dir: "/home/u", prefix: "", lead: "/" } });
    expect(pathContext("vim src/ma", "/srv/app", "/home/u")).toEqual({ ok: true, ctx: { dir: "/srv/app/src", prefix: "ma", lead: "" } });
    expect(pathContext("ls ", "/srv", "/home/u")).toEqual({ ok: true, ctx: { dir: "/srv", prefix: "", lead: "" } });
    expect(pathContext("tar --file=/tmp/a", null, "/h")).toEqual({ ok: true, ctx: { dir: "/tmp", prefix: "a", lead: "" } });
  });
  it("says why it cannot complete", () => {
    expect(pathContext("vim src/ma", null, "/h")).toMatchObject({ ok: false });
    expect(pathContext("scp web1:/tm", "/x", "/h")).toMatchObject({ ok: false });
    expect(pathContext("cat 'a", "/x", "/h")).toMatchObject({ ok: false });
    expect(pathContext("ls ~bob/", "/x", "/h")).toMatchObject({ ok: false });
  });
  it("matches names, hiding dotfiles unless asked", () => {
    const e = [
      { name: ".bashrc", dir: false },
      { name: "bin", dir: true },
      { name: "build.sh", dir: false },
      { name: "..", dir: true },
    ];
    expect(matchPaths("b", e).map((x) => x.name)).toEqual(["bin", "build.sh"]);
    expect(matchPaths("", e).map((x) => x.name)).toEqual(["bin", "build.sh"]);
    expect(matchPaths(".", e).map((x) => x.name)).toEqual([".bashrc"]);
  });
  it("types the rest of the name, escaped, with a slash after a folder", () => {
    const ctx = { dir: "/x", prefix: "my", lead: "" };
    expect(pathEdit(ctx, { name: "my dir", dir: true })).toEqual({ erase: 0, text: "\\ dir/" });
    expect(pathEdit({ ...ctx, prefix: "", lead: "/" }, { name: "a", dir: false })).toEqual({ erase: 0, text: "/a" });
    expect(shellEscape("a$b(c)")).toBe("a\\$b\\(c\\)");
    expect(sharedStart(["build.sh", "builder", "build"])).toBe("build");
    expect(sharedStart([])).toBe("");
  });
});
