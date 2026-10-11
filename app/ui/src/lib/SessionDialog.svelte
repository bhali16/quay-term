<script lang="ts">
  import { focus } from "./focus";
  import type { AuthKind } from "./bindings/AuthKind";
  import type { Folder } from "./bindings/Folder";
  import type { SavedSession } from "./bindings/SavedSession";
  import type { SecretUpdate } from "./bindings/SecretUpdate";
  import type { SessionInput } from "./bindings/SessionInput";

  export type Result = {
    input: SessionInput;
    secret: SecretUpdate;
    /** A secret typed but not saved: used for this connection only. */
    transient: string | null;
    connect: boolean;
  };

  let {
    session = null,
    folders,
    error = null,
    onsubmit,
    oncancel,
  }: {
    session?: SavedSession | null;
    folders: Folder[];
    error?: string | null;
    onsubmit: (r: Result) => void;
    oncancel: () => void;
  } = $props();

  const swatches = ["#3b6fd8", "#2f9e6b", "#d98e04", "#d9534f", "#8e5bd9", "#6b7280"];

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
  let folderId = $state<number | null>(s?.folder_id ?? null);
  let tags = $state((s?.tags ?? []).join(", "));
  let secret = $state("");
  let saveSecret = $state(true);
  let clearSecret = $state(false);
  const hadSecret = s?.has_secret ?? false;

  const secretLabel = $derived(auth === "password" ? "Password" : "Key passphrase (if any)");
  const valid = $derived(
    host.trim() !== "" && user.trim() !== "" && port > 0 && port < 65536 && (auth !== "key" || keyPath.trim() !== ""),
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
    };
    const typed = auth !== "agent" && secret !== "";
    const update: SecretUpdate =
      typed && saveSecret ? { action: "set", value: secret } : clearSecret ? { action: "clear" } : { action: "unchanged" };
    onsubmit({ input, secret: update, transient: typed && !saveSecret ? secret : null, connect });
  }
</script>

<div class="dlg-backdrop" role="presentation" onkeydown={(e) => e.key === "Escape" && oncancel()}>
  <form class="dlg" onsubmit={(e) => (e.preventDefault(), submit(true))} aria-label="Session">
    <h2>{session ? "Edit session" : "New session"}</h2>
    <label>Name <input bind:value={name} placeholder="Optional, defaults to user@host" /></label>
    <div class="row">
      <label class="grow">Host <input use:focus bind:value={host} placeholder="example.com" /></label>
      <label class="narrow">Port <input type="number" bind:value={port} min="1" max="65535" /></label>
    </div>
    <label>User <input bind:value={user} autocomplete="username" /></label>
    <label>
      Authentication
      <select bind:value={auth}>
        <option value="password">Password</option>
        <option value="key">Private key</option>
        <option value="agent">SSH agent</option>
      </select>
    </label>
    {#if auth === "key"}
      <label>Key file <input bind:value={keyPath} /></label>
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
      {#if hadSecret}
        <label class="check"><input type="checkbox" bind:checked={clearSecret} /> Remove the saved secret</label>
      {/if}
    {/if}
    <div class="row">
      <label class="grow">
        Folder
        <select bind:value={folderId}>
          <option value={null}>None</option>
          {#each folders as f (f.id)}
            <option value={f.id}>{f.name}</option>
          {/each}
        </select>
      </label>
    </div>
    <label>Tags <input bind:value={tags} placeholder="prod, eu (comma separated)" /></label>
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
    border-color: #44464f;
  }
  .swatches .sel {
    border-color: #fff;
  }
</style>
