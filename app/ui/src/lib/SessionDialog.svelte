<script lang="ts">
  import { untrack } from "svelte";
  import Button from "./Button.svelte";
  import Dialog from "./Dialog.svelte";
  import SecretStoreCheck, { canStore, storeName } from "./SecretStoreCheck.svelte";
  import type { VaultStatus } from "./bindings/VaultStatus";
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
  import type { SshTarget } from "./bindings/SshTarget";

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
    draft = null,
    folders,
    sessions = [],
    error = null,
    vault = null,
    onunlock,
    onsubmit,
    oncancel,
  }: {
    /** Where secrets are kept on this computer, so the form names the right place. */
    vault?: VaultStatus | null;
    onunlock?: () => void;
    session?: SavedSession | null;
    /** Folder preselected for a new session. */
    defaultFolderId?: number | null;
    /** An open connection to keep: prefills a new session with its host, user and sign-in. */
    draft?: SshTarget | null;
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
  const d = untrack(() => draft);
  let host = $state(s?.host ?? d?.host ?? "");
  let port = $state(s?.port ?? d?.port ?? 22);
  let user = $state(s?.user ?? d?.user ?? "");
  let auth = $state<AuthKind>(s?.auth ?? d?.auth.method ?? "password");
  let keyPath = $state(s?.key_path ?? (d?.auth.method === "key" ? d.auth.path : "~/.ssh/id_ed25519"));
  let color = $state<string | null>(s?.color ?? null);
  let env = $state(s?.env ?? "");
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
  let secret = $state(d?.auth.method === "password" ? d.auth.password : d?.auth.method === "key" ? (d.auth.passphrase ?? "") : "");
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
      env: env.trim() || null,
      tags: tags.split(",").map((t) => t.trim()).filter(Boolean),
      jump_id: jumpId,
      tunnels: $state.snapshot(tunnels),
    };
    const typed = auth !== "agent" && secret !== "";
    // A locked vault cannot take a secret: what was typed is used for this connection only.
    const keep = saveSecret && canStore(vault);
    const update: SecretUpdate =
      typed && keep ? { action: "set", value: secret } : clearSecret ? { action: "clear" } : { action: "unchanged" };
    onsubmit({ input, secret: update, transient: typed && !keep ? secret : null, connect });
  }

</script>

<Dialog title={session ? "Edit session" : draft ? "Save this connection" : "New session"} onclose={oncancel} onsubmit={(e) => (e.preventDefault(), submit(!session))}>
    <section class="sec" aria-labelledby="sec-conn">
    <h3 id="sec-conn">Connection</h3>
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
        {jumpChosen.auth === "password" && !jumpChosen.has_secret ? " - it has no saved password yet, so you will be asked for it when you connect" : ""}
      </p>
    {/if}
    </section>
    <section class="sec" aria-labelledby="sec-auth">
    <h3 id="sec-auth">Sign-in</h3>
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
      <SecretStoreCheck bind:checked={saveSecret} {vault} {onunlock} what={auth === "key" ? "passphrase" : "password"} />
      {#if saveSecret && canStore(vault)}
        <p class="note"><Icon name="lock" size={14} />{auth === "key" ? "Passphrase" : "Password"} stored in the {storeName(vault)}, never in the database</p>
      {/if}
      {#if hadSecret}
        <label class="check"><input type="checkbox" bind:checked={clearSecret} /> Remove the saved secret</label>
      {/if}
    {/if}
    </section>
    <section class="sec" aria-labelledby="sec-org">
    <h3 id="sec-org">Organise</h3>
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
      <label class="envf">Environment <input bind:value={env} maxlength="8" list="env-presets" placeholder="prod, dev…" title="Optional badge shown on the tab and the terminal" autocapitalize="off" spellcheck="false" /></label>
    </div>
    <datalist id="env-presets"><option value="prod"></option><option value="staging"></option><option value="dev"></option></datalist>
    <label>Tags <input bind:value={tags} placeholder="prod, eu (comma separated)" /></label>
    <div class="colour">
      <span id="colour-label">Color</span>
      <div class="swatches" role="group" aria-labelledby="colour-label">
        <button type="button" class="none" class:sel={color === null} aria-label="No color" onclick={() => (color = null)}>∅</button>
        {#each swatches as c (c)}
          <button type="button" class:sel={color === c} style:background={c} aria-label={`Color ${c}`} onclick={() => (color = c)}></button>
        {/each}
      </div>
    </div>
    </section>
    <section class="sec tunnels" aria-labelledby="sec-tun">
      <div class="thead">
        <h3 id="sec-tun">Tunnels{tunnels.length ? ` (${tunnels.length})` : ""}</h3>
        <Button variant="ghost" size="sm" title="Add a port forward that is saved with this session" onclick={addTunnel}><Icon name="plus" size={14} />Add</Button>
      </div>
      {#each tunnels as t, i (i)}
        <div class="tun">
          <div class="row">
            <label class="grow">
              Type
              <select bind:value={t.kind}>
                {#each Object.entries(kindLabels) as [k, label] (k)}<option value={k}>{label}</option>{/each}
              </select>
            </label>
            <label class="port">
              {t.kind === "remote" ? "Server port" : "Local port"}
              <input type="number" min="0" max="65535" bind:value={t.bind_port} />
            </label>
            <Button variant="icon" aria-label="Remove tunnel" title="Remove tunnel" onclick={() => tunnels.splice(i, 1)}><Icon name="trash" size={14} /></Button>
          </div>
          {#if t.kind !== "dynamic"}
            <div class="row">
              <label class="grow">
                {t.kind === "remote" ? "Send to (from this computer)" : "Send to (from the server)"}
                <input class="mono" bind:value={t.dest_host} autocapitalize="off" spellcheck="false" />
              </label>
              <label class="port">Port <input type="number" min="1" max="65535" bind:value={t.dest_port} /></label>
            </div>
          {:else}
            <p class="hint">A SOCKS5 proxy on this computer; the app using it picks the destination.</p>
          {/if}
          <label class="check"><input type="checkbox" bind:checked={t.auto_start} /> Start when this session connects</label>
        </div>
      {/each}
    </section>
    {#if error}<p class="error">{error}</p>{/if}
  {#snippet actions()}
    <Button onclick={oncancel}>Cancel</Button>
    <!-- Enter saves an existing session; it connects only when the session is new. -->
    {#if session}
      <Button disabled={!valid} onclick={() => submit(true)}>Save &amp; connect</Button>
      <Button type="submit" variant="primary" disabled={!valid}>Save</Button>
    {:else}
      <Button disabled={!valid} onclick={() => submit(false)}>Save</Button>
      <Button type="submit" variant="primary" disabled={!valid}>Save &amp; connect</Button>
    {/if}
  {/snippet}
</Dialog>

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
    border-color: var(--border-strong);
  }
  /* var(--fg), not white: the dialog surface is white in the light themes. */
  .swatches .sel {
    border-color: var(--fg);
  }
  /* Same fields as before, grouped under four headings with a rule between groups. */
  .sec {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .sec + .sec {
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }
  .sec h3 {
    margin: 0;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--dim);
  }
  .sec .grow {
    min-width: 0;
  }
  .envf {
    width: 130px;
    flex: none;
  }
  .envf input {
    width: 100%;
    box-sizing: border-box;
  }
  .colour {
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: 12px;
    color: var(--dim);
  }
  .thead {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  /* One tunnel: listen side on the first line, destination on the second, so every field has room. */
  .tun {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  .tun .row {
    align-items: flex-end;
  }
  .tun .port {
    width: 96px;
    flex: none;
  }
  .tun .port input,
  .tun .grow input,
  .tun .grow select {
    width: 100%;
    box-sizing: border-box;
  }
</style>
