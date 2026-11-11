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
  import { onMount } from "svelte";
  import { focus } from "./focus";
  import { onEscape } from "./escape";
  import { modal } from "./modal";
  import Icon from "./Icon.svelte";
  import { parseQuery, rank } from "./fuzzy";
  import { formatShortcut } from "./keys";
  import { ago, applyFilter, collapse, hostsOf, noFilter, type HistoryFilter, type HistoryRow, type Range } from "./history";
  import type { HistoryEntry } from "./bindings/HistoryEntry";

  /** Command history (`$` mode). `recording` is the setting, to explain an empty list. */
  export type HistorySource = {
    recording: boolean;
    load: () => Promise<HistoryEntry[]>;
    /** Type the command at the focused prompt (no Enter). */
    insert: (command: string) => void;
    /** Forget the command everywhere it ran. */
    remove: (command: string) => Promise<unknown>;
    onsettings: () => void;
  };

  let { items, recent, onrun, onclose, history, initial = "" }: {
    items: PaletteItem[];
    /** Ids of recently chosen items, newest first. */
    recent: string[];
    onrun: (item: PaletteItem) => void;
    onclose: () => void;
    history?: HistorySource;
    /** Text to open with, such as "$ " for history. */
    initial?: string;
  } = $props();

  // svelte-ignore state_referenced_locally
  let raw = $state(initial);
  let sel = $state(0);
  let listEl: HTMLDivElement | undefined = $state();

  const modeGroup: Record<string, PaletteItem["group"]> = { ">": "Actions", "@": "Sessions", "!": "Snippets", "/": "Tabs" };
  const modeHelp = $derived([
    [">", "actions"],
    ["@", "sessions"],
    ["!", "snippets"],
    ["/", "tabs"],
    ...(history ? [["$", "history"]] : []),
  ]);
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

  // ---- history ($ mode) ----
  const histMode = $derived(parsed.mode === "$" && !!history);
  let hist = $state<HistoryEntry[] | null>(null);
  let histError = $state<string | null>(null);
  let filter = $state<HistoryFilter>({ ...noFilter });
  $effect(() => {
    if (histMode && hist === null && history) {
      hist = [];
      history
        .load()
        .then((h) => (hist = h))
        .catch((e) => (histError = `${e}`));
    }
  });
  const hosts = $derived(hostsOf(hist ?? []));
  const hrows = $derived.by((): HistoryRow[] => {
    if (!histMode || !hist) return [];
    const rows = collapse(applyFilter(hist, filter, Date.now()));
    const q = parsed.query.trim();
    // Only the command text: scattered letters in a long folder path would match almost anything.
    return (q ? rank(rows, q, (r) => [r.entry.command]) : rows).slice(0, 200);
  });
  const count = $derived(histMode ? hrows.length : results.length);
  const ranges: [Range, string][] = [
    ["all", "Any time"],
    ["today", "Today"],
    ["week", "This week"],
  ];
  function insert(row: HistoryRow | undefined) {
    if (row && history) history.insert(row.entry.command);
  }
  async function forget(row: HistoryRow | undefined) {
    if (!row || !history || !hist) return;
    try {
      await history.remove(row.entry.command);
      hist = hist.filter((e) => e.command !== row.entry.command);
      sel = Math.min(sel, Math.max(0, hrows.length - 1));
    } catch (e) {
      histError = `${e}`;
    }
  }
  const failedExit = (e: HistoryEntry) => e.exit !== null && e.exit !== 0;

  $effect(() => {
    results;
    hrows.length;
    filter.host;
    filter.failed;
    filter.range;
    sel = 0;
  });
  $effect(() => {
    const el = listEl?.querySelector<HTMLElement>(`[data-i="${sel}"]`);
    el?.scrollIntoView({ block: "nearest" });
  });

  onMount(() => onEscape(() => onclose()));

  function onkey(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || (e.ctrlKey && e.key === "n")) {
      e.preventDefault();
      sel = count ? (sel + 1) % count : 0;
    } else if (e.key === "ArrowUp" || (e.ctrlKey && e.key === "p")) {
      e.preventDefault();
      sel = count ? (sel - 1 + count) % count : 0;
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (histMode) insert(hrows[sel]);
      else {
        const item = results[sel];
        if (item) onrun(item);
      }
    } else if (histMode && e.key === "Delete" && e.shiftKey) {
      // Shift+Delete forgets the selected command (plain Delete still edits the query).
      e.preventDefault();
      forget(hrows[sel]);
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
  <div use:modal class="pal" role="dialog" aria-label="Command palette">
    <div class="box">
      <Icon name="search" size={16} />
      <input
        use:focus
        bind:value={raw}
        onkeydown={onkey}
        role="combobox"
        aria-expanded="true"
        aria-controls="pal-list"
        aria-activedescendant={sel < count ? `pal-${sel}` : undefined}
        placeholder={histMode ? "Search command history" : "Search actions, sessions, snippets and tabs"}
        aria-label="Command palette"
        autocapitalize="off"
        spellcheck="false"
      />
    </div>
    {#if histMode}
      <div class="filters" role="group" aria-label="History filters">
        <select bind:value={filter.host} aria-label="Host">
          <option value={null}>All hosts</option>
          {#each hosts as h (h)}<option value={h}>{h}</option>{/each}
        </select>
        <div class="seg" role="radiogroup" aria-label="When">
          {#each ranges as [r, label] (r)}
            <button role="radio" aria-checked={filter.range === r} class:on={filter.range === r} onclick={() => (filter.range = r)}>{label}</button>
          {/each}
        </div>
        <button class="chip" class:on={filter.failed} aria-pressed={filter.failed} onclick={() => (filter.failed = !filter.failed)}>Failed only</button>
      </div>
    {/if}
    <div class="list" id="pal-list" role="listbox" bind:this={listEl}>
      {#if histMode}
        {#each hrows as row, i (row.entry.command)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            id="pal-{i}"
            data-i={i}
            class="row hrow"
            class:sel={i === sel}
            role="option"
            aria-selected={i === sel}
            tabindex="-1"
            onmousemove={() => (sel = i)}
            onclick={() => insert(row)}
          >
            <span class="ic" class:bad={failedExit(row.entry)} title={row.entry.exit === null ? "Exit code unknown" : `Exit ${row.entry.exit}`}>
              <Icon name={failedExit(row.entry) ? "x" : "terminal"} size={15} />
            </span>
            <span class="cmd">{row.entry.command.split("\n")[0]}</span>
            <span class="s">
              {row.hosts.join(", ")}{row.entry.cwd ? `  ·  ${row.entry.cwd}` : ""}  ·  {ago(row.entry.ts, Date.now())}{row.count > 1 ? `  ·  ${row.count}×` : ""}
            </span>
            <button class="del" title="Forget this command (Shift+Delete)" aria-label="Forget this command" onclick={(e) => (e.stopPropagation(), forget(row))}>
              <Icon name="trash" size={13} />
            </button>
          </div>
        {:else}
          <div class="none">
            {#if histError}
              Could not read history: {histError}
            {:else if hist === null || (hist.length === 0 && !history?.recording)}
              Command history is off. <button class="link" onclick={() => history?.onsettings()}>Turn it on in Settings</button>
            {:else if hist.length === 0}
              No commands saved yet. Commands are saved when they finish in a shell with command marks.
            {:else}
              Nothing matches{parsed.query.trim() ? ` "${parsed.query}"` : " these filters"}.
            {/if}
          </div>
        {/each}
      {:else}
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
      {/if}
    </div>
    <div class="foot">
      {#if histMode}
        <span><kbd>{formatShortcut("Shift+Delete")}</kbd> forget</span>
        <span class="sp"><kbd>↑↓</kbd> move <kbd>↵</kbd> type at prompt <kbd>esc</kbd> close</span>
      {:else}
        {#each modeHelp as [k, label] (k)}<span><kbd>{k}</kbd> {label}</span>{/each}
        <span class="sp"><kbd>↑↓</kbd> move <kbd>↵</kbd> run <kbd>esc</kbd> close</span>
      {/if}
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
    color: var(--on-accent);
  }
  .ic {
    display: flex;
    flex: none;
    color: var(--dim);
  }
  /* On the selected row everything takes the full on-accent colour: a faded one fails on the accent fill. */
  .row.sel .ic,
  .row.sel .s,
  .row.sel .g {
    color: inherit;
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
    color: var(--dim);
  }
  .g {
    margin-left: auto;
    font-size: 11px;
    color: var(--dim);
  }
  kbd {
    flex: none;
    margin-left: auto;
    padding: 1px 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    font: inherit;
    font-size: 12px;
    letter-spacing: 0.04em;
  }
  .row.sel kbd {
    border-color: currentColor;
  }
  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border);
    font-size: 12px;
  }
  .filters select,
  .filters button {
    font: inherit;
    color: var(--fg);
    background: none;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    padding: 3px 8px;
    cursor: pointer;
  }
  .filters select {
    max-width: 200px;
    background: var(--surface);
  }
  .seg {
    display: flex;
  }
  .seg button {
    border-radius: 0;
    margin-left: -1px;
  }
  .seg button:first-child {
    border-radius: 6px 0 0 6px;
    margin-left: 0;
  }
  .seg button:last-child {
    border-radius: 0 6px 6px 0;
  }
  .filters .on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--on-accent);
  }
  .cmd {
    flex: none;
    max-width: 60%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: 13px;
  }
  .ic.bad {
    color: var(--red-text);
  }
  .row.sel .ic.bad {
    color: inherit;
  }
  .del {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 24px;
    min-height: 24px;
    margin-left: auto;
    padding: 2px;
    border: 0;
    border-radius: 4px;
    background: none;
    color: inherit;
    opacity: 0;
    cursor: pointer;
  }
  .row.sel .del,
  .del:focus-visible {
    opacity: 0.8;
  }
  .del:hover {
    opacity: 1;
    background: var(--hover);
  }
  .link {
    font: inherit;
    color: var(--accent-text);
    background: none;
    border: 0;
    padding: 0;
    text-decoration: underline;
    cursor: pointer;
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
