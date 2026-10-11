<script lang="ts">
  import { focus } from "./focus";
  import type { SshTarget } from "./bindings/SshTarget";

  let { onconnect, oncancel }: { onconnect: (t: SshTarget) => void; oncancel: () => void } = $props();

  let host = $state("");
  let port = $state(22);
  let user = $state("");
  let method = $state<"password" | "key" | "agent">("password");
  let password = $state("");
  let keyPath = $state("~/.ssh/id_ed25519");
  let passphrase = $state("");

  const valid = $derived(host.trim() !== "" && user.trim() !== "" && port > 0 && port < 65536);

  function submit(e: Event) {
    e.preventDefault();
    if (!valid) return;
    const auth: SshTarget["auth"] =
      method === "password"
        ? { method, password }
        : method === "key"
          ? { method, path: keyPath, passphrase: passphrase || null }
          : { method };
    onconnect({ host: host.trim(), port, user: user.trim(), auth });
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && oncancel()} />

<div class="backdrop" role="presentation">
  <form class="dialog" onsubmit={submit} aria-label="New SSH connection">
    <h2>New SSH connection</h2>
    <div class="row">
      <label class="grow">Host <input use:focus bind:value={host} placeholder="example.com" /></label>
      <label class="narrow">Port <input type="number" bind:value={port} min="1" max="65535" /></label>
    </div>
    <label>User <input bind:value={user} autocomplete="username" /></label>
    <label>
      Authentication
      <select bind:value={method}>
        <option value="password">Password</option>
        <option value="key">Private key</option>
        <option value="agent">SSH agent</option>
      </select>
    </label>
    {#if method === "password"}
      <label>Password <input type="password" bind:value={password} autocomplete="current-password" /></label>
    {:else if method === "key"}
      <label>Key file <input bind:value={keyPath} /></label>
      <label>Passphrase (if any) <input type="password" bind:value={passphrase} /></label>
    {/if}
    <div class="actions">
      <button type="button" onclick={oncancel}>Cancel</button>
      <button type="submit" class="primary" disabled={!valid}>Connect</button>
    </div>
  </form>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.5);
    z-index: 10;
  }
  .dialog {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: 340px;
    padding: 18px;
    border-radius: 10px;
    background: #2b2d36;
    font-size: 13px;
  }
  h2 {
    margin: 0 0 4px;
    font-size: 15px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .row {
    display: flex;
    gap: 8px;
  }
  .grow {
    flex: 1;
  }
  .narrow {
    width: 80px;
  }
  input,
  select {
    font: inherit;
    color: inherit;
    padding: 6px 8px;
    border: 1px solid #44464f;
    border-radius: 5px;
    background: #1b1c21;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 6px;
  }
  button {
    font: inherit;
    color: inherit;
    padding: 6px 14px;
    border: 1px solid #44464f;
    border-radius: 5px;
    background: none;
    cursor: pointer;
  }
  .primary {
    background: #3b6fd8;
    border-color: #3b6fd8;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
