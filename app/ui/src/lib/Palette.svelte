<script lang="ts" module>
  export type PaletteItem = {
    id: string;
    group: "Actions" | "Sessions" | "Snippets" | "Tabs";
    title: string;
    subtitle?: string;
    /** Shortcut shown on the right. */
    hint?: string;
    run: () => void;
  };
</script>

<script lang="ts">
  import { focus } from "./focus";
  import Icon from "./Icon.svelte";
  import { parseQuery, rank } from "./fuzzy";

  let { items, recent, onrun, onclose }: {
    items: PaletteItem[];
    /** Ids of recently chosen items, newest first. */
    recent: string[];
    onrun: (item: PaletteItem) => void;
    onclose: () => void;
  } = $props();

  let raw = $state("");
  let sel = $state(0);
  let listEl: HTMLDivElement | undefined = $state();

  const modeGroup: Record<string, PaletteItem["group"]> = { ">": "Actions", "@": "Sessions", "!": "Snippets", "/": "Tabs" };
  const modeHelp = [
    [">", "actions"],
    ["@", "sessions"],
    ["!", "snippets"],
    ["/", "tabs"],
  ];
  const order: PaletteItem["group"][] = ["Actions", "Sessions", "Snippets", "Tabs"];

  const parsed = $derived(parseQuery(raw));
  type Row = PaletteItem & { recent?: boolean };
  const results = $derived.by((): Row[] => {
    const { mode, query } = parsed;
    const pool = mode ? items.filter((i) => i.group === modeGroup[mode]) : items;
    if (query.trim()) return rank(pool, query, (i) => [i.title, i.subtitle ?? ""]).slice(0, 60);
    // Empty query: recent first, then the rest in group order.
    const byId = new Map(pool.map((i) => [i.id, i]));
    const recents = recent.map((id) => byId.get(id)).filter((i): i is PaletteItem => !!i);
    const seen = new Set(recents.map((i) => i.id));
    const rest = order.flatMap((g) => pool.filter((i) => i.group === g && !seen.has(i.id)));
    return [...recents.map((i) => ({ ...i, recent: true })), ...rest].slice(0, 80);
  });

  $effect(() => {
    results;
    sel = 0;
  });
  $effect(() => {
    const el = listEl?.querySelector<HTMLElement>(`[data-i="${sel}"]`);
    el?.scrollIntoView({ block: "nearest" });
  });

  function onkey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onclose();
    } else if (e.key === "ArrowDown" || (e.ctrlKey && e.key === "n")) {
      e.preventDefault();
      sel = results.length ? (sel + 1) % results.length : 0;
    } else if (e.key === "ArrowUp" || (e.ctrlKey && e.key === "p")) {
      e.preventDefault();
      sel = results.length ? (sel - 1 + results.length) % results.length : 0;
    } else if (e.key === "Enter") {
      e.preventDefault();
      const item = results[sel];
      if (item) onrun(item);
    }
  }
  /** Section title above row `i`; only when the query is empty and the group changes. */
  function header(i: number): string | null {
    if (parsed.query.trim()) return null;
    const row = results[i];
    const prev = results[i - 1];
    if (row.recent) return i === 0 ? "Recent" : null;
    return !prev || prev.recent || prev.group !== row.group ? row.group : null;
  }
  const icon = (g: PaletteItem["group"]) => (g === "Sessions" ? "server" : g === "Snippets" ? "terminal" : g === "Tabs" ? "folder" : "search");
</script>

<div class="dlg-backdrop" role="presentation" onmousedown={(e) => e.target === e.currentTarget && onclose()}>
  <div class="pal" role="dialog" aria-label="Command palette">
    <div class="box">
      <Icon name="search" size={16} />
      <input
        use:focus
        bind:value={raw}
        onkeydown={onkey}
        role="combobox"
        aria-expanded="true"
        aria-controls="pal-list"
        aria-activedescendant={results[sel] ? `pal-${sel}` : undefined}
        placeholder="Search actions, sessions, snippets and tabs"
        aria-label="Command palette"
        autocapitalize="off"
        spellcheck="false"
      />
    </div>
    <div class="list" id="pal-list" role="listbox" bind:this={listEl}>
      {#each results as item, i (item.id)}
        {@const h = header(i)}
        {#if h}<div class="grp">{h}</div>{/if}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          id="pal-{i}"
          data-i={i}
          class="row"
          class:sel={i === sel}
          role="option"
          aria-selected={i === sel}
          tabindex="-1"
          onmousemove={() => (sel = i)}
          onclick={() => onrun(item)}
        >
          <span class="ic"><Icon name={icon(item.group)} size={15} /></span>
          <span class="t">{item.title}</span>
          {#if item.subtitle}<span class="s">{item.subtitle}</span>{/if}
          {#if parsed.query.trim()}<span class="g">{item.group}</span>{/if}
          {#if item.hint}<kbd>{item.hint}</kbd>{/if}
        </div>
      {:else}
        <div class="none">Nothing matches "{parsed.query}".</div>
      {/each}
    </div>
    <div class="foot">
      {#each modeHelp as [k, label] (k)}<span><kbd>{k}</kbd> {label}</span>{/each}
      <span class="sp"><kbd>↑↓</kbd> move <kbd>↵</kbd> run <kbd>esc</kbd> close</span>
    </div>
  </div>
</div>

<style>
  .dlg-backdrop {
    align-items: flex-start;
    padding-top: 12vh;
  }
  .pal {
    width: min(640px, calc(100vw - 32px));
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
    box-shadow: 0 18px 50px rgba(0, 0, 0, 0.5);
    overflow: hidden;
  }
  .box {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
    color: var(--dim);
  }
  input {
    flex: 1;
    min-width: 0;
    font: inherit;
    font-size: 15px;
    color: var(--fg);
    background: none;
    border: 0;
    outline: none;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 6px;
  }
  .grp {
    padding: 8px 8px 4px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--dim);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 8px;
    border-radius: 7px;
    font-size: 14px;
    cursor: pointer;
  }
  .row.sel {
    background: var(--accent);
    color: var(--on-accent, #fff);
  }
  .ic {
    display: flex;
    flex: none;
    opacity: 0.75;
  }
  .t {
    flex: none;
    max-width: 55%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .s {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    opacity: 0.65;
  }
  .g {
    margin-left: auto;
    font-size: 11px;
    opacity: 0.6;
  }
  kbd {
    flex: none;
    margin-left: auto;
    padding: 1px 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    font: inherit;
    font-size: 11px;
    opacity: 0.85;
  }
  .row.sel kbd {
    border-color: currentColor;
  }
  .none {
    padding: 18px;
    text-align: center;
    color: var(--dim);
    font-size: 13px;
  }
  .foot {
    display: flex;
    flex-wrap: wrap;
    gap: 14px;
    padding: 8px 14px;
    border-top: 1px solid var(--border);
    font-size: 12px;
    color: var(--dim);
  }
  .foot kbd {
    margin-left: 0;
  }
  .foot .sp {
    margin-left: auto;
  }
</style>
