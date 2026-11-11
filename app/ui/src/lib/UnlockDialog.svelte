<script lang="ts">
  import { onMount } from "svelte";
  import { focus } from "./focus";
  import { onEscape } from "./escape";
  import { modal } from "./modal";
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

  onMount(() => onEscape(() => oncancel()));
</script>

<div class="dlg-backdrop" role="presentation">
  <form use:modal class="dlg" onsubmit={(e) => (e.preventDefault(), submit())} aria-label="Secret vault">
    <h2>{isNew ? "Set a master password" : "Unlock the secret vault"}</h2>
    <p class="hint">
      {isNew
        ? "This computer has no system keychain, so saved passwords are kept in a file locked with a master password. You enter it once per launch."
        : "This computer has no system keychain; your saved passwords are in a file locked with your master password."}
    </p>
    <input type="password" use:focus bind:value={pw} autocomplete="off" aria-label="Master password" />
    {#if isNew}<input type="password" bind:value={pw2} autocomplete="off" aria-label="Repeat the master password" />{/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="actions">
      <button type="button" onclick={oncancel}>Not now</button>
      <button type="submit" class="primary" disabled={!pw || busy}>{busy ? "Working…" : isNew ? "Set password" : "Unlock"}</button>
    </div>
  </form>
</div>
