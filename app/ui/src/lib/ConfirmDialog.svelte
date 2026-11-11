<script lang="ts">
  import { onMount } from "svelte";
  import { focus } from "./focus";
  import { onEscape } from "./escape";
  import { modal } from "./modal";

  let {
    title,
    body,
    confirmLabel,
    dontAsk = false,
    onconfirm,
    oncancel,
  }: {
    title: string;
    body: string;
    confirmLabel: string;
    /** Offer "Do not ask again"; its state is passed to `onconfirm`. */
    dontAsk?: boolean;
    onconfirm: (dontAskAgain: boolean) => void;
    oncancel: () => void;
  } = $props();

  let never = $state(false);

  onMount(() => onEscape(() => oncancel()));
</script>

<div class="dlg-backdrop" role="presentation">
  <div use:modal class="dlg" role="alertdialog" aria-labelledby="confirm-title" aria-describedby="confirm-body" tabindex="-1">
    <h2 id="confirm-title">{title}</h2>
    <p id="confirm-body">{body}</p>
    {#if dontAsk}
      <label class="check"><input type="checkbox" bind:checked={never} /> Do not ask again (change it in Settings &gt; Terminal)</label>
    {/if}
    <div class="actions">
      <button type="button" onclick={oncancel}>Cancel</button>
      <button type="button" class="primary" use:focus onclick={() => onconfirm(never)}>{confirmLabel}</button>
    </div>
  </div>
</div>

<style>
  p {
    margin: 0;
    line-height: 1.5;
  }
</style>
