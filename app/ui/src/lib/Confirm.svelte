<script lang="ts">
  import Button from "./Button.svelte";
  import Dialog from "./Dialog.svelte";

  let {
    title,
    body,
    confirmLabel,
    danger = false,
    dontAsk = false,
    onconfirm,
    oncancel,
  }: {
    title: string;
    body: string;
    confirmLabel: string;
    /** The action cannot be taken back: the button is red and focus starts on Cancel. */
    danger?: boolean;
    /** Offer "Do not ask again"; its state is passed to `onconfirm`. */
    dontAsk?: boolean;
    onconfirm: (dontAskAgain: boolean) => void;
    oncancel: () => void;
  } = $props();

  const uid = $props.id();
  let never = $state(false);
</script>

<Dialog {title} alert describedby="{uid}-body" onclose={oncancel}>
  <p id="{uid}-body">{body}</p>
  {#if dontAsk}
    <label class="check"><input type="checkbox" bind:checked={never} /> Do not ask again (change it in Settings &gt; Terminal)</label>
  {/if}
  {#snippet actions()}
    <Button initialFocus={danger} onclick={oncancel}>Cancel</Button>
    <Button variant={danger ? "danger" : "primary"} initialFocus={!danger} onclick={() => onconfirm(never)}>{confirmLabel}</Button>
  {/snippet}
</Dialog>

<style>
  p {
    margin: 0;
    line-height: 1.5;
  }
</style>
