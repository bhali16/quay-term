<script lang="ts">
  import { onMount, type Snippet } from "svelte";
  import { onEscape } from "./escape";
  import { menu } from "./modal";

  /**
   * A popup menu at a point. The items are the caller's: `<button role="menuitem">`, or
   * `menuitemradio` with `aria-checked`, and `<hr>` between groups. Arrow keys, Home, End and
   * type-ahead move between them; Escape, a click outside or a right-click closes.
   */
  let {
    label,
    left,
    top,
    right,
    scroll = false,
    onclose,
    children,
  }: {
    label: string;
    /** Position in px from the window's edges; the menu is nudged back inside if it would stick out. */
    left?: number;
    top?: number;
    right?: number;
    /** A long list (all tabs): limit the size and scroll. */
    scroll?: boolean;
    onclose: () => void;
    children: Snippet;
  } = $props();

  const px = (n?: number) => (n === undefined ? undefined : `${n}px`);

  // Level 2: a menu is above any dialog, so it gets Escape first.
  // svelte-ignore state_referenced_locally
  onMount(() => onEscape(() => onclose(), 2));
</script>

<div class="shield" role="presentation" onmousedown={onclose} oncontextmenu={(e) => (e.preventDefault(), onclose())}></div>
<div use:menu class="menu" class:scroll role="menu" aria-label={label} tabindex="-1" style:left={px(left)} style:top={px(top)} style:right={px(right)}>
  {@render children()}
</div>

<style>
  .shield {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
  .menu {
    position: fixed;
    z-index: 41;
    display: flex;
    flex-direction: column;
    min-width: 180px;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    color: var(--fg);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
    font-family: var(--font-ui);
    font-size: 13px;
  }
  .scroll {
    max-width: min(320px, calc(100vw - 16px));
    max-height: min(420px, calc(100vh - 80px));
    overflow-y: auto;
  }
  .menu :global(button) {
    all: unset;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
    padding: 6px 10px;
    border-radius: 5px;
    cursor: pointer;
  }
  .menu :global(button:hover:not(:disabled)),
  .menu :global(button:focus-visible) {
    background: var(--hover);
  }
  .menu :global(button:disabled) {
    opacity: 0.4;
    cursor: default;
  }
  .menu :global(button[aria-checked="true"]) {
    background: var(--hover);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .menu :global(hr) {
    width: 100%;
    box-sizing: border-box;
    margin: 4px 0;
    border: 0;
    border-top: 1px solid var(--border);
  }
</style>
