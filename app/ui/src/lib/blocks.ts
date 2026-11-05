/**
 * Command blocks from shell-integration marks (OSC 133):
 *   A prompt starts, B prompt ends (typing begins), C command output starts, D command finished.
 * The tracker is independent of xterm: positions are any object with a `line` and `isDisposed`,
 * which xterm's markers satisfy. A mark that arrives out of order is ignored, never fatal,
 * because the stream comes from a remote program.
 */
export type Mark = { readonly line: number; readonly isDisposed: boolean };

export type Block<M extends Mark = Mark> = {
  id: number;
  prompt: M | null;
  /** Where typing starts (end of the prompt), with its column. */
  input: M | null;
  inputCol: number;
  /** First line of output. */
  out: M | null;
  /** Line after the output (where the next prompt is drawn). */
  end: M | null;
  exit: number | null;
  startedAt: number | null;
  endedAt: number | null;
  /** Command text, read from the screen once the command starts. */
  command: string;
};

export class BlockTracker<M extends Mark = Mark> {
  blocks: Block<M>[] = [];
  private nextId = 1;

  constructor(private max = 1000) {}

  private last(): Block<M> | undefined {
    return this.blocks[this.blocks.length - 1];
  }

  private push(b: Partial<Block<M>>): Block<M> {
    const block: Block<M> = {
      id: this.nextId++,
      prompt: null,
      input: null,
      inputCol: 0,
      out: null,
      end: null,
      exit: null,
      startedAt: null,
      endedAt: null,
      command: "",
      ...b,
    };
    this.blocks.push(block);
    if (this.blocks.length > this.max) this.blocks.splice(0, this.blocks.length - this.max);
    return block;
  }

  /** A: a prompt starts. An earlier prompt that never ran a command is replaced. */
  promptStart(m: M): Block<M> {
    const last = this.last();
    if (last && last.out === null) {
      last.prompt = m;
      last.input = null;
      return last;
    }
    return this.push({ prompt: m });
  }

  /** B: the prompt ended; the cursor column says where the command text begins. */
  promptEnd(m: M, col: number) {
    const last = this.last();
    if (last && last.out === null) {
      last.input = m;
      last.inputCol = col;
    }
  }

  /** C: output starts. Returns the block, or null if a command is already running. */
  commandStart(m: M, now: number): Block<M> | null {
    const last = this.last();
    if (last && last.out !== null && last.end === null) return null;
    const b = last && last.out === null ? last : this.push({});
    b.out = m;
    b.startedAt = now;
    return b;
  }

  /** D: the command finished. Ignored when no command is running (first prompt, empty Enter). */
  commandEnd(m: M, exit: number | null, now: number): Block<M> | null {
    const running = [...this.blocks].reverse().find((b) => b.out !== null && b.end === null);
    if (!running) return null;
    running.end = m;
    running.exit = exit;
    running.endedAt = now;
    return running;
  }

  /** Blocks whose screen position is still in the buffer. */
  alive(): Block<M>[] {
    this.blocks = this.blocks.filter((b) => !(b.out ?? b.prompt)?.isDisposed);
    return this.blocks;
  }

  /** Blocks that have run (not a bare prompt). */
  ran(): Block<M>[] {
    return this.alive().filter((b) => b.out !== null);
  }

  /** The most recent block that finished. */
  lastFinished(): Block<M> | undefined {
    return [...this.ran()].reverse().find((b) => b.end !== null);
  }

  /** Line of the nearest prompt strictly above `line`, or null. */
  prevPrompt(line: number): number | null {
    let best: number | null = null;
    for (const b of this.alive()) {
      const l = (b.prompt ?? b.out)?.line;
      if (l !== undefined && l < line && (best === null || l > best)) best = l;
    }
    return best;
  }

  /** Line of the nearest prompt strictly below `line`, or null. */
  nextPrompt(line: number): number | null {
    let best: number | null = null;
    for (const b of this.alive()) {
      const l = (b.prompt ?? b.out)?.line;
      if (l !== undefined && l > line && (best === null || l < best)) best = l;
    }
    return best;
  }

  /** The block whose prompt-to-end span contains `line`. */
  at(line: number, bottom: number): Block<M> | undefined {
    return this.ran().find((b) => {
      const start = (b.prompt ?? b.out)!.line;
      const stop = b.end ? b.end.line : bottom + 1;
      return line >= start && line < stop;
    });
  }
}

/** "1m 05s" style duration. */
export function duration(ms: number): string {
  const s = Math.round(ms / 1000);
  if (s < 1) return `${ms}ms`;
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  return `${m}m ${String(s % 60).padStart(2, "0")}s`;
}

/** Parse the `D;<exit>` parameter; null when absent or not a number. */
export function parseExit(arg: string | undefined): number | null {
  if (arg === undefined || arg === "") return null;
  const n = Number(arg);
  return Number.isInteger(n) ? n : null;
}
