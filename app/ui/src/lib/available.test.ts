import { describe, expect, it } from "vitest";
import { pushRecent, unavailable } from "./available";

const local = { remote: false, panes: 1, tabs: 1 };
const ssh = { remote: true, panes: 2, tabs: 3 };

describe("unavailable", () => {
  it("explains panels that need an SSH tab", () => {
    expect(unavailable("toggle_files", local)).toMatch(/SSH tab/);
    expect(unavailable("toggle_tunnels", local)).toMatch(/SSH tab/);
    expect(unavailable("toggle_files", ssh)).toBeNull();
    expect(unavailable("toggle_tunnels", ssh)).toBeNull();
  });

  it("explains pane actions in a tab with one pane", () => {
    for (const a of ["zoom_pane", "focus_left", "resize_down"] as const) {
      expect(unavailable(a, local)).toMatch(/one pane/);
      expect(unavailable(a, ssh)).toBeNull();
    }
  });

  it("explains tab switching with one tab", () => {
    expect(unavailable("next_tab", local)).toMatch(/one tab/);
    expect(unavailable("prev_tab", ssh)).toBeNull();
  });

  it("lets everything else run", () => {
    expect(unavailable("new_tab", local)).toBeNull();
    expect(unavailable("split_right", local)).toBeNull();
    expect(unavailable("palette", local)).toBeNull();
  });
});

describe("pushRecent", () => {
  it("moves an id to the front without repeating it", () => {
    expect(pushRecent([1, 2, 3], 2, 5)).toEqual([2, 1, 3]);
  });
  it("keeps at most max", () => {
    expect(pushRecent([1, 2, 3], 4, 3)).toEqual([4, 1, 2]);
  });
});
