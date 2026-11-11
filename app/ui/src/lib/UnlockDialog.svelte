<script lang="ts">
  import Button from "./Button.svelte";
  import Dialog from "./Dialog.svelte";
  import { focus } from "./focus";
  import { vaultUnlock } from "./ipc";

  let { isNew, ondone, oncancel }: { isNew: boolean; ondone: () => void; oncancel: () => void } = $props();

  let pw = $state("");
  let pw2 = $state("");
  let error = $state<string | null>(null);
  let busy = $state(false);

  async function submit() {
    error = null;
    if (isNew && pw !== pw2) return void (error = "The two passwords differ.");
    busy = true;
    try {
      await vaultUnlock(pw);
      ondone();
    } catch (e) {
      error = `${e}`;
    } finally {
      busy = false;
    }
  }
</script>

<Dialog title={isNew ? "Set a master password" : "Unlock the secret vault"} onclose={oncancel} onsubmit={(e) => (e.preventDefault(), submit())}>
    <p class="hint">
      {isNew
        ? "This computer has no system keychain, so saved passwords are kept in a file locked with a master password. You enter it once per launch."
        : "This computer has no system keychain; your saved passwords are in a file locked with your master password."}
    </p>
    <input type="password" use:focus bind:value={pw} autocomplete="off" aria-label="Master password" />
    {#if isNew}<input type="password" bind:value={pw2} autocomplete="off" aria-label="Repeat the master password" />{/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#snippet actions()}
    <Button onclick={oncancel}>Not now</Button>
    <Button type="submit" variant="primary" disabled={!pw || busy}>{busy ? "Working…" : isNew ? "Set password" : "Unlock"}</Button>
  {/snippet}
</Dialog>
