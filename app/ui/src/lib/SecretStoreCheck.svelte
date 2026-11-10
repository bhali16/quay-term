<script lang="ts" module>
  import type { VaultStatus } from "./bindings/VaultStatus";

  /** False while secrets sit in a master-password file that has not been unlocked. */
  export const canStore = (vault: VaultStatus | null | undefined) => vault?.mode !== "locked";
  /** Where a saved secret goes on this computer, in the user's words. */
  export const storeName = (vault: VaultStatus | null | undefined) =>
    !vault || vault.mode === "keychain" ? "system keychain" : "secret vault";
</script>

<script lang="ts">
  let {
    checked = $bindable(true),
    vault = null,
    what = "",
    onunlock,
  }: {
    checked: boolean;
    vault?: VaultStatus | null;
    /** "password" or "passphrase"; empty reads as "Save in the ...". */
    what?: string;
    onunlock?: () => void;
  } = $props();
</script>

{#if canStore(vault)}
  <label class="check"><input type="checkbox" bind:checked /> Save {what ? `the ${what} ` : ""}in the {storeName(vault)}</label>
{:else}
  <p class="hint">
    The secret vault is locked, so {what ? `the ${what}` : "it"} is used for this connection only and not saved.
    {#if onunlock}<button type="button" class="link" onclick={onunlock}>{vault?.is_new ? "Set a master password…" : "Unlock…"}</button>{/if}
  </p>
{/if}

<style>
  .link {
    font: inherit;
    color: var(--accent);
    background: none;
    border: 0;
    padding: 0;
    text-decoration: underline;
    cursor: pointer;
  }
</style>
