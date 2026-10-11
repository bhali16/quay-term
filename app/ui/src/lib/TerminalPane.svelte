<script lang="ts">
  import { onMount } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import { openLocalSession, sessionWrite, sessionResize, sessionClose } from "./ipc";

  let { active, onexit }: { active: boolean; onexit: () => void } = $props();

  let host: HTMLDivElement;
  let term: Terminal;
  let fit: FitAddon;
  let id: number | null = null;

  export function sessionId() {
    return id;
  }
  export function notifyExited() {
    term?.writeln("\r\n[process exited]");
  }

  // Refit and focus when this tab becomes visible.
  $effect(() => {
    if (active && fit) {
      requestAnimationFrame(() => {
        fit.fit();
        term.focus();
      });
    }
  });

  onMount(() => {
    term = new Terminal({ fontSize: 14, cursorBlink: true });
    fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    fit.fit();

    const encoder = new TextEncoder();
    term.onData((d) => id !== null && sessionWrite(id, encoder.encode(d)));
    term.onBinary((d) => id !== null && sessionWrite(id, Uint8Array.from(d, (c) => c.charCodeAt(0))));
    term.onResize(({ cols, rows }) => id !== null && sessionResize(id, cols, rows));

    openLocalSession(term.cols, term.rows, (bytes) => term.write(bytes))
      .then((sid) => (id = sid))
      .catch((e) => term.writeln(`Failed to start shell: ${e}`));

    const ro = new ResizeObserver(() => active && fit.fit());
    ro.observe(host);

    return () => {
      ro.disconnect();
      if (id !== null) sessionClose(id);
      term.dispose();
    };
  });
</script>

<div class="term" class:hidden={!active} bind:this={host}></div>

<style>
  .term {
    position: absolute;
    inset: 0;
    padding: 6px;
  }
  .hidden {
    visibility: hidden;
  }
</style>
