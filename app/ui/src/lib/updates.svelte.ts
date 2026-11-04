import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "current" }
  | { kind: "available"; version: string; notes: string | null }
  | { kind: "downloading"; version: string; percent: number | null }
  | { kind: "error"; message: string };

/** Looks for a newer release and installs it on request. Signatures are checked by the updater plugin. */
class Updates {
  state = $state<UpdateState>({ kind: "idle" });
  private pending: Update | null = null;

  /** `quiet` keeps "up to date" and failures out of sight, for the check at launch. */
  async check(quiet = false) {
    if (this.state.kind === "checking" || this.state.kind === "downloading") return;
    this.state = { kind: "checking" };
    try {
      const update = await check();
      if (!update) {
        this.pending = null;
        this.state = { kind: "current" };
        return;
      }
      this.pending = update;
      this.state = { kind: "available", version: update.version, notes: update.body ?? null };
    } catch (e) {
      this.state = quiet ? { kind: "idle" } : { kind: "error", message: `${e}` };
    }
  }

  async install() {
    const update = this.pending;
    if (!update) return;
    let total = 0;
    let done = 0;
    this.state = { kind: "downloading", version: update.version, percent: null };
    try {
      await update.downloadAndInstall((e) => {
        if (e.event === "Started") total = e.data.contentLength ?? 0;
        else if (e.event === "Progress") {
          done += e.data.chunkLength;
          this.state = {
            kind: "downloading",
            version: update.version,
            percent: total ? Math.min(100, Math.round((done / total) * 100)) : null,
          };
        }
      });
      await relaunch();
    } catch (e) {
      this.state = { kind: "error", message: `${e}` };
    }
  }
}

export const updates = new Updates();
