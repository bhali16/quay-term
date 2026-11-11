import { durationFor, plainError, type ToastKind } from "./toasts";

export type ToastAction = { label: string; run: () => void };
export type Toast = { id: number; kind: ToastKind; text: string; detail?: string; action?: ToastAction };
export type ToastOptions = {
  /** One button, for example Undo or Open Settings. Using it closes the toast. */
  action?: ToastAction;
  /** Longer text behind a "Details" toggle. */
  detail?: string;
  /** Stay until dismissed, whatever the kind. */
  sticky?: boolean;
  /** Runs once when the toast goes away; `acted` is true when its button was used. */
  onclose?: (acted: boolean) => void;
};

type Timer = { left: number; since: number; handle: ReturnType<typeof setTimeout> | null };

/** At most this many on screen; the oldest gives way. */
const MAX_SHOWN = 4;

/** The toasts on screen. Timers stop while the pointer or keyboard focus is on them. */
class Toasts {
  list = $state<Toast[]>([]);
  private nextId = 0;
  private timers = new Map<number, Timer>();
  private closers = new Map<number, (acted: boolean) => void>();
  private paused = false;

  show(text: string, kind: ToastKind = "info", opts: ToastOptions = {}): number {
    // The same message again replaces the one showing instead of stacking.
    const same = this.list.find((t) => t.kind === kind && t.text === text && !t.action && !opts.action);
    if (same) this.close(same.id);
    const id = ++this.nextId;
    this.list.push({ id, kind, text, detail: opts.detail, action: opts.action });
    if (opts.onclose) this.closers.set(id, opts.onclose);
    const ms = opts.sticky ? null : durationFor(kind, text, !!opts.action);
    if (ms !== null) {
      const timer: Timer = { left: ms, since: Date.now(), handle: null };
      this.timers.set(id, timer);
      if (!this.paused) this.start(id, timer);
    }
    while (this.list.length > MAX_SHOWN) this.close(this.list[0].id);
    return id;
  }

  /** An error, said plainly, with the original text behind "Details". */
  error(e: unknown, opts: ToastOptions = {}) {
    const { text, detail } = plainError(e);
    return this.show(text, "error", { detail, ...opts });
  }

  close(id: number, acted = false) {
    if (!this.list.some((t) => t.id === id)) return;
    this.list = this.list.filter((t) => t.id !== id);
    const timer = this.timers.get(id);
    if (timer?.handle) clearTimeout(timer.handle);
    this.timers.delete(id);
    const done = this.closers.get(id);
    this.closers.delete(id);
    done?.(acted);
  }

  /** Use the toast's button. */
  act(id: number) {
    const t = this.list.find((x) => x.id === id);
    if (!t) return;
    t.action?.run();
    this.close(id, true);
  }

  /** Close everything now, as if each had run out (the app is going away). */
  flush() {
    for (const t of [...this.list]) this.close(t.id);
  }

  pause() {
    if (this.paused) return;
    this.paused = true;
    for (const timer of this.timers.values()) {
      if (!timer.handle) continue;
      clearTimeout(timer.handle);
      timer.handle = null;
      timer.left = Math.max(0, timer.left - (Date.now() - timer.since));
    }
  }

  resume() {
    if (!this.paused) return;
    this.paused = false;
    // Never less than a moment, so a toast does not vanish the instant the pointer leaves.
    for (const [id, timer] of this.timers) this.start(id, { ...timer, left: Math.max(timer.left, 1500) });
  }

  private start(id: number, timer: Timer) {
    timer.since = Date.now();
    timer.handle = setTimeout(() => this.close(id), timer.left);
    this.timers.set(id, timer);
  }
}

export const toasts = new Toasts();
