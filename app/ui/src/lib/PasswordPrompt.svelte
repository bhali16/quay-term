<script lang="ts">
  import { onMount } from "svelte";
  import { focus } from "./focus";
  import { onEscape } from "./escape";
  import SecretStoreCheck, { canStore } from "./SecretStoreCheck.svelte";
  import type { VaultStatus } from "./bindings/VaultStatus";
  let {
    title,
    vault = null,
    onunlock,
    onsubmit,
    oncancel,
  }: {
    title: string;
    vault?: VaultStatus | null;
    onunlock?: () => void;
    onsubmit: (password: string, remember: boolean) => void;
    oncancel: () => void;
  } = $props();

  let password = $state("");
  let remember = $state(true);

  onMount(() => onEscape(() => oncancel()));
</script>

<div class="dlg-backdrop" role="presentation">
  <form class="dlg" onsubmit={(e) => (e.preventDefault(), password && onsubmit(password, remember && canStore(vault)))} aria-label="Password">
    <h2>Password for {title}</h2>
    <input type="password" use:focus bind:value={password} autocomplete="off" aria-label="Password" />
    <SecretStoreCheck bind:checked={remember} {vault} {onunlock} />
    <div class="actions">
      <button type="button" onclick={oncancel}>Cancel</button>
      <button type="submit" class="primary" disabled={!password}>Connect</button>
    </div>
  </form>
</div>
