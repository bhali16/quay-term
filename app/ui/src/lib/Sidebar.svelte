<script lang="ts">
  import { focus } from "./focus";
  import Icon from "./Icon.svelte";
  import type { Folder } from "./bindings/Folder";
  import type { LibraryView } from "./bindings/LibraryView";
  import type { SavedSession } from "./bindings/SavedSession";

  let {
    library,
    query = $bindable(""),
    notice = null,
    version = "",
    railOnly = false,
    openIds = [],
    activeId = null,
    oncollapse,
    onconnect,
    onedit,
    onnew,
    ondelete,
    onimport,
    onnewfolder,
    onrenamefolder,
    ondeletefolder,
    onmovesession,
    onmovefolder,
  }: {
    library: LibraryView;
    query: string;
    notice?: string | null;
    version?: string;
    railOnly?: boolean;
    /** Saved-session ids that have a tab open, and the one in the front tab. */
    openIds?: number[];
    activeId?: number | null;
    oncollapse: () => void;
    onconnect: (s: SavedSession) => void;
    onedit: (s: SavedSession) => void;
    /** `folderId` preselects a folder in the editor. */
    onnew: (folderId: number | null) => void;
    ondelete: (s: SavedSession) => void;
    onimport: () => void;
    onnewfolder: (name: string, parentId: number | null) => void;
    onrenamefolder: (id: number, name: string) => void;
    ondeletefolder: (id: number) => void;
    onmovesession: (id: number, folderId: number | null) => void;
    onmovefolder: (id: number, parentId: number | null) => void;
  } = $props();

  let collapsed = $state<Record<number, boolean>>({});
  /** Renaming a folder, or creating one under `parent` (null = top level). */
  let renaming = $state<number | null>(null);
  let creating = $state<{ parent: number | null } | null>(null);
  let folderName = $state("");

  const searching = $derived(query.trim() !== "");
  const childrenOf = (parent: number | null) => library.folders.filter((f) => f.parent_id === parent);
  /** Sessions in a folder and all of its subfolders. */
  const countIn = (folder: number): number =>
    sessionsIn(folder).length + childrenOf(folder).reduce((n, f) => n + countIn(f.id), 0);
  const sessionsIn = (folder: number | null) => library.sessions.filter((s) => s.folder_id === folder);

  function startRename(id: number, current: string) {
    creating = null;
    renaming = id;
    folderName = current;
  }
  function startCreate(parent: number | null) {
    renaming = null;
    creating = { parent };
    folderName = "";
    if (parent !== null) collapsed[parent] = false;
  }
  function commitFolder() {
    const name = folderName.trim();
    if (creating && name) onnewfolder(name, creating.parent);
    else if (renaming !== null && name) onrenamefolder(renaming, name);
    creating = null;
    renaming = null;
  }
  function cancelFolderEdit() {
    creating = null;
    renaming = null;
  }
  function confirmDelete(s: SavedSession) {
    if (confirm(`Delete "${s.name}"? Its saved password, if any, is removed too.`)) ondelete(s);
  }

  // ---- drag and drop (pointer events: works in every webview and does not
  // interfere with the OS file drop used for SFTP later) ----
  type Drag = { kind: "session" | "folder"; id: number; label: string };
  let drag = $state<Drag | null>(null);
  let dragPos = $state({ x: 0, y: 0 });
  /** Folder id, "root", or null when over nothing valid. */
  let dropTarget = $state<number | "root" | null>(null);
  let pending: { drag: Drag; x: number; y: number } | null = null;
  let justDragged = false;

  function isInside(candidate: number, ancestor: number): boolean {
    for (let cur: number | null = candidate; cur !== null; ) {
      if (cur === ancestor) return true;
      cur = library.folders.find((f) => f.id === cur)?.parent_id ?? null;
    }
    return false;
  }

  function pressed(e: PointerEvent, d: Drag) {
    if (e.button !== 0 || searching) return;
    pending = { drag: d, x: e.clientX, y: e.clientY };
    window.addEventListener("pointermove", moved);
    window.addEventListener("pointerup", released, { once: true });
    window.addEventListener("pointercancel", abort, { once: true });
  }
  function targetAt(x: number, y: number): number | "root" | null {
    const el = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-drop]");
    if (!el || !drag) return null;
    const t = el.dataset.drop === "root" ? "root" : Number(el.dataset.drop);
    if (drag.kind === "folder" && t !== "root" && isInside(t, drag.id)) return null;
    return t;
  }
  function moved(e: PointerEvent) {
    if (pending && !drag && Math.hypot(e.clientX - pending.x, e.clientY - pending.y) > 5) drag = pending.drag;
    if (drag) {
      dragPos = { x: e.clientX, y: e.clientY };
      dropTarget = targetAt(e.clientX, e.clientY);
    }
  }
  function finish() {
    window.removeEventListener("pointermove", moved);
    window.removeEventListener("pointerup", released);
    window.removeEventListener("pointercancel", abort);
    pending = null;
    drag = null;
    dropTarget = null;
  }
  function released() {
    const d = drag;
    const t = dropTarget;
    if (d) {
      justDragged = true;
      setTimeout(() => (justDragged = false), 0);
    }
    finish();
    if (!d || t === null) return;
    const folderId = t === "root" ? null : t;
    if (folderId !== null) collapsed[folderId] = false;
    if (d.kind === "session") {
      if (library.sessions.find((s) => s.id === d.id)?.folder_id !== folderId) onmovesession(d.id, folderId);
    } else if (library.folders.find((f) => f.id === d.id)?.parent_id !== folderId) {
      onmovefolder(d.id, folderId);
    }
  }
  function abort() {
    finish();
  }
</script>

{#snippet folderInput()}
  <div class="item folder">
    <input
      use:focus
      placeholder="Folder name"
      bind:value={folderName}
      onblur={commitFolder}
      onkeydown={(e) => (e.key === "Enter" && commitFolder()) || (e.key === "Escape" && cancelFolderEdit())}
    />
  </div>
{/snippet}

{#snippet sessionRow(s: SavedSession)}
  <div class="item session" class:current={activeId === s.id} data-drop={s.folder_id ?? "root"}>
    <button
      class="main"
      title={`${s.user}@${s.host}:${s.port}`}
      onpointerdown={(e) => pressed(e, { kind: "session", id: s.id, label: s.name })}
      ondblclick={() => onconnect(s)}
      onkeydown={(e) => e.key === "Enter" && onconnect(s)}
    >
      <span class="sicon" style:--c={s.color ?? "var(--accent)"}><Icon name="terminal" size={10} /></span>
      <span class="name">{s.name}</span>
      {#each s.tags as t, i (i)}<span class="tag">{t}</span>{/each}
      {#if openIds.includes(s.id)}<span class="live" title="Open in a tab"></span>{/if}
    </button>
    <span class="acts">
      <button title="Connect" aria-label={`Connect to ${s.name}`} onclick={() => onconnect(s)}><Icon name="play" size={13} /></button>
      <button title="Edit" aria-label={`Edit ${s.name}`} onclick={() => onedit(s)}><Icon name="pencil" size={13} /></button>
      <button title="Delete" aria-label={`Delete ${s.name}`} onclick={() => confirmDelete(s)}><Icon name="x" size={14} /></button>
    </span>
  </div>
{/snippet}

{#snippet tree(parent: number | null)}
  {#each childrenOf(parent) as f, i (f.id)}
    <div class="item folder" class:drop={dropTarget === f.id} data-drop={f.id}>
      {#if renaming === f.id}
        <input
          use:focus
          bind:value={folderName}
          onblur={commitFolder}
          onkeydown={(e) => (e.key === "Enter" && commitFolder()) || (e.key === "Escape" && cancelFolderEdit())}
        />
      {:else}
        <button
          class="main"
          onpointerdown={(e) => pressed(e, { kind: "folder", id: f.id, label: f.name })}
          onclick={() => !justDragged && (collapsed[f.id] = !collapsed[f.id])}
        >
          <span class="caret"><Icon name={collapsed[f.id] ? "chevron-right" : "chevron-down"} size={13} /></span>
          <span class="ficon"><Icon name={collapsed[f.id] ? "folder" : "folder-open"} size={17} tint /></span>
          <span class="name">{f.name}</span>
          <span class="count" title="Sessions in this folder">{countIn(f.id)}</span>
        </button>
        <span class="acts">
          <button title="New session in this folder" aria-label={`New session in ${f.name}`} onclick={() => onnew(f.id)}><Icon name="plus" size={14} /></button>
          <button title="New subfolder" aria-label={`New subfolder in ${f.name}`} onclick={() => startCreate(f.id)}><Icon name="folder-plus" size={14} /></button>
          <button title="Rename" aria-label={`Rename ${f.name}`} onclick={() => startRename(f.id, f.name)}><Icon name="pencil" size={13} /></button>
          <button
            title="Delete folder"
            aria-label={`Delete folder ${f.name}`}
            onclick={() => confirm(`Delete folder "${f.name}"? Its sessions and subfolders are kept.`) && ondeletefolder(f.id)}><Icon name="x" size={14} /></button>
        </span>
      {/if}
    </div>
    {#if !collapsed[f.id]}
      <div class="children" class:drop={dropTarget === f.id} data-drop={f.id}>
        {#if creating?.parent === f.id}{@render folderInput()}{/if}
        {@render tree(f.id)}
        {#each sessionsIn(f.id) as s (s.id)}{@render sessionRow(s)}{/each}
      </div>
    {/if}
    {#if i < childrenOf(parent).length - 1}<div class="sep"></div>{/if}
  {/each}
{/snippet}

{#if railOnly}
  <aside class="rail">
    <div class="railtools">
      <button title="Search sessions" aria-label="Search sessions" onclick={oncollapse}><Icon name="search" size={17} /></button>
      <button title="New session" aria-label="New session" onclick={() => onnew(null)}><Icon name="plus" size={17} /></button>
      <button title="New folder" aria-label="New folder" onclick={() => (oncollapse(), startCreate(null))}><Icon name="folder-plus" size={17} /></button>
      <button title="Import hosts from ~/.ssh/config" aria-label="Import from SSH config" onclick={onimport}><Icon name="import" size={17} /></button>
    </div>
    <div class="spacer"></div>
    <div class="footer">
      <button title="Show sidebar" aria-label="Show sidebar" onclick={oncollapse}><Icon name="panel-left" size={16} /></button>
    </div>
  </aside>
{:else}
<aside class:dragging={drag !== null}>
  <div class="bar">
    <div class="searchbox"><Icon name="search" size={14} /><input class="search" type="search" placeholder="Search sessions" bind:value={query} aria-label="Search sessions" /></div>
  </div>
  <div class="tools">
    <button title="New session" aria-label="New session" onclick={() => onnew(null)}><Icon name="plus" size={14} />Session</button>
    <button title="New folder" aria-label="New folder" onclick={() => startCreate(null)}><Icon name="folder-plus" size={15} />Folder</button>
    <button title="Import hosts from ~/.ssh/config" aria-label="Import from SSH config" onclick={onimport}><Icon name="import" size={14} />Import</button>
  </div>
  {#if notice}<p class="notice">{notice}</p>{/if}
  <div class="list" class:drop={dropTarget === "root"} data-drop="root">
    {#if creating?.parent === null}{@render folderInput()}{/if}
    {#if searching}
      {#each library.sessions as s (s.id)}{@render sessionRow(s)}{:else}
        <p class="empty">No sessions match.</p>
      {/each}
    {:else}
      {@render tree(null)}
      {#if childrenOf(null).length > 0 && sessionsIn(null).length > 0}<div class="sep"></div>{/if}
      {#each sessionsIn(null) as s (s.id)}{@render sessionRow(s)}{/each}
      {#if library.sessions.length === 0 && library.folders.length === 0}
        <p class="empty">No saved sessions yet. Press + to add one, or import your ~/.ssh/config.</p>
      {/if}
    {/if}
  </div>
  <div class="footer">
    <button title="Hide sidebar" aria-label="Hide sidebar" onclick={oncollapse}><Icon name="panel-left" size={16} /></button>
    <span class="version">{version}</span>
  </div>
</aside>
{/if}

{#if drag}
  <div class="ghost" style:left={`${dragPos.x + 12}px`} style:top={`${dragPos.y + 12}px`}>{drag.label}</div>
{/if}

<style>
  aside {
    display: flex;
    flex-direction: column;
    width: 250px;
    flex: none;
    background: var(--bg-alt);
    border-right: 1px solid var(--surface);
    font-size: 13px;
    min-height: 0;
  }
  .bar {
    display: flex;
    gap: 4px;
    padding: 6px;
  }
  .searchbox {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-deep);
    opacity: 0.9;
  }
  .search {
    flex: 1;
    min-width: 0;
    padding: 6px 0;
    font: inherit;
    color: inherit;
    border: 0;
    outline: 0;
    background: none;
  }
  button {
    font: inherit;
    color: inherit;
    background: none;
    border: 0;
    cursor: pointer;
  }
  .tools {
    display: flex;
    gap: 4px;
    padding: 0 6px 8px;
    border-bottom: 1px solid var(--border);
  }
  .tools button {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 5px 4px;
    font-size: 12px;
    border: 1px solid var(--border);
    border-radius: 5px;
  }
  .tools button:hover {
    background: var(--hover);
  }
  .ficon,
  .sicon {
    display: flex;
    flex: none;
  }
  .ficon {
    color: var(--accent);
  }
  .sicon {
    align-items: center;
    justify-content: center;
    width: 17px;
    height: 17px;
    border-radius: 5px;
    color: #fff;
    background: var(--c);
  }
  .notice {
    margin: 0 8px 4px;
    font-size: 12px;
    opacity: 0.8;
  }
  .list {
    flex: 1;
    overflow: auto;
    padding: 6px 0 8px;
  }
  .item {
    display: flex;
    align-items: center;
    min-height: 30px;
    margin: 1px 6px;
    padding-right: 4px;
    border-radius: 6px;
  }
  .item:hover {
    background: var(--hover);
  }
  .item.current {
    background: var(--hover);
    box-shadow: inset 0 0 0 1px var(--border);
  }
  .live {
    width: 7px;
    height: 7px;
    margin-left: auto;
    flex: none;
    border-radius: 50%;
    background: #2f9e6b;
  }
  .count {
    margin-left: auto;
    padding: 0 6px;
    border-radius: 5px;
    background: var(--bg-deep);
    font-size: 11px;
    opacity: 0.7;
  }
  .item:hover .count,
  .item:focus-within .count {
    display: none;
  }
  .sep {
    height: 1px;
    margin: 6px 12px;
    background: var(--border);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 6px;
    text-align: left;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tag {
    flex: none;
    padding: 0 5px;
    border-radius: 8px;
    background: var(--hover);
    font-size: 11px;
    opacity: 0.8;
  }
  .caret {
    display: flex;
    width: 13px;
    margin-left: -2px;
    opacity: 0.6;
  }
  .acts {
    display: none;
    flex: none;
  }
  .item:hover .acts,
  .item:focus-within .acts {
    display: flex;
  }
  .acts button {
    display: flex;
    padding: 3px 5px;
    opacity: 0.7;
  }
  .acts button:hover {
    opacity: 1;
  }
  .item input {
    flex: 1;
    min-width: 0;
    margin: 3px 4px;
    padding: 2px 4px;
    font: inherit;
    color: inherit;
    border: 1px solid var(--border);
    background: var(--input);
  }
  aside.dragging {
    user-select: none;
    cursor: grabbing;
  }
  .children {
    margin-left: 18px;
    padding-left: 2px;
    border-left: 1px solid var(--border);
  }
  .item.drop,
  .children.drop {
    background: rgba(59, 111, 216, 0.25);
    outline: 1px solid var(--accent);
  }
  .list.drop {
    background: rgba(59, 111, 216, 0.12);
  }
  .ghost {
    position: fixed;
    z-index: 30;
    padding: 3px 8px;
    border-radius: 5px;
    background: var(--accent);
    font-size: 12px;
    pointer-events: none;
    opacity: 0.9;
  }
  .footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px;
    border-top: 1px solid var(--surface);
  }
  aside.rail {
    width: 40px;
  }
  .railtools {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 8px 0;
  }
  .railtools button {
    display: flex;
    padding: 8px;
    border-radius: 6px;
    opacity: 0.7;
  }
  .railtools button:hover {
    opacity: 1;
    background: var(--hover);
  }
  .spacer {
    flex: 1;
  }
  .version {
    font-size: 11px;
    opacity: 0.5;
  }
  .footer button {
    display: flex;
    padding: 4px 6px;
    border-radius: 5px;
    opacity: 0.7;
  }
  .footer button:hover {
    opacity: 1;
    background: var(--hover);
  }
  .empty {
    margin: 12px;
    opacity: 0.5;
  }
</style>
