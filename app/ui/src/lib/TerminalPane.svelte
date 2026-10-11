<script lang="ts">
  import { onMount } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import { openLocalSession, openSshSession, sessionWrite, sessionResize, sessionClose } from "./ipc";
  import type { SshTarget } from "./bindings/SshTarget";

  // `ssh` is kept in memory so Enter can reconnect after a drop; nothing is persisted.
  let { active, ssh = null }: { active: boolean; ssh?: SshTarget | null } = $props();

  let host: HTMLDivElement;
  let term: Terminal;
  let fit: FitAddon;
  let id: number | null = null;
  let ended = false;

  export function sessionId() {
    return id;
  }
  export function notifyExited(reason: string | null) {
    ended = true;
    id = null;
    if (reason) term?.writeln(`\r\n\x1b[31m${reason}\x1b[0m`);
    term?.writeln(ssh ? "\r\n[connection closed - press Enter to reconnect]" : "\r\n[process exited]");
  }

  function start() {
    ended = false;
    const out = (bytes: Uint8Array) => term.write(bytes);
    if (ssh) term.writeln(`Connecting to ${ssh.user}@${ssh.host}:${ssh.port}...`);
    (ssh ? openSshSession(ssh, term.cols, term.rows, out) : openLocalSession(term.cols, term.rows, out))
      .then((sid) => (id = sid))
      .catch((e) => {
        ended = true;
        term.writeln(`\r\n\x1b[31m${e}\x1b[0m`);
        if (ssh) term.writeln("\r\n[press Enter to try again]");
      });
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
    term.onData((d) => {
      if (id !== null) sessionWrite(id, encoder.encode(d));
      else if (ssh && ended && d === "\r") start();
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
