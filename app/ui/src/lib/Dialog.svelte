<script lang="ts">
  import { onMount, type Snippet } from "svelte";
  import { onEscape } from "./escape";
  import { modal } from "./modal";

  /**
   * The shell every dialog shares: backdrop, modal focus handling, one Escape for the dialog on
   * top, a heading and a pinned row of actions. With `onsubmit` it is a form, so Enter submits.
   */
  let {
    title,
    label,
    alert = false,
    top = false,
    plain = false,
    backdropClose = false,
    describedby,
    class: extra = "",
    onclose,
    onsubmit,
    children,
    actions,
  }: {
    /** Shown as the heading and used as the accessible name. */
    title?: string;
    /** Accessible name when there is no heading (or the dialog draws its own). */
    label?: string;
    /** A prompt that interrupts (`alertdialog`): confirm, host key. */
    alert?: boolean;
    /** Drawn above every other dialog and first in line for Escape (host key, sign-in). */
    top?: boolean;
    /** No panel styling: the content brings its own (the command palette). */
    plain?: boolean;
    /** A click outside closes it. Only for dialogs with nothing to lose. */
    backdropClose?: boolean;
    describedby?: string;
    class?: string;
    /** Escape, and a backdrop click when `backdropClose` is set. */
    onclose: () => void;
    onsubmit?: (e: SubmitEvent) => void;
    children: Snippet;
    actions?: Snippet;
  } = $props();

  const uid = $props.id();

  // svelte-ignore state_referenced_locally
  onMount(() => onEscape(() => onclose(), top ? 1 : 0));
</script>

<div class="dlg-backdrop" class:top class:plain role="presentation" onmousedown={(e) => backdropClose && e.target === e.currentTarget && onclose()}>
  <svelte:element
    this={onsubmit ? "form" : "div"}
    use:modal
    class="{plain ? '' : 'dlg'} {extra}"
    role={alert ? "alertdialog" : "dialog"}
    aria-labelledby={title ? `${uid}-title` : undefined}
    aria-label={title ? undefined : label}
    aria-describedby={describedby}
    tabindex="-1"
    {onsubmit}
  >
    {#if title}<h2 id="{uid}-title">{title}</h2>{/if}
    {@render children()}
    {#if actions}<div class="actions">{@render actions()}</div>{/if}
  </svelte:element>
</div>

<style>
  /* Above the other dialogs (z-index 10): a host key or sign-in prompt must never hide behind one. */
  .top {
    z-index: 20;
  }
  .plain {
    align-items: flex-start;
    padding-top: 12vh;
  }
</style>
