<script lang="ts">
  import type { JumpSecret } from "./bindings/JumpSecret";
  import { onMount } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
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
  import { actionFor } from "./keys";

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
  } = $props();
  const remote = $derived(ssh !== null || saved !== null);

  let host: HTMLDivElement;
  let term: Terminal;
  let fit: FitAddon;
  let id: number | null = null;
  let ended = false;

  export function sessionId() {
    return id;
  }
  /** Type into this pane from outside (a snippet, or another pane's broadcast). */
  export function send(text: string) {
    if (id !== null) sessionWrite(id, new TextEncoder().encode(text));
  }
  export function notifyExited(reason: string | null) {
    ended = true;
    id = null;
    if (reason) term?.writeln(`\r\n\x1b[31m${reason}\x1b[0m`);
    term?.writeln(remote ? "\r\n[connection closed - press Enter to reconnect]" : "\r\n[process exited]");
  }

  function start() {
    ended = false;
    const out = (bytes: Uint8Array) => term.write(bytes);
    if (ssh) term.writeln(`Connecting to ${ssh.user}@${ssh.host}:${ssh.port}...`);
    if (saved) term.writeln(`Connecting to ${saved.label}...`);
    const opening = saved
      ? openSavedSession(saved.id, saved.secret ?? null, saved.remember ?? false, saved.jumps ?? [], term.cols, term.rows, out)
      : ssh
        ? openSshSession(ssh, term.cols, term.rows, out)
        : openLocalSession(term.cols, term.rows, out);
    opening
      .then((sid) => {
        id = sid;
        onconnected?.();
      })
      .catch((e) => {
        ended = true;
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

  onMount(() => {
    const v = settings.value;
    term = new Terminal({
      fontFamily: v.font_family,
      fontSize: v.font_size,
      lineHeight: 1.5,
      cursorBlink: v.cursor_blink,
      scrollback: v.scrollback,
      theme: settings.theme.term,
    });
    // App shortcuts must not reach the shell; returning false stops xterm handling the key.
    term.attachCustomKeyEventHandler((e) => !(e.type === "keydown" && actionFor(e, settings.bindings)));
    fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    fit.fit();
    term.textarea?.addEventListener("focus", () => onfocus?.());

    const encoder = new TextEncoder();
    term.onData((d) => {
      if (id !== null) {
        sessionWrite(id, encoder.encode(d));
        oninput?.(d);
      }
      else if (remote && ended && d === "\r") start();
    });
    term.onBinary((d) => id !== null && sessionWrite(id, Uint8Array.from(d, (c) => c.charCodeAt(0))));
    term.onResize(({ cols, rows }) => id !== null && sessionResize(id, cols, rows));

    start();

    const ro = new ResizeObserver(() => active && fit.fit());
    ro.observe(host);

    return () => {
      ro.disconnect();
      if (id !== null) sessionClose(id);
      term.dispose();
    };
  });
</script>

<div class="term" class:hidden={!active} class:compact bind:this={host}></div>
{#if broadcasting && active}<span class="bcast" title="What you type here is also sent to the other selected panes">Broadcast</span>{/if}

<style>
  .term {
    position: absolute;
    inset: 0;
    /* content-box on purpose: FitAddon reads this element's computed width and
       does not subtract its padding, so border-box would overflow the window. */
    padding: 20px 24px;
    background: var(--bg);
  }
  .bcast {
    position: absolute;
    top: 6px;
    right: 14px;
    z-index: 1;
    padding: 1px 7px;
    border-radius: 4px;
    background: var(--red);
    color: var(--bg);
    font-size: 11px;
    font-weight: 600;
    pointer-events: none;
  }
  .term.compact {
    padding: 8px 10px;
  }
  .hidden {
    visibility: hidden;
  }
</style>
