<script lang="ts">
  /**
   * The input assist list, drawn over the terminal next to the cursor. Focus stays in the
   * terminal: the pane handles Tab, the arrows and Escape, and this only shows the list.
   * A click on a row takes it (mousedown is cancelled so the terminal keeps focus).
   */
  export type AssistRow = { label: string; detail: string };
  let {
    label,
    rows,
    index,
    x,
    y,
    above,
    onpick,
  }: {
    label: string;
    rows: AssistRow[];
    index: number;
    /** Window coordinates of the cursor cell's left edge, and of the row's bottom (or top when `above`). */
    x: number;
    y: number;
    /** Open upwards: the cursor is in the lower half of the pane. */
    above: boolean;
    onpick: (i: number) => void;
  } = $props();

  let list = $state<HTMLDivElement>();
  $effect(() => {
    // Keep the chosen row in view as the arrows move through a long list.
    list?.querySelector<HTMLElement>(`[data-i="${index}"]`)?.scrollIntoView({ block: "nearest" });
  });
</script>

<div
  bind:this={list}
  class="assist"
  role="listbox"
  aria-label={label}
  style:left="{Math.max(4, Math.min(x, window.innerWidth - 324))}px"
  style:top={above ? undefined : `${y}px`}
  style:bottom={above ? `${window.innerHeight - y}px` : undefined}
>
  {#each rows as r, i (i)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="row"
      class:on={i === index}
      role="option"
      tabindex="-1"
      aria-selected={i === index}
      data-i={i}
      onmousedown={(e) => e.preventDefault()}
      onclick={() => onpick(i)}
    >
      <span class="name">{r.label}</span>
      {#if r.detail}<span class="detail">{r.detail}</span>{/if}
    </div>
  {/each}
  <div class="foot" aria-hidden="true">Tab to insert · Esc to close</div>
</div>
<span class="live" role="status">{rows.length} {rows.length === 1 ? "suggestion" : "suggestions"}: {rows[index]?.label}. Tab inserts it, Escape closes.</span>

<style>
  .assist {
    position: fixed;
    z-index: 30;
    display: flex;
    flex-direction: column;
    min-width: 200px;
    max-width: 320px;
    max-height: 240px;
    overflow-y: auto;
    padding: var(--space-1);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-m);
    background: var(--surface);
    color: var(--fg);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
    font-size: var(--text-m);
  }
  .row {
    display: flex;
    align-items: baseline;
    gap: var(--space-3);
    padding: 3px var(--space-2);
    border-radius: var(--radius-s);
    cursor: pointer;
    white-space: nowrap;
  }
  .row.on {
    background: var(--accent);
    color: var(--on-accent);
  }
  .row:not(.on):hover {
    background: var(--hover);
  }
  .name {
    font-family: var(--font-mono, ui-monospace, monospace);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .detail {
    margin-left: auto;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--dim);
    font-size: var(--text-s);
  }
  .row.on .detail {
    color: inherit;
    opacity: 0.85;
  }
  .foot {
    padding: 3px var(--space-2) 1px;
    border-top: 1px solid var(--border);
    margin-top: 2px;
    color: var(--dim);
    font-size: var(--text-s);
  }
  .live {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
