<script lang="ts">
  import { focus } from "./focus";
  import Icon from "./Icon.svelte";
  import type { Snippet } from "./bindings/Snippet";
  import { fill, group, matches, toInput, variables } from "./snippets";

  export type Target = "focused" | "tab" | "broadcast";
  let {
    snippets,
    tabPanes,
    broadcast,
    onsend,
    onsave,
    ondelete,
    onclose,
  }: {
    snippets: Snippet[];
    /** Panes in the current tab. */
    tabPanes: number;
    /** Panes currently selected for broadcast. */
    broadcast: number;
    onsend: (text: string, target: Target) => void;
    onsave: (id: number | null, name: string, folder: string, body: string) => Promise<string | null>;
    ondelete: (s: Snippet) => void;
    onclose: () => void;
  } = $props();

  let query = $state("");
  let mode = $state<"list" | "fill" | "edit">("list");
  let current = $state<Snippet | null>(null);
  let values = $state<Record<string, string>>({});
  let enter = $state(true);
  let target = $state<Target>("focused");
  let draft = $state({ name: "", folder: "", body: "" });
  let error = $state<string | null>(null);

  const shown = $derived(group(snippets.filter((s) => matches(s, query))));
  const folders = $derived([...new Set(snippets.map((s) => s.folder).filter(Boolean))].sort());
  const vars = $derived(current ? variables(current.body) : []);

  function pick(s: Snippet) {
    current = s;
    values = Object.fromEntries(variables(s.body).map((v) => [v.name, v.fallback]));
    target = broadcast > 0 ? "broadcast" : "focused";
    mode = "fill";
  }
  function send() {
    if (!current) return;
    onsend(toInput(fill(current.body, values), enter), target);
  }
  function edit(s: Snippet | null) {
    current = s;
    draft = { name: s?.name ?? "", folder: s?.folder ?? "", body: s?.body ?? "" };
    error = null;
    mode = "edit";
  }
  async function save() {
    error = await onsave(current?.id ?? null, draft.name, draft.folder, draft.body);
    if (!error) mode = "list";
  }
  function onkey(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    if (mode === "list") onclose();
    else mode = "list";
  }
</script>

<svelte:window onkeydown={onkey} />

<div class="dlg-backdrop" role="presentation">
  <div class="dlg snip" role="dialog" aria-label="Snippets">
    {#if mode === "list"}
      <h2>Snippets</h2>
      <div class="search">
        <Icon name="search" size={15} />
        <input use:focus bind:value={query} placeholder="Search name, folder or text" aria-label="Search snippets" />
      </div>
      <div class="list">
        {#each shown as g (g.folder)}
          {#if g.folder}<div class="folder"><Icon name="folder" size={14} />{g.folder}</div>{/if}
          {#each g.snippets as s (s.id)}
            <div class="item">
              <button class="main" onclick={() => pick(s)} title="Send this snippet">
                <span class="nm">{s.name}</span>
                <code class="pv">{s.body.split("\n")[0]}</code>
              </button>
              <button class="ic" aria-label="Edit {s.name}" onclick={() => edit(s)}><Icon name="pencil" size={14} /></button>
              <button class="ic" aria-label="Delete {s.name}" onclick={() => ondelete(s)}><Icon name="trash" size={14} /></button>
            </div>
          {/each}
        {:else}
          <p class="hint">{snippets.length ? "No snippet matches." : "No snippets yet. A snippet is a command you send often; use {{name}} for a part that changes."}</p>
        {/each}
      </div>
      <div class="actions">
        <button type="button" onclick={() => edit(null)}>New snippet</button>
        <button type="button" class="primary" onclick={onclose}>Close</button>
      </div>
    {:else if mode === "fill" && current}
      <h2>{current.name}</h2>
      <pre class="body">{fill(current.body, values)}</pre>
      {#each vars as v (v.name)}
        <label>{v.name}<input bind:value={values[v.name]} onkeydown={(e) => e.key === "Enter" && (e.preventDefault(), send())} /></label>
      {/each}
      <label>Send to
        <select bind:value={target}>
          <option value="focused">The focused pane</option>
          <option value="tab" disabled={tabPanes < 2}>All {tabPanes} panes of this tab</option>
          <option value="broadcast" disabled={broadcast === 0}>The {broadcast} broadcast pane{broadcast === 1 ? "" : "s"}</option>
        </select>
      </label>
      <label class="check"><input type="checkbox" bind:checked={enter} /> Press Enter after the text</label>
      {#if target !== "focused"}<p class="note"><Icon name="info" size={14} /><span>This types into several terminals at once.</span></p>{/if}
      <div class="actions">
        <button type="button" onclick={() => (mode = "list")}>Back</button>
        <button type="button" class="primary" onclick={send}>Send</button>
      </div>
    {:else}
      <h2>{current ? "Edit snippet" : "New snippet"}</h2>
      <form onsubmit={(e) => (e.preventDefault(), save())}>
        <label>Name<input use:focus bind:value={draft.name} /></label>
        <label>Folder (optional, "a/b" nests)<input bind:value={draft.folder} list="snippet-folders" /></label>
        <datalist id="snippet-folders">{#each folders as f}<option value={f}></option>{/each}</datalist>
        <label>Text to send
          <textarea class="mono" rows="5" bind:value={draft.body} spellcheck="false"></textarea>
        </label>
        <p class="hint">Write <code>{"{{name}}"}</code> for a part you fill in when sending, or <code>{"{{name:default}}"}</code> to suggest a value. Each line is sent as if you pressed Enter after it.</p>
        {#if error}<p class="error" role="alert">{error}</p>{/if}
        <div class="actions">
          <button type="button" onclick={() => (mode = "list")}>Cancel</button>
          <button type="submit" class="primary">Save</button>
        </div>
      </form>
    {/if}
  </div>
</div>

<style>
  .snip {
    width: min(560px, calc(100vw - 48px));
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--dim);
  }
  .search input {
    flex: 1;
  }
  .list {
    max-height: 340px;
    overflow-y: auto;
    margin: 10px 0;
  }
  .folder {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 10px 0 4px;
    color: var(--dim);
    font-size: 12px;
  }
  .item {
    display: flex;
    align-items: center;
    border-radius: 6px;
  }
  .item:hover {
    background: var(--hover);
  }
  .main {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    padding: 6px 8px;
    text-align: left;
    cursor: pointer;
    background: none;
    border: 0;
    color: inherit;
    font: inherit;
  }
  .nm {
    font-size: 13.5px;
  }
  .pv {
    overflow: hidden;
    color: var(--dim);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ic {
    display: flex;
    padding: 6px;
    color: var(--dim);
    cursor: pointer;
    background: none;
    border: 0;
  }
  .ic:hover {
    color: var(--fg);
  }
  .body {
    max-height: 140px;
    overflow: auto;
    margin: 0 0 8px;
    padding: 8px 10px;
    border-radius: 6px;
    background: var(--input);
    font-size: 12.5px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  textarea {
    width: 100%;
    box-sizing: border-box;
    resize: vertical;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--input);
    color: var(--fg);
    font: inherit;
    font-size: 12.5px;
  }
</style>
