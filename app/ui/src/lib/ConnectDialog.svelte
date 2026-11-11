<script lang="ts">
  import Button from "./Button.svelte";
  import Dialog from "./Dialog.svelte";
  import KeyFileField from "./KeyFileField.svelte";
  import SecretStoreCheck, { canStore } from "./SecretStoreCheck.svelte";
  import type { VaultStatus } from "./bindings/VaultStatus";
  import { focus } from "./focus";
  import type { SshTarget } from "./bindings/SshTarget";

  let {
    vault = null,
    onunlock,
    onconnect,
    oncancel,
  }: {
    vault?: VaultStatus | null;
    onunlock?: () => void;
    /** `save` asks for the connection to be kept as a saved session; `keepSecret` also keeps its password or passphrase. */
    onconnect: (t: SshTarget, save: { save: boolean; keepSecret: boolean }) => void;
    oncancel: () => void;
  } = $props();

  let host = $state("");
  let port = $state(22);
  let user = $state("");
  let method = $state<"password" | "key" | "agent">("password");
  let password = $state("");
  let keyPath = $state("~/.ssh/id_ed25519");
  let passphrase = $state("");
  let save = $state(false);
  let keepSecret = $state(true);

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
    onconnect({ host: host.trim(), port, user: user.trim(), auth }, { save, keepSecret: keepSecret && canStore(vault) });
  }
</script>

<Dialog title="New SSH connection" onclose={oncancel} onsubmit={submit}>
    <div class="row">
      <label class="grow">Host <input class="mono" use:focus bind:value={host} placeholder="example.com" /></label>
      <label class="narrow">Port <input type="number" bind:value={port} min="1" max="65535" /></label>
    </div>
    <label>User <input class="mono" bind:value={user} autocomplete="username" /></label>
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
      <KeyFileField bind:value={keyPath} />
      <label>Passphrase (if any) <input type="password" bind:value={passphrase} /></label>
    {/if}
    <label class="check"><input type="checkbox" bind:checked={save} /> Save to sessions</label>
    {#if save && method !== "agent"}
      <SecretStoreCheck bind:checked={keepSecret} {vault} {onunlock} what={method === "key" ? "passphrase" : "password"} />
    {/if}
  {#snippet actions()}
    <Button onclick={oncancel}>Cancel</Button>
    <Button type="submit" variant="primary" disabled={!valid}>Connect</Button>
  {/snippet}
</Dialog>
