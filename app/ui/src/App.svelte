<script lang="ts">
  import { onMount } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import { getAppInfo } from "./lib/ipc";
  import type { AppInfo } from "./lib/bindings/AppInfo";

  let host: HTMLDivElement;
  let info = $state<AppInfo | null>(null);

  onMount(() => {
    const term = new Terminal({ fontSize: 14, cursorBlink: true });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    fit.fit();

    const onResize = () => fit.fit();
    window.addEventListener("resize", onResize);

    getAppInfo()
      .then((i) => {
        info = i;
        term.writeln(`${i.name} ${i.version} (${i.os})`);
        term.writeln("Local shell and SSH arrive in M1 and M2.");
      })
      .catch((e) => term.writeln(`Failed to reach backend: ${e}`));

    return () => {
      window.removeEventListener("resize", onResize);
      term.dispose();
    };
  });
</script>

<main>
  <header>{info ? `${info.name} v${info.version}` : "quay-term"}</header>
  <div class="term" bind:this={host}></div>
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  header {
    padding: 6px 12px;
    font-size: 13px;
    background: #16171b;
  }
  .term {
    flex: 1;
    min-height: 0;
    padding: 6px;
  }
</style>
