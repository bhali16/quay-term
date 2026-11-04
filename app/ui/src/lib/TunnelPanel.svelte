<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import { tunnelList, tunnelStart, tunnelStop } from "./ipc";
  import type { TunnelInfo } from "./bindings/TunnelInfo";
  import type { TunnelKind } from "./bindings/TunnelKind";
  import type { TunnelSpec } from "./bindings/TunnelSpec";

  let {
    terminalId,
    saved = [],
    active,
    onclose,
  }: {
    /** The terminal tab's session id while it is connected. */
    terminalId: () => number | null;
    /** Tunnels saved with this session, offered when not running. */
    saved?: TunnelSpec[];
    active: boolean;
    onclose: () => void;
  } = $props();

  let running = $state<TunnelInfo[]>([]);
  let error = $state<string | null>(null);
  let busy = $state(false);

  // New tunnel form.
  let kind = $state<TunnelKind>("local");
  let bindPort = $state(8080);
  let destHost = $state("localhost");
  let destPort = $state(80);
  let everyone = $state(false);

  const kinds: { id: TunnelKind; label: string; hint: string }[] = [
    { id: "local", label: "Local", hint: "Open a port here that leads to a host the server can reach" },
    { id: "remote", label: "Remote", hint: "Open a port on the server that leads back to a host you can reach" },
    { id: "dynamic", label: "Dynamic (SOCKS5)", hint: "A proxy on this machine; the app using it picks the destination" },
  ];

  const kindName = (k: TunnelKind) => (k === "dynamic" ? "SOCKS5" : k === "local" ? "Local" : "Remote");
  const bytes = (n: number) =>
    n < 1024 ? `${n} B` : n < 1024 ** 2 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1024 ** 2).toFixed(1)} MB`;

  /** "127.0.0.1:8080 → db:5432", using the port that really opened. */
  function line(t: TunnelInfo): string {
    const s = t.spec;
    const listen = `${s.bind_host}:${t.bound_port || s.bind_port}`;
    if (s.kind === "dynamic") return `${listen}`;
    return s.kind === "remote" ? `server ${listen} → ${s.dest_host}:${s.dest_port}` : `${listen} → ${s.dest_host}:${s.dest_port}`;
  }
  const same = (a: TunnelSpec, b: TunnelSpec) =>
    a.kind === b.kind && a.bind_host === b.bind_host && a.bind_port === b.bind_port && a.dest_host === b.dest_host && a.dest_port === b.dest_port;
  const notRunning = $derived(saved.filter((s) => !running.some((r) => same(r.spec, s) && !r.error)));
  const lan = (host: string) => host !== "127.0.0.1" && host !== "localhost" && host !== "::1";

  async function refresh() {
    const sid = terminalId();
    if (sid === null) {
      running = [];
      return;
    }
    try {
      running = await tunnelList(sid);
    } catch {
      /* the next tick tries again */
    }
  }

  async function start(spec: TunnelSpec) {
    const sid = terminalId();
    if (sid === null) {
      error = "This tab is not connected.";
      return;
    }
    busy = true;
    error = null;
    try {
      await tunnelStart(sid, spec);
      await refresh();
    } catch (e) {
      error = `${e}`;
    } finally {
      busy = false;
    }
  }

  const startNew = () =>
    start({
      kind,
      bind_host: everyone && kind !== "remote" ? "0.0.0.0" : "127.0.0.1",
      bind_port: bindPort,
      dest_host: kind === "dynamic" ? "" : destHost.trim(),
      dest_port: kind === "dynamic" ? 0 : destPort,
      auto_start: false,
    });

  async function stop(t: TunnelInfo) {
    await tunnelStop(t.id);
    await refresh();
  }

  const valid = $derived(
    bindPort >= 0 && bindPort < 65536 && (kind === "dynamic" || (destHost.trim() !== "" && destPort > 0 && destPort < 65536)),
  );

  onMount(() => {
    refresh();
    // Counters move while connections are open, so poll while the panel is showing.
    const timer = setInterval(() => active && refresh(), 1000);
    return () => clearInterval(timer);
  });
</script>

<aside aria-label="Tunnels">
  <header>
    <span class="title"><Icon name="swap" size={16} />Tunnels</span>
    <button class="ibtn" aria-label="Close tunnels" onclick={onclose}><Icon name="x" size={16} /></button>
  </header>

  <div class="body">
    {#if running.length === 0}
      <p class="empty">No tunnels yet. Add one below{saved.length ? ", or start a saved one" : ""}.</p>
    {/if}
    {#each running as t (t.id)}
      <div class="item" class:failed={t.error}>
        <div class="top">
          <span class="badge">{kindName(t.spec.kind)}</span>
          <span class="mono grow">{line(t)}</span>
          <button class="ibtn" aria-label="Stop tunnel" title="Stop" onclick={() => stop(t)}><Icon name="x" size={14} /></button>
        </div>
        {#if t.error}
          <p class="err">{t.error}</p>
        {:else}
          <p class="stats">
            {t.active} open · {t.total} total · <Icon name="arrow-up" size={11} />{bytes(t.bytes_up)}
            <Icon name="arrow-down" size={11} />{bytes(t.bytes_down)}
          </p>
          {#if lan(t.spec.bind_host)}
            <p class="warn">Listening on {t.spec.bind_host}: other devices on the network can use this.</p>
          {/if}
        {/if}
      </div>
    {/each}

    {#if notRunning.length}
      <h3>Saved</h3>
      {#each notRunning as s}
        <div class="item">
          <div class="top">
            <span class="badge">{kindName(s.kind)}</span>
            <span class="mono grow">{s.kind === "dynamic" ? `${s.bind_host}:${s.bind_port}` : `${s.bind_host}:${s.bind_port} → ${s.dest_host}:${s.dest_port}`}</span>
            <button class="ibtn" aria-label="Start saved tunnel" title="Start" disabled={busy} onclick={() => start(s)}><Icon name="play" size={14} /></button>
          </div>
        </div>
      {/each}
    {/if}

    <h3>New tunnel</h3>
    <form onsubmit={(e) => (e.preventDefault(), valid && startNew())}>
      <label>
        Type
        <select bind:value={kind}>
          {#each kinds as k (k.id)}<option value={k.id}>{k.label}</option>{/each}
        </select>
      </label>
      <p class="hint">{kinds.find((k) => k.id === kind)?.hint}</p>
      <label>
        {kind === "remote" ? "Port on the server" : "Port on this computer"}
        <input type="number" min="0" max="65535" bind:value={bindPort} />
      </label>
      {#if kind !== "dynamic"}
        <div class="row">
          <label class="grow">
            {kind === "remote" ? "Send to (from this computer)" : "Send to (from the server)"}
            <input class="mono" bind:value={destHost} autocapitalize="off" spellcheck="false" />
          </label>
          <label class="narrow">Port <input type="number" min="1" max="65535" bind:value={destPort} /></label>
        </div>
      {/if}
      {#if kind !== "remote"}
        <label class="check"><input type="checkbox" bind:checked={everyone} /> Let other devices on the network use it</label>
      {/if}
      {#if error}<p class="err">{error}</p>{/if}
      <button type="submit" class="primary" disabled={!valid || busy}>Start tunnel</button>
    </form>
  </div>
</aside>

<style>
  aside {
    width: clamp(280px, 34vw, 380px);
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--bg-alt);
    border-left: 1px solid var(--border);
    font-size: 13px;
    min-height: 0;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
  }
  .title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
  }
  .body {
    flex: 1;
    overflow: auto;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  h3 {
    margin: 8px 0 0;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--dim);
  }
  .empty,
  .hint {
    margin: 0;
    color: var(--dim);
    line-height: 1.45;
  }
  .item {
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px;
    background: var(--bg);
  }
  .item.failed {
    border-color: var(--danger, #d9534f);
  }
  .top {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .badge {
    flex: none;
    padding: 1px 6px;
    border-radius: 4px;
    font-size: 11px;
    background: var(--hover);
    color: var(--dim);
  }
  .mono {
    font-family: var(--font-mono);
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .stats {
    margin: 6px 0 0;
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--dim);
    font-size: 12px;
  }
  .warn,
  .err {
    margin: 6px 0 0;
    font-size: 12px;
    line-height: 1.4;
  }
  .warn {
    color: var(--warn, #d98e04);
  }
  .err {
    color: var(--danger, #d9534f);
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--dim);
  }
  label.check {
    flex-direction: row;
    align-items: center;
    gap: 6px;
  }
  .row {
    display: flex;
    gap: 8px;
  }
  .narrow {
    width: 84px;
  }
  input,
  select {
    font: inherit;
    color: var(--fg);
    background: var(--input, var(--bg));
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 5px 8px;
  }
  input[type="checkbox"] {
    padding: 0;
  }
  button {
    font: inherit;
    color: inherit;
    background: none;
    border: 0;
    cursor: pointer;
    border-radius: 4px;
  }
  .ibtn {
    display: flex;
    flex: none;
    padding: 4px;
    color: var(--dim);
  }
  .ibtn:hover:not(:disabled) {
    background: var(--hover);
    color: var(--fg);
  }
  .primary {
    align-self: flex-start;
    padding: 6px 12px;
    background: var(--accent);
    color: var(--on-accent, #fff);
  }
  button:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
