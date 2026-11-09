import { describe, expect, it } from "vitest";
import { DEFAULT_DENY, ago, applyFilter, collapse, compileDeny, hostsOf, parseCwd, rangeStart, recordable } from "./history";
import type { HistoryEntry } from "./bindings/HistoryEntry";

const deny = compileDeny(DEFAULT_DENY);
const e = (id: number, command: string, host = "local", ts = id * 1000, exit: number | null = 0): HistoryEntry => ({
  id,
  ts,
  command,
  host,
  session_id: null,
  cwd: null,
  exit,
});

describe("recordable", () => {
  it("keeps ordinary commands, trimmed", () => {
    expect(recordable("rsync -av ./site web1:/srv  ", deny)).toBe("rsync -av ./site web1:/srv");
    expect(recordable("git push --force-with-lease", deny)).toBe("git push --force-with-lease");
    expect(recordable("kubectl get pods -n kube-system", deny)).toBe("kubectl get pods -n kube-system");
  });
  it("never saves commands typed with a leading space", () => {
    expect(recordable(" ls", deny)).toBeNull();
    expect(recordable("\tls", deny)).toBeNull();
    expect(recordable("   ", deny)).toBeNull();
    expect(recordable("", deny)).toBeNull();
  });
  it.each([
    "mysql --password=hunter2 db",
    "curl -u me:x 'https://h/?password=abc'",
    "gh auth login --token ghp_123",
    "deploy --api-token abc",
    "curl -H 'Authorization: Bearer abc' https://api",
    "AWS_SECRET_ACCESS_KEY=abc aws s3 ls",
    "export GITHUB_TOKEN=abc",
    "export OPENAI_API_KEY=sk-1",
    "export db_passwd=x",
    "env SECRET=abc ./run",
    "API_KEY=abc ./run",
  ])("drops %s", (cmd) => {
    expect(recordable(cmd, deny)).toBeNull();
  });
  it("does not drop commands that only mention the words", () => {
    expect(recordable("cat tokens.txt", deny)).not.toBeNull();
    expect(recordable("export PATH=$PATH:/opt/bin", deny)).not.toBeNull();
    expect(recordable("ssh-keygen -t ed25519", deny)).not.toBeNull();
  });
  it("honours a custom list and reports bad patterns", () => {
    const d = compileDeny(["^vault ", "([", "  "]);
    expect(d.invalid).toEqual(["(["]);
    expect(d.regexes).toHaveLength(1);
    expect(recordable("vault read secret/x", d)).toBeNull();
    expect(recordable("VAULT read x", d)).toBeNull();
    expect(recordable("export TOKEN=x", d)).toBe("export TOKEN=x");
  });
});

describe("filtering and grouping", () => {
  const now = new Date(2026, 10, 10, 15, 0).getTime();
  const day = 24 * 3600 * 1000;
  const list = [
    e(5, "make", "local", now - 1000, 2),
    e(4, "ls", "web1", now - 2 * day),
    e(3, "make", "web1", now - 3 * day, 0),
    e(2, "ls", "local", now - 10 * day),
    e(1, "make", "local", now - 20 * day, 0),
  ];
  it("groups by command, newest run first, with count and hosts", () => {
    const rows = collapse(list);
    expect(rows.map((r) => r.entry.command)).toEqual(["make", "ls"]);
    expect(rows[0]).toMatchObject({ count: 3, hosts: ["local", "web1"] });
    expect(rows[0].entry.id).toBe(5);
  });
  it("filters by host, failure and time", () => {
    expect(applyFilter(list, { host: "web1", failed: false, range: "all" }, now).map((x) => x.id)).toEqual([4, 3]);
    expect(applyFilter(list, { host: null, failed: true, range: "all" }, now).map((x) => x.id)).toEqual([5]);
    expect(applyFilter(list, { host: null, failed: false, range: "today" }, now).map((x) => x.id)).toEqual([5]);
    expect(applyFilter(list, { host: null, failed: false, range: "week" }, now).map((x) => x.id)).toEqual([5, 4, 3]);
  });
  it("an unknown exit code is not a failure", () => {
    expect(applyFilter([e(1, "x", "local", now, null)], { host: null, failed: true, range: "all" }, now)).toEqual([]);
  });
  it("today starts at local midnight", () => {
    expect(rangeStart("today", now)).toBe(new Date(2026, 10, 10).getTime());
    expect(rangeStart("all", now)).toBe(-Infinity);
  });
  it("lists hosts most recent first", () => {
    expect(hostsOf(list)).toEqual(["local", "web1"]);
  });
});

describe("parseCwd", () => {
  it("reads OSC 7 file URLs", () => {
    expect(parseCwd("file://mac.local/Users/me/my%20dir")).toBe("/Users/me/my dir");
    expect(parseCwd("file:///srv/app")).toBe("/srv/app");
    expect(parseCwd("file://host/raw path 100%")).toBe("/raw path 100%");
  });
  it("ignores anything else", () => {
    expect(parseCwd("http://x/y")).toBeNull();
    expect(parseCwd("garbage")).toBeNull();
  });
});

describe("ago", () => {
  const now = Date.UTC(2026, 10, 10, 12);
  it("reads naturally", () => {
    expect(ago(now - 5000, now)).toBe("just now");
    expect(ago(now - 5 * 60_000, now)).toBe("5m ago");
    expect(ago(now - 3 * 3600_000, now)).toBe("3h ago");
    expect(ago(now - 30 * 3600_000, now)).toBe("yesterday");
    expect(ago(now - 3 * 86400_000, now)).toBe("3d ago");
  });
});
