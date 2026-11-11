<script lang="ts">
  import Button from "./Button.svelte";
  import Dialog from "./Dialog.svelte";
  import { focus } from "./focus";
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
</script>

<Dialog title={`Password for ${title}`} onclose={oncancel} onsubmit={(e) => (e.preventDefault(), password && onsubmit(password, remember && canStore(vault)))}>
    <input type="password" use:focus bind:value={password} autocomplete="off" aria-label="Password" />
    <SecretStoreCheck bind:checked={remember} {vault} {onunlock} />
  {#snippet actions()}
    <Button onclick={oncancel}>Cancel</Button>
    <Button type="submit" variant="primary" disabled={!password}>Connect</Button>
  {/snippet}
</Dialog>
