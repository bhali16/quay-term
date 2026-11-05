<script lang="ts">
  import type { JumpSecret } from "./bindings/JumpSecret";
  import { onMount } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebLinksAddon } from "@xterm/addon-web-links";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import "@xterm/xterm/css/xterm.css";
  import {
    openLocalSession,
    openSavedSession,
    openSshSession,
    sessionWrite,
    sessionResize,
    sessionClose,
  } from "./ipc";
  import type { SshTarget } from "./bindings/SshTarget";
  import { settings } from "./settings.svelte";
  import { actionFor, isMac } from "./keys";
  import { focus } from "./focus";
  import { BlockTracker, parseExit, type Block } from "./blocks";
  import { REMOTE_SNIPPET } from "./shellIntegration";
  import type { IMarker } from "@xterm/xterm";

  export type PaneStatus = "connecting" | "connected" | "ended";

  // `ssh` is kept in memory so Enter can reconnect after a drop; nothing is persisted.
  // A saved session connects by id; its stored secret never leaves Rust.
  type Saved = { id: number; label: string; secret?: string | null; remember?: boolean; jumps?: JumpSecret[] };
  let {
    active,
    ssh = null,
    saved = null,
    onconnected,
    focused = true,
    compact = false,
    onfocus,
    oninput,
    broadcasting = false,
    env = null,
    color = null,
    confirmPaste = false,
    onstatus,
    onactivity,
    onfiles,
  }: {
    active: boolean;
    ssh?: SshTarget | null;
    saved?: Saved | null;
    onconnected?: () => void;
    focused?: boolean;
    compact?: boolean;
    onfocus?: () => void;
    /** Text typed or pasted here, for the app to mirror to other panes. */
    oninput?: (data: string) => void;
    broadcasting?: boolean;
    /** Environment label (prod, staging...) shown as a badge. */
    env?: string | null;
    /** Accent colour of the saved session, drawn as a thin strip along the top. */
    color?: string | null;
    /** Ask before pasting several lines (production sessions). */
    confirmPaste?: boolean;
    onstatus?: (s: PaneStatus) => void;
    /** Output arrived while this pane was in a background tab (reported once per visit). */
    onactivity?: () => void;
    onfiles?: () => void;
  } = $props();
  const remote = $derived(ssh !== null || saved !== null);

  let host: HTMLDivElement;
  let term: Terminal;
  let fit: FitAddon;
  let id: number | null = null;
  let ended = $state(false);
  let hintDismissed = $state(false);
  let pendingPaste = $state<string | null>(null);
  let noisy = false;
  let lastExit = $state<string | null>(null);
  const tracker = new BlockTracker<IMarker>(1000);
  let seenMarks = false;

  export function sessionId() {
    return id;
  }
  /** Type into this pane from outside (a snippet, or another pane's broadcast). */
  export function send(text: string) {
    if (id !== null) sessionWrite(id, new TextEncoder().encode(text));
  }
  export function notifyExited(reason: string | null) {
    ended = true;
    hintDismissed = false;
    pendingPaste = null;
    id = null;
    lastExit = reason;
    onstatus?.("ended");
    if (reason) term?.writeln(`\r\n\x1b[31m${reason}\x1b[0m`);
    term?.writeln(remote ? "\r\n[connection closed - press Enter to reconnect]" : "\r\n[process exited - press Enter to restart]");
  }
  /** Start again in this pane (a new login for SSH, a new shell for a local pane). */
  export function reconnect() {
    if (ended) start();
  }
  export const isEnded = () => ended;
  export const focusTerminal = () => term?.focus();

  async function writeClipboard(text: string) {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // Some webviews refuse the async API outside a gesture; fall back to execCommand.
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.style.cssText = "position:fixed;opacity:0;pointer-events:none";
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      ta.remove();
      term?.focus();
    }
  }
  function copySelection() {
    const text = term.getSelection();
    if (text) void writeClipboard(text);
  }
  /** Several lines pasted into a production session wait for a confirmation first. */
  const needsConfirm = (text: string) => confirmPaste && /[\r\n]/.test(text.replace(/[\r\n]+$/, ""));
  function paste(text: string) {
    if (needsConfirm(text)) {
      pendingPaste = text;
      return;
    }
    term.paste(text);
  }
  async function pasteClipboard() {
    try {
      const text = await navigator.clipboard.readText();
      if (text) paste(text);
    } catch {
      /* clipboard unreadable; nothing to paste */
    }
  }
  function answerPaste(ok: boolean) {
    const text = pendingPaste;
    pendingPaste = null;
    if (ok && text) term.paste(text);
    term.focus();
  }

  // ---- command blocks (shell integration marks) ----
  const nowMs = () => Date.now();
  function cursorAbs() {
    const b = term.buffer.active;
    return b.baseY + b.cursorY;
  }
  /** Text of buffer lines [from, to), starting at column `col` on the first; wrapped rows are joined. */
  function readRange(from: number, col: number, to: number): string {
    const buf = term.buffer.active;
    const rows: string[] = [];
    for (let y = from; y < to; y++) {
      const line = buf.getLine(y);
      if (!line) break;
      const trim = !buf.getLine(y + 1)?.isWrapped;
      const text = line.translateToString(trim, y === from ? col : 0);
      if (line.isWrapped && rows.length) rows[rows.length - 1] += text;
      else rows.push(text);
    }
    return rows.join("\n");
  }
  function commandText(b: Block<IMarker>): string {
    if (!b.input || !b.out || b.input.isDisposed || b.out.isDisposed) return "";
    return readRange(b.input.line, b.inputCol, b.out.line).replace(/\s+$/, "").trim();
  }
  function outputRange(b: Block<IMarker>): [number, number] | null {
    if (!b.out || b.out.isDisposed) return null;
    let end = b.end && !b.end.isDisposed ? b.end.line : cursorAbs();
    // The last row is the prompt of the next command; drop blank rows at the tail.
    while (end > b.out.line && readRange(end - 1, 0, end).trim() === "") end--;
    return [b.out.line, end];
  }
  /** OSC 133 handler. A fault here must never stop the terminal drawing, so it is contained. */
  function onMark(data: string): boolean {
    try {
      handleMark(data);
    } catch (e) {
      console.warn("shell integration mark ignored", e);
    }
    return true;
  }
  function handleMark(data: string) {
    // Marks from the alternate screen (vim, less) would point into a buffer that is thrown away.
    if (term.buffer.active.type === "alternate") return;
    const [kind, arg] = data.split(";");
    const m = () => term.registerMarker(0);
    switch (kind) {
      case "A": {
        const mk = m();
        if (mk) {
          tracker.promptStart(mk);
          seenMarks = true;
        }
        break;
      }
      case "B": {
        const mk = m();
        if (mk) tracker.promptEnd(mk, term.buffer.active.cursorX);
        break;
      }
      case "C": {
        const mk = m();
        const b = mk && tracker.commandStart(mk, nowMs());
        if (b) b.command = commandText(b);
        break;
      }
      case "D": {
        const mk = m();
        const b = mk && tracker.commandEnd(mk, parseExit(arg), nowMs());
        if (b) decorate(b);
        break;
      }
    }
  }
  function decorate(b: Block<IMarker>) {
    const start = b.prompt ?? b.out;
    if (!settings.value.block_gutter || !start || !b.end || start.isDisposed) return;
    const height = b.end.line - start.line;
    if (height < 1) return;
    const ok = b.exit === null || b.exit === 0;
    const deco = term.registerDecoration({ marker: start, x: 0, width: 1, height });
    deco?.onRender((el) => {
      el.style.width = "3px";
      el.style.marginLeft = compact ? "-8px" : "-14px";
      el.style.borderRadius = "2px";
      el.style.background = ok ? settings.theme.ui.green : settings.theme.ui.red;
      el.style.opacity = ok ? "0.45" : "0.9";
      el.title = ok ? "" : `exit code ${b.exit}`;
      el.style.pointerEvents = "none";
    });
  }
  /** True when the pane has produced any prompt marks, so block features have something to work with. */
  export const hasMarks = () => seenMarks;
  export function prevPrompt() {
    // At the bottom the reference is the live prompt, so the first press goes to the last command.
    const buf = term.buffer.active;
    const line = tracker.prevPrompt(buf.viewportY >= buf.baseY ? cursorAbs() : buf.viewportY);
    if (line === null) return false;
    term.scrollToLine(line);
    return true;
  }
  export function nextPrompt() {
    if (!seenMarks) return false;
    const next = tracker.nextPrompt(term.buffer.active.viewportY);
    if (next === null) term.scrollToBottom();
    else term.scrollToLine(next);
    return true;
  }
  /** Copy the output of the last finished command; returns its line count, or null if there is none. */
  export function copyLastOutput(): number | null {
    const b = tracker.lastFinished();
    const r = b && outputRange(b);
    if (!r || r[1] <= r[0]) return b ? 0 : null;
    void writeClipboard(readRange(r[0], 0, r[1]));
    return r[1] - r[0];
  }
  export function copyLastCommand(): string | null {
    const b = tracker.lastFinished();
    if (!b || !b.command) return null;
    void writeClipboard(b.command);
    return b.command;
  }
  /** Select the last command's output on screen. */
  export function selectLastOutput(): boolean {
    const b = tracker.lastFinished();
    const r = b && outputRange(b);
    if (!r || r[1] <= r[0]) return false;
    term.selectLines(r[0], r[1] - 1);
    return true;
  }
  /** Type the last command at the prompt without pressing Enter. */
  export function retypeLast(): string | null {
    const b = tracker.lastFinished();
    if (!b?.command || id === null) return null;
    send(b.command);
    return b.command;
  }
  /** Type the one-line hook into a remote bash or zsh. Only called when the user asks for it. */
  export function enableIntegration() {
    if (id !== null) send(REMOTE_SNIPPET);
  }
  export const lastExitCode = () => tracker.lastFinished()?.exit ?? null;

  function start() {
    ended = false;
    onstatus?.("connecting");
    const out = (bytes: Uint8Array) => {
      term.write(bytes);
      if (!active && !noisy) {
        noisy = true;
        onactivity?.();
      }
    };
    if (ssh) term.writeln(`Connecting to ${ssh.user}@${ssh.host}:${ssh.port}...`);
    if (saved) term.writeln(`Connecting to ${saved.label}...`);
    const opening = saved
      ? openSavedSession(saved.id, saved.secret ?? null, saved.remember ?? false, saved.jumps ?? [], term.cols, term.rows, out)
      : ssh
        ? openSshSession(ssh, term.cols, term.rows, out)
        : openLocalSession(term.cols, term.rows, settings.value.shell_integration, out);
    opening
      .then((sid) => {
        id = sid;
        onstatus?.("connected");
        onconnected?.();
      })
      .catch((e) => {
        ended = true;
        hintDismissed = false;
        lastExit = `${e}`;
        onstatus?.("ended");
        term.writeln(`\r\n\x1b[31m${e}\x1b[0m`);
        if (remote) term.writeln("\r\n[press Enter to try again]");
      });
  }

  // Live-apply settings; a font change alters the cell size, so refit afterwards.
  $effect(() => {
    const v = settings.value;
    const theme = settings.theme.term;
    if (!term) return;
    term.options.fontFamily = v.font_family;
    term.options.fontSize = v.font_size;
    term.options.cursorBlink = v.cursor_blink;
    term.options.scrollback = v.scrollback;
    term.options.theme = theme;
    if (active) requestAnimationFrame(() => fit.fit());
  });

  // Refit when this tab becomes visible; focus only the pane that has the tab's focus.
  $effect(() => {
    if (active && fit) {
      const wantFocus = focused;
      requestAnimationFrame(() => {
        fit.fit();
        if (wantFocus) term.focus();
      });
    }
  });

  $effect(() => {
    if (active) noisy = false;
  });

  onMount(() => {
    const v = settings.value;
    term = new Terminal({
      fontFamily: v.font_family,
      fontSize: v.font_size,
      lineHeight: 1.2,
      minimumContrastRatio: 4.5,
      // Needed for decorations (the gutter bars beside finished commands).
      allowProposedApi: true,
      macOptionClickForcesSelection: true,
      rightClickSelectsWord: false,
      cursorBlink: v.cursor_blink,
      scrollback: v.scrollback,
      theme: settings.theme.term,
    });
    // App shortcuts must not reach the shell; returning false stops xterm handling the key.
    term.attachCustomKeyEventHandler((e) => {
      if (e.type !== "keydown") return true;
      if (actionFor(e, settings.bindings)) return false;
      const k = e.key.toLowerCase();
      // Copy: Ctrl/Cmd+C copies a selection (and clears it); with none, Ctrl+C is the shell's interrupt.
      // Ctrl+Shift+C always copies, never interrupts.
      if (k === "c" && !e.altKey && (e.ctrlKey || e.metaKey)) {
        if (term.hasSelection() || (e.shiftKey && !isMac)) {
          copySelection();
          term.clearSelection();
          return false;
        }
        return true;
      }
      // Ctrl+Shift+V pastes (plain Ctrl+V is handled by xterm's own paste event).
      if (k === "v" && e.ctrlKey && e.shiftKey && !e.altKey) {
        pasteClipboard();
        return false;
      }
      return true;
    });
    fit = new FitAddon();
    term.loadAddon(fit);
    // Ctrl/Cmd+click opens http(s) links in the system browser; a plain click still selects.
    term.loadAddon(
      new WebLinksAddon((e, uri) => {
        if (!(isMac ? e.metaKey : e.ctrlKey)) return;
        if (/^https?:\/\//i.test(uri)) openUrl(uri).catch(() => {});
      }),
    );
    term.open(host);
    fit.fit();
    term.parser.registerOscHandler(133, onMark);
    // Capture phase, so this runs before xterm's own paste handler on the textarea.
    const onPaste = (e: ClipboardEvent) => {
      const text = e.clipboardData?.getData("text");
      if (text && needsConfirm(text)) {
        e.preventDefault();
        e.stopPropagation();
        pendingPaste = text;
      }
    };
    host.addEventListener("paste", onPaste, true);
    term.textarea?.addEventListener("focus", () => onfocus?.());

    // Copy on select: wait for the mouse to be released so a drag copies once, not per cell.
    const onMouseUp = () => {
      if (settings.value.copy_on_select && term.hasSelection()) copySelection();
    };
    host.addEventListener("mouseup", onMouseUp);
    // Right-click pastes on Windows/Linux (PuTTY-style); macOS keeps the native context menu.
    const onContext = (e: MouseEvent) => {
      if (isMac) return;
      e.preventDefault();
      if (term.hasSelection()) {
        copySelection();
        term.clearSelection();
      } else pasteClipboard();
    };
    host.addEventListener("contextmenu", onContext);

    const encoder = new TextEncoder();
    term.onData((d) => {
      if (id !== null) {
        sessionWrite(id, encoder.encode(d));
        oninput?.(d);
      }
      else if (ended && (d === "\r" || d === "r" || d === "R")) start();
    });
    term.onBinary((d) => id !== null && sessionWrite(id, Uint8Array.from(d, (c) => c.charCodeAt(0))));
    term.onResize(({ cols, rows }) => id !== null && sessionResize(id, cols, rows));

    start();

    const ro = new ResizeObserver(() => active && fit.fit());
    ro.observe(host);

    return () => {
      ro.disconnect();
      host.removeEventListener("mouseup", onMouseUp);
      host.removeEventListener("contextmenu", onContext);
      host.removeEventListener("paste", onPaste, true);
      if (id !== null) sessionClose(id);
      term.dispose();
    };
  });
</script>

<div class="term" class:hidden={!active} class:compact bind:this={host}></div>
{#if active}
  {#if color}<span class="strip" style:background={color}></span>{/if}
  <div class="badges">
    {#if env}<span class="env" class:prod={/^prod/i.test(env)} style:--c={color ?? "var(--accent)"} title="Environment: {env}">{env}</span>{/if}
    {#if broadcasting}<span class="bcast" title="What you type here is also sent to the other selected panes">Broadcast</span>{/if}
  </div>
  {#if pendingPaste !== null}
    <div class="bar confirm" role="alertdialog" tabindex="-1" aria-label="Confirm paste" onkeydown={(e) => e.key === "Escape" && answerPaste(false)}>
      <span class="msg">Paste {pendingPaste.replace(/[\r\n]+$/, "").split(/\r\n|\r|\n/).length} lines into this {env ?? "production"} session?</span>
      <button use:focus class="go" onclick={() => answerPaste(true)}>Paste</button>
      <button onclick={() => answerPaste(false)}>Cancel</button>
    </div>
  {:else if ended && settings.value.show_hints && !hintDismissed}
    <div class="bar" role="status">
      <span class="msg">{remote ? "Disconnected" : "Shell exited"}{lastExit ? `: ${lastExit}` : ""}</span>
      <button class="go" onclick={() => (reconnect(), term.focus())}>{remote ? "Reconnect" : "Restart"} <kbd>R</kbd></button>
      {#if remote && onfiles}<button onclick={onfiles}>Files</button>{/if}
      <button aria-label="Dismiss" onclick={() => ((hintDismissed = true), term.focus())}>Dismiss</button>
    </div>
  {/if}
{/if}

<style>
  .term {
    position: absolute;
    inset: 0;
    /* content-box on purpose: FitAddon reads this element's computed width and
       does not subtract its padding, so border-box would overflow the window. */
    padding: 20px 24px;
    background: var(--bg);
  }
  .badges {
    position: absolute;
    top: 5px;
    right: 14px;
    z-index: 1;
    display: flex;
    gap: 6px;
    pointer-events: none;
  }
  .strip {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 2px;
    z-index: 1;
    pointer-events: none;
  }
  .env {
    padding: 1px 7px;
    border-radius: 4px;
    background: var(--c);
    color: var(--on-accent, #fff);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .env.prod {
    background: var(--red);
    color: var(--bg);
  }
  .bar {
    position: absolute;
    left: 12px;
    right: 12px;
    bottom: 10px;
    z-index: 3;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.3);
    font-size: 13px;
  }
  .bar.confirm {
    border-color: var(--red);
  }
  .bar .msg {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bar button {
    padding: 3px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--hover);
    color: var(--fg);
    font: inherit;
    cursor: pointer;
  }
  .bar button.go {
    border-color: var(--accent);
  }
  .bar kbd {
    margin-left: 4px;
    padding: 0 4px;
    border-radius: 3px;
    background: var(--bg);
    font: inherit;
    font-size: 11px;
    color: var(--dim);
  }
  .bcast {
    padding: 1px 7px;
    border-radius: 4px;
    background: var(--red);
    color: var(--bg);
    font-size: 11px;
    font-weight: 600;
  }
  .term.compact {
    padding: 8px 10px;
  }
  .hidden {
    visibility: hidden;
  }
</style>
