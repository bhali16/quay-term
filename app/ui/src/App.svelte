<script lang="ts">
  import { onMount } from "svelte";
  import { getAppInfo, onHostKeyPrompt, onSessionExit, sshTrustResponse } from "./lib/ipc";
  import TerminalPane from "./lib/TerminalPane.svelte";
  import ConnectDialog from "./lib/ConnectDialog.svelte";
  import TrustDialog from "./lib/TrustDialog.svelte";
  import type { AppInfo } from "./lib/bindings/AppInfo";
  import type { HostKeyPrompt } from "./lib/bindings/HostKeyPrompt";
  import type { SshTarget } from "./lib/bindings/SshTarget";

  type Tab = { key: number; title: string; ssh: SshTarget | null; pane?: TerminalPane };

  let info = $state<AppInfo | null>(null);
  let tabs = $state<Tab[]>([]);
  let activeKey = $state(0);
  let counter = 0;
  let editing = $state<number | null>(null);
  let connecting = $state(false);
  let trustPrompt = $state<HostKeyPrompt | null>(null);

  function newTab(ssh: SshTarget | null = null) {
    const key = ++counter;
    tabs.push({ key, title: ssh ? `${ssh.user}@${ssh.host}` : `Shell ${key}`, ssh });
    activeKey = key;
  }

  function connectSsh(target: SshTarget) {
    connecting = false;
    newTab(target);
  }

  function answerTrust(trust: boolean) {
    if (trustPrompt) sshTrustResponse(trustPrompt.prompt_id, trust);
    trustPrompt = null;
  }

  function closeTab(key: number) {
    const i = tabs.findIndex((t) => t.key === key);
    if (i < 0) return;
    tabs.splice(i, 1);
    if (activeKey === key) activeKey = tabs[Math.min(i, tabs.length - 1)]?.key ?? 0;
    if (tabs.length === 0) newTab();
  }

  onMount(() => {
    getAppInfo().then((i) => (info = i));
    newTab();
    const unlisten = [
      onSessionExit(({ id, reason }) => {
        const tab = tabs.find((t) => t.pane?.sessionId() === id);
        tab?.pane?.notifyExited(reason);
      }),
      onHostKeyPrompt((p) => (trustPrompt = p)),
    ];
    return () => {
      unlisten.forEach((u) => u.then((f) => f()));
    };
  });
</script>

<main>
  <nav>
    {#each tabs as tab (tab.key)}
      <div class="tab" class:active={tab.key === activeKey}>
        {#if editing === tab.key}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            autofocus
            bind:value={tab.title}
            onblur={() => (editing = null)}
            onkeydown={(e) => (e.key === "Enter" || e.key === "Escape") && (editing = null)}
          />
        {:else}
          <button class="label" onclick={() => (activeKey = tab.key)} ondblclick={() => (editing = tab.key)}>
            {tab.title}
          </button>
        {/if}
        <button class="close" aria-label="Close tab" onclick={() => closeTab(tab.key)}>×</button>
      </div>
    {/each}
    <button class="add" aria-label="New local tab" title="New local shell" onclick={() => newTab()}>+</button>
    <button class="add ssh" title="New SSH connection" onclick={() => (connecting = true)}>SSH</button>
    <span class="ver">{info ? `${info.name} v${info.version}` : ""}</span>
  </nav>
  <div class="panes">
    {#each tabs as tab (tab.key)}
      <TerminalPane bind:this={tab.pane} active={tab.key === activeKey} ssh={tab.ssh} />
    {/each}
  </div>
  {#if connecting}
    <ConnectDialog onconnect={connectSsh} oncancel={() => (connecting = false)} />
  {/if}
  {#if trustPrompt}
    <TrustDialog prompt={trustPrompt} onanswer={answerTrust} />
  {/if}
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  nav {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px 6px 0;
    background: #16171b;
    font-size: 13px;
  }
  .tab {
    display: flex;
    align-items: center;
    border-radius: 6px 6px 0 0;
    background: #202127;
  }
  .tab.active {
    background: #2b2d36;
  }
  button,
  input {
    font: inherit;
    color: inherit;
    background: none;
    border: 0;
  }
  .label {
    padding: 6px 4px 6px 10px;
    cursor: pointer;
  }
  input {
    width: 90px;
    margin: 4px 4px 4px 8px;
    padding: 2px 4px;
    background: #111;
  }
  .close,
  .add {
    padding: 4px 8px;
    cursor: pointer;
    opacity: 0.6;
  }
  .close:hover,
  .add:hover {
    opacity: 1;
  }
  .ssh {
    font-size: 12px;
  }
  .ver {
    margin-left: auto;
    padding-right: 6px;
    opacity: 0.5;
  }
  .panes {
    position: relative;
    flex: 1;
    min-height: 0;
  }
</style>
