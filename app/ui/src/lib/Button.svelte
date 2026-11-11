<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";

  /**
   * The one button. `default` is outlined, `primary` is the filled main action, `danger` is an
   * outlined destructive one, `ghost` has no outline until hovered (toolbars), and `icon` is a
   * ghost sized for a single icon (always give it an `aria-label`).
   */
  let {
    variant = "default",
    size = "md",
    active = false,
    initialFocus = false,
    type = "button",
    class: extra = "",
    children,
    ...rest
  }: HTMLButtonAttributes & {
    variant?: "default" | "primary" | "danger" | "ghost" | "icon";
    size?: "md" | "sm";
    /** The "on" state of a toggle; pair it with `aria-pressed`. */
    active?: boolean;
    /** Take focus when the button appears (the safe choice in a prompt). */
    initialFocus?: boolean;
    children: Snippet;
  } = $props();

  function focusIf(node: HTMLButtonElement) {
    if (initialFocus) node.focus();
  }
</script>

<button {type} class="btn {variant} {size} {extra}" class:active use:focusIf {...rest}>{@render children()}</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    flex: none;
    /* Not stretched by a column layout. */
    width: fit-content;
    box-sizing: border-box;
    min-height: 24px;
    margin: 0;
    padding: var(--space-1) var(--space-4);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-m);
    background: none;
    color: var(--fg);
    font: inherit;
    font-size: var(--text-m);
    white-space: nowrap;
    cursor: pointer;
  }
  .btn:hover:not(:disabled) {
    background: var(--hover);
  }
  .btn:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .sm {
    padding: 2px var(--space-3);
    font-size: var(--text-s);
  }
  .primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--on-accent);
  }
  .primary:hover:not(:disabled) {
    background: var(--accent);
    filter: brightness(1.1);
  }
  .danger {
    border-color: var(--red);
    color: var(--red-text);
  }
  .ghost,
  .icon {
    border-color: transparent;
    color: var(--dim);
  }
  .ghost {
    padding: var(--space-1) var(--space-2);
  }
  .ghost.sm {
    padding: 2px var(--space-2);
  }
  .icon {
    min-width: 24px;
    padding: var(--space-1);
  }
  .ghost:hover:not(:disabled),
  .icon:hover:not(:disabled) {
    color: var(--fg);
  }
  .active,
  .active:hover:not(:disabled) {
    color: var(--accent-text);
  }
</style>
