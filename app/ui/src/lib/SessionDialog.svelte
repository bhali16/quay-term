<script lang="ts">
  import { untrack } from "svelte";
  import Icon from "./Icon.svelte";
  import KeyFileField from "./KeyFileField.svelte";
  import { focus } from "./focus";
  import type { AuthKind } from "./bindings/AuthKind";
  import type { Folder } from "./bindings/Folder";
  import type { SavedSession } from "./bindings/SavedSession";
  import type { SecretUpdate } from "./bindings/SecretUpdate";
  import type { SessionInput } from "./bindings/SessionInput";
  import type { TunnelSpec } from "./bindings/TunnelSpec";
  import type { TunnelKind } from "./bindings/TunnelKind";

  export type Result = {
    input: SessionInput;
    secret: SecretUpdate;
    /** A secret typed but not saved: used for this connection only. */
    transient: string | null;
    connect: boolean;
  };

  let {
    session = null,
    defaultFolderId = null,
    folders,
    sessions = [],
    error = null,
    onsubmit,
    oncancel,
  }: {
    session?: SavedSession | null;
    /** Folder preselected for a new session. */
    defaultFolderId?: number | null;
    folders: Folder[];
    /** All saved sessions, to pick a jump host from. */
    sessions?: SavedSession[];
    error?: string | null;
    onsubmit: (r: Result) => void;
    oncancel: () => void;
  } = $props();

  const swatches = ["var(--accent)", "#2f9e6b", "#d98e04", "#d9534f", "#8e5bd9", "#6b7280"];

  // Fields start from the session being edited; the dialog is remounted per use.
  const s = untrackedSession();
  function untrackedSession() {
    return session;
  }
  let name = $state(s?.name ?? "");
  let host = $state(s?.host ?? "");
  let port = $state(s?.port ?? 22);
  let user = $state(s?.user ?? "");
  let auth = $state<AuthKind>(s?.auth ?? "password");
  let keyPath = $state(s?.key_path ?? "~/.ssh/id_ed25519");
  let color = $state<string | null>(s?.color ?? null);
  let folderId = $state<number | null>(s ? s.folder_id : untrack(() => defaultFolderId));
  let tags = $state((s?.tags ?? []).join(", "));
  let jumpId = $state<number | null>(s?.jump_id ?? null);
  let tunnels = $state<TunnelSpec[]>((s?.tunnels ?? []).map((t) => ({ ...t })));

  const kindLabels: Record<TunnelKind, string> = { local: "Local", remote: "Remote", dynamic: "SOCKS5" };
  function addTunnel() {
    tunnels.push({ kind: "local", bind_host: "127.0.0.1", bind_port: 8080, dest_host: "localhost", dest_port: 80, auto_start: false });
  }
  const tunnelsValid = $derived(
    tunnels.every(
      (t) => t.bind_port >= 0 && t.bind_port < 65536 && (t.kind === "dynamic" || (t.dest_host.trim() !== "" && t.dest_port > 0 && t.dest_port < 65536)),
    ),
  );
  let secret = $state("");
  let saveSecret = $state(true);
  let clearSecret = $state(false);
  const hadSecret = s?.has_secret ?? false;

  /** Folders as an indented tree, in display order. */
  const folderOptions = $derived.by(() => {
    const out: { id: number; label: string }[] = [];
    const walk = (parent: number | null, depth: number) => {
      for (const f of folders.filter((x) => x.parent_id === parent)) {
        out.push({ id: f.id, label: `${"\u00a0\u00a0".repeat(depth)}${depth ? "↳ " : ""}${f.name}` });
        walk(f.id, depth + 1);
      }
    };
    walk(null, 0);
    return out;
  });

  /** Sessions that can be a jump host: anything but this one. Loops are refused on save. */
  const jumpOptions = $derived(sessions.filter((x) => x.id !== s?.id));
  const jumpChosen = $derived(jumpOptions.find((x) => x.id === jumpId) ?? null);

  const secretLabel = $derived(auth === "password" ? "Password" : "Key passphrase (if any)");
  const valid = $derived(
    host.trim() !== "" && user.trim() !== "" && port > 0 && port < 65536 && (auth !== "key" || keyPath.trim() !== "") && tunnelsValid,
  );

  function submit(connect: boolean) {
    if (!valid) return;
    const input: SessionInput = {
      name: name.trim(),
      folder_id: folderId,
      host: host.trim(),
      port,
      user: user.trim(),
      auth,
      key_path: auth === "key" ? keyPath.trim() : null,
      color,
      tags: tags.split(",").map((t) => t.trim()).filter(Boolean),
      jump_id: jumpId,
      tunnels: $state.snapshot(tunnels),
    };
    const typed = auth !== "agent" && secret !== "";
    const update: SecretUpdate =
      typed && saveSecret ? { action: "set", value: secret } : clearSecret ? { action: "clear" } : { action: "unchanged" };
    onsubmit({ input, secret: update, transient: typed && !saveSecret ? secret : null, connect });
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && oncancel()} />

<div class="dlg-backdrop" role="presentation">
  <form class="dlg" onsubmit={(e) => (e.preventDefault(), submit(true))} aria-label="Session">
    <h2>{session ? "Edit session" : "New session"}</h2>
    <label>Name <input bind:value={name} placeholder="Optional, defaults to user@host" /></label>
    <div class="row">
      <label class="grow">Host <input class="mono" use:focus bind:value={host} placeholder="example.com" /></label>
      <label class="narrow">Port <input type="number" bind:value={port} min="1" max="65535" /></label>
    </div>
    <label>User <input class="mono" bind:value={user} autocomplete="username" /></label>
    <label>
      Connect via
      <select bind:value={jumpId}>
        <option value={null}>Directly</option>
        {#each jumpOptions as j (j.id)}
          <option value={j.id}>{j.name} ({j.user}@{j.host})</option>
        {/each}
      </select>
    </label>
    {#if jumpChosen}
      <p class="note">
        <Icon name="lock" size={14} />Signs in to {jumpChosen.name} first, with the credentials saved for it
        {jumpChosen.auth === "password" && !jumpChosen.has_secret ? " - it has no saved password yet, so save one there" : ""}
      </p>
    {/if}
    <label>
      Authentication
      <select bind:value={auth}>
        <option value="password">Password</option>
        <option value="key">Private key</option>
        <option value="agent">SSH agent</option>
      </select>
    </label>
    {#if auth === "key"}
      <KeyFileField bind:value={keyPath} />
    {/if}
    {#if auth !== "agent"}
      <label>
        {secretLabel}
        <input
          type="password"
          bind:value={secret}
          autocomplete="off"
          placeholder={hadSecret ? "Saved - leave blank to keep" : ""}
        />
      </label>
      <label class="check"><input type="checkbox" bind:checked={saveSecret} /> Save in the system keychain</label>
      {#if saveSecret}
        <p class="note"><Icon name="lock" size={14} />{auth === "key" ? "Passphrase" : "Password"} stored in the OS keychain, never in the database</p>
      {/if}
      {#if hadSecret}
        <label class="check"><input type="checkbox" bind:checked={clearSecret} /> Remove the saved secret</label>
      {/if}
    {/if}
    <div class="row">
      <label class="grow">
        Folder
        <select bind:value={folderId}>
          <option value={null}>None</option>
          {#each folderOptions as f (f.id)}
            <option value={f.id}>{f.label}</option>
          {/each}
        </select>
      </label>
    </div>
    <label>Tags <input bind:value={tags} placeholder="prod, eu (comma separated)" /></label>
    <div class="tunnels">
      <div class="thead">
        <span>Tunnels</span>
        <button type="button" class="small" onclick={addTunnel}><Icon name="plus" size={14} />Add</button>
      </div>
      {#each tunnels as t, i (i)}
        <div class="trow">
          <select bind:value={t.kind} aria-label="Tunnel type">
            {#each Object.entries(kindLabels) as [k, label] (k)}<option value={k}>{label}</option>{/each}
          </select>
          <input class="num" type="number" min="0" max="65535" bind:value={t.bind_port} aria-label="Port to listen on" title={t.kind === "remote" ? "Port on the server" : "Port on this computer"} />
          {#if t.kind !== "dynamic"}
            <span class="arrow">→</span>
            <input class="mono dest" bind:value={t.dest_host} aria-label="Destination host" autocapitalize="off" spellcheck="false" />
            <input class="num" type="number" min="1" max="65535" bind:value={t.dest_port} aria-label="Destination port" />
          {:else}
            <span class="arrow dim">proxy</span>
          {/if}
          <label class="auto" title="Start when this session connects"><input type="checkbox" bind:checked={t.auto_start} />Auto</label>
          <button type="button" class="small" aria-label="Remove tunnel" onclick={() => tunnels.splice(i, 1)}><Icon name="x" size={14} /></button>
        </div>
      {/each}
      {#if tunnels.length === 0}
        <p class="note">Port forwards saved with this session. Mark one Auto to start it on connect.</p>
      {/if}
    </div>
    <div class="swatches" role="group" aria-label="Colour">
      <button type="button" class="none" class:sel={color === null} aria-label="No colour" onclick={() => (color = null)}>∅</button>
      {#each swatches as c (c)}
        <button type="button" class:sel={color === c} style:background={c} aria-label={`Colour ${c}`} onclick={() => (color = c)}></button>
      {/each}
    </div>
    {#if error}<p class="error">{error}</p>{/if}
    <div class="actions">
      <button type="button" onclick={oncancel}>Cancel</button>
      <button type="button" disabled={!valid} onclick={() => submit(false)}>Save</button>
      <button type="submit" class="primary" disabled={!valid}>Save &amp; connect</button>
    </div>
  </form>
</div>

<style>
  .swatches {
    display: flex;
    gap: 8px;
  }
  .swatches button {
    width: 22px;
    height: 22px;
    padding: 0;
    border: 2px solid transparent;
    border-radius: 50%;
    cursor: pointer;
    color: inherit;
    font-size: 12px;
  }
  .swatches .none {
    background: none;
    border-color: var(--border);
  }
  /* var(--fg), not white: the dialog surface is white in the light themes. */
  .swatches .sel {
    border-color: var(--fg);
  }
  .tunnels {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .thead {
    display: flex;
    align-items: center;
    justify-content: space-between;
    color: var(--dim);
  }
  .trow {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .trow select {
    width: 84px;
    flex: none;
  }
  .trow .num {
    width: 68px;
    flex: none;
  }
  .trow .dest {
    flex: 1;
    min-width: 0;
  }
  .arrow {
    color: var(--dim);
  }
  .auto {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 4px;
    flex: none;
    font-size: 12px;
  }
  .small {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px 6px;
    background: none;
    border: 0;
    color: var(--dim);
    cursor: pointer;
    border-radius: 4px;
    font: inherit;
  }
  .small:hover {
    background: var(--hover);
    color: var(--fg);
  }
</style>
