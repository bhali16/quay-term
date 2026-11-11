<script lang="ts">
  import { onMount } from "svelte";
  import { focus } from "./focus";
  import { onEscape } from "./escape";
  import { modal } from "./modal";
  import Icon from "./Icon.svelte";
  import type { Snippet } from "./bindings/Snippet";
  import { fill, group, initialValues, matches, slug, variables } from "./snippets";

  export type Target = "focused" | "tab" | "broadcast";
  let {
    snippets,
    initial = null,
    draft: startDraft = null,
    session = null,
    tabPanes,
    broadcast,
    onsend,
    onsave,
    onremember,
    ondelete,
    onclose,
  }: {
    snippets: Snippet[];
    /** Open straight into this snippet's fill form (chosen from the command palette). */
    initial?: Snippet | null;
    /** Open straight into a new snippet with this text (saved from a command block). */
    draft?: { name: string; body: string } | null;
    /** The saved session of the focused pane, whose remembered values fill the form. */
    session?: { id: number; label: string } | null;
    /** Panes in the current tab. */
    tabPanes: number;
    /** Panes currently selected for broadcast. */
    broadcast: number;
    /** `text` is filled in; `enter` says whether to run it. The pane decides how to type it. */
    onsend: (text: string, enter: boolean, target: Target) => void;
    onsave: (id: number | null, name: string, folder: string, body: string) => Promise<string | null>;
    /** Remember values for one session; empty values forget them. */
    onremember: (id: number, session: number, values: Record<string, string>) => Promise<void>;
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
  let remember = $state(false);
  let error = $state<string | null>(null);

  const shown = $derived(group(snippets.filter((s) => matches(s, query))));
  const folders = $derived([...new Set(snippets.map((s) => s.folder).filter(Boolean))].sort());
  const vars = $derived(current ? variables(current.body) : []);

  const remembered = (s: Snippet) => (session ? s.defaults[String(session.id)] : undefined);
  function pick(s: Snippet) {
    current = s;
    values = initialValues(s.body, remembered(s));
    remember = remembered(s) !== undefined;
    target = broadcast > 0 ? "broadcast" : "focused";
    mode = "fill";
  }
  let started = false;
  $effect.pre(() => {
    if (started) return;
    started = true;
    if (initial) pick(initial);
    else if (startDraft) edit(null, startDraft);
  });
  async function send() {
    if (!current) return;
    if (session && vars.length) {
      const want = remember ? Object.fromEntries(vars.map((v) => [v.name, values[v.name] ?? ""])) : {};
      const had = remembered(current) ?? {};
      if (JSON.stringify(want) !== JSON.stringify(had)) {
        try {
          await onremember(current.id, session.id, want);
        } catch {
          /* the snippet still goes out; remembering is a convenience */
        }
      }
    }
    onsend(fill(current.body, values), enter, target);
  }
  function edit(s: Snippet | null, from: { name: string; body: string } | null = null) {
    current = s;
    draft = { name: s?.name ?? from?.name ?? "", folder: s?.folder ?? "", body: s?.body ?? from?.body ?? "" };
    error = null;
    mode = "edit";
  }
  async function save() {
    error = await onsave(current?.id ?? null, draft.name, draft.folder, draft.body);
    if (!error) mode = "list";
  }
  /** Take focus a frame after the form appears, so a terminal that opened it (`;name` + Tab) cannot keep it. */
  function focusIf(node: HTMLElement, on: boolean) {
    if (on) requestAnimationFrame(() => node.focus());
  }
  onMount(() => onEscape(() => (mode === "list" ? onclose() : (mode = "list"))));
</script>

<div class="dlg-backdrop" role="presentation">
  <div use:modal class="dlg snip" role="dialog" aria-label="Snippets">
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
                <span class="nm">{s.name}<span class="short">;{slug(s.name)}</span></span>
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
      {#each vars as v, i (v.name)}
        <label>{v.name}
          {#if v.choices.length}
            <select bind:value={values[v.name]} use:focusIf={i === 0}>
              {#each v.choices as c (c)}<option value={c}>{c}</option>{/each}
              {#if values[v.name] && !v.choices.includes(values[v.name])}<option value={values[v.name]}>{values[v.name]}</option>{/if}
            </select>
          {:else}
            <input bind:value={values[v.name]} use:focusIf={i === 0} onkeydown={(e) => e.key === "Enter" && (e.preventDefault(), send())} />
          {/if}
        </label>
      {/each}
      {#if session && vars.length}
        <label class="check"><input type="checkbox" bind:checked={remember} /> Remember these values for {session.label}</label>
      {/if}
      <label>Send to
        <select bind:value={target}>
          <option value="focused">The focused pane</option>
          <option value="tab" disabled={tabPanes < 2}>All {tabPanes} panes of this tab</option>
          <option value="broadcast" disabled={broadcast === 0}>The {broadcast} broadcast pane{broadcast === 1 ? "" : "s"}</option>
        </select>
      </label>
      <label class="check"><input type="checkbox" bind:checked={enter} /> Run it (press Enter after the text). Off types it into the prompt to check first</label>
      {#if target !== "focused"}<p class="note"><Icon name="info" size={14} /><span>This types into several terminals at once.</span></p>{/if}
      <div class="actions">
        <button type="button" onclick={() => (mode = "list")}>Back</button>
        <button type="button" class="primary" onclick={send} use:focusIf={vars.length === 0}>Send</button>
      </div>
    {:else}
      <h2>{current ? "Edit snippet" : startDraft ? "Save command as snippet" : "New snippet"}</h2>
      <form onsubmit={(e) => (e.preventDefault(), save())}>
        <label>Name<input use:focus bind:value={draft.name} /></label>
        <label>Folder (optional, "a/b" nests)<input bind:value={draft.folder} list="snippet-folders" /></label>
        <datalist id="snippet-folders">{#each folders as f}<option value={f}></option>{/each}</datalist>
        <label>Text to send
          <textarea class="mono" rows="5" bind:value={draft.body} spellcheck="false"></textarea>
        </label>
        <p class="hint">
          Write <code>{"{{name}}"}</code> for a part you fill in when sending, <code>{"{{name|default}}"}</code> to suggest a value,
          or <code>{"{{name|dev|staging|prod}}"}</code> to pick from a list. <code>{"\\{{"}</code> sends a plain <code>{"{{"}</code>.
          Each line is sent as if you pressed Enter after it.
        </p>
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
  .short {
    margin-left: 8px;
    color: var(--dim);
    font-family: var(--font-mono);
    font-size: 11.5px;
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
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--input);
    color: var(--fg);
    font: inherit;
    font-size: 12.5px;
  }
</style>
