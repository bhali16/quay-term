import { describe, expect, it } from "vitest";
import { runningTunnels, tabState } from "./tabs";

describe("tabState", () => {
  it("is ended only when every pane ended", () => {
    expect(tabState(["ended", "ended"], true)).toBe("ended");
    expect(tabState(["ended", "connected"], true)).toBe("connected");
    expect(tabState(["ended"], false)).toBe("ended");
  });
  it("is connecting while a pane has not connected or reported", () => {
    expect(tabState(["connecting"], true)).toBe("connecting");
    expect(tabState([undefined], true)).toBe("connecting");
    expect(tabState(["connected", "connecting"], true)).toBe("connecting");
  });
  it("is connected once all panes are", () => {
    expect(tabState(["connected", "connected"], true)).toBe("connected");
  });
  it("is local for local shells that are alive", () => {
    expect(tabState(["connected"], false)).toBe("local");
    expect(tabState(["connecting"], false)).toBe("local");
  });
  it("treats a tab with no panes as local or connected, not ended", () => {
    expect(tabState([], false)).toBe("local");
    expect(tabState([], true)).toBe("connected");
  });
});

describe("runningTunnels", () => {
  it("counts tunnels without an error", () => {
    expect(runningTunnels([])).toBe(0);
    expect(runningTunnels([{ error: null }, { error: "port in use" }, { error: null }])).toBe(2);
  });
});
