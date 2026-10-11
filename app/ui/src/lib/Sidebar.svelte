<script lang="ts">
  import { focus } from "./focus";
  import type { Folder } from "./bindings/Folder";
  import type { LibraryView } from "./bindings/LibraryView";
  import type { SavedSession } from "./bindings/SavedSession";

  let {
    library,
    query = $bindable(""),
    notice = null,
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
  <div class="item session" data-drop={s.folder_id ?? "root"}>
    <button
      class="main"
      title={`${s.user}@${s.host}:${s.port}`}
      onpointerdown={(e) => pressed(e, { kind: "session", id: s.id, label: s.name })}
      ondblclick={() => onconnect(s)}
      onkeydown={(e) => e.key === "Enter" && onconnect(s)}
    >
      <span class="dot" style:background={s.color ?? "transparent"}></span>
      <span class="name">{s.name}</span>
      {#each s.tags as t, i (i)}<span class="tag">{t}</span>{/each}
    </button>
    <span class="acts">
      <button title="Connect" aria-label={`Connect to ${s.name}`} onclick={() => onconnect(s)}>▶</button>
      <button title="Edit" aria-label={`Edit ${s.name}`} onclick={() => onedit(s)}>✎</button>
      <button title="Delete" aria-label={`Delete ${s.name}`} onclick={() => confirmDelete(s)}>×</button>
    </span>
  </div>
{/snippet}

{#snippet tree(parent: number | null)}
  {#each childrenOf(parent) as f (f.id)}
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
          <span class="caret">{collapsed[f.id] ? "▸" : "▾"}</span>
          <span class="name">{f.name}</span>
        </button>
        <span class="acts">
          <button title="New session in this folder" aria-label={`New session in ${f.name}`} onclick={() => onnew(f.id)}>+</button>
          <button title="New subfolder" aria-label={`New subfolder in ${f.name}`} onclick={() => startCreate(f.id)}>🗀</button>
          <button title="Rename" aria-label={`Rename ${f.name}`} onclick={() => startRename(f.id, f.name)}>✎</button>
          <button
            title="Delete folder"
            aria-label={`Delete folder ${f.name}`}
            onclick={() => confirm(`Delete folder "${f.name}"? Its sessions and subfolders are kept.`) && ondeletefolder(f.id)}>×</button>
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
  {/each}
{/snippet}

<aside class:dragging={drag !== null}>
  <div class="bar">
    <input class="search" type="search" placeholder="Search sessions" bind:value={query} aria-label="Search sessions" />
    <button title="New session" aria-label="New session" onclick={() => onnew(null)}>+</button>
    <button title="New folder" aria-label="New folder" onclick={() => startCreate(null)}>🗀</button>
    <button title="Import from ~/.ssh/config" aria-label="Import from SSH config" onclick={onimport}>⇩</button>
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
      {#each sessionsIn(null) as s (s.id)}{@render sessionRow(s)}{/each}
      {#if library.sessions.length === 0 && library.folders.length === 0}
        <p class="empty">No saved sessions yet. Press + to add one, or import your ~/.ssh/config.</p>
      {/if}
    {/if}
  </div>
</aside>

{#if drag}
  <div class="ghost" style:left={`${dragPos.x + 12}px`} style:top={`${dragPos.y + 12}px`}>{drag.label}</div>
{/if}

<style>
  aside {
    display: flex;
    flex-direction: column;
    width: 250px;
    flex: none;
    background: #1a1b20;
    border-right: 1px solid #2b2d36;
    font-size: 13px;
    min-height: 0;
  }
  .bar {
    display: flex;
    gap: 4px;
    padding: 6px;
  }
  .search {
    flex: 1;
    min-width: 0;
    padding: 5px 8px;
    font: inherit;
    color: inherit;
    border: 1px solid #33353e;
    border-radius: 5px;
    background: #111216;
  }
  button {
    font: inherit;
    color: inherit;
    background: none;
    border: 0;
    cursor: pointer;
  }
  .bar button {
    padding: 2px 6px;
    opacity: 0.7;
  }
  .bar button:hover {
    opacity: 1;
  }
  .notice {
    margin: 0 8px 4px;
    font-size: 12px;
    opacity: 0.8;
  }
  .list {
    flex: 1;
    overflow: auto;
    padding-bottom: 8px;
  }
  .item {
    display: flex;
    align-items: center;
    padding-right: 4px;
    border-radius: 4px;
  }
  .item:hover {
    background: #25262d;
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 4px;
    text-align: left;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dot {
    width: 8px;
    height: 8px;
    flex: none;
    border-radius: 50%;
  }
  .tag {
    flex: none;
    padding: 0 5px;
    border-radius: 8px;
    background: #33353e;
    font-size: 11px;
    opacity: 0.8;
  }
  .caret {
    width: 10px;
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
    padding: 2px 5px;
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
    border: 1px solid #44464f;
    background: #111;
  }
  aside.dragging {
    user-select: none;
    cursor: grabbing;
  }
  .children {
    padding-left: 12px;
  }
  .item.drop,
  .children.drop {
    background: rgba(59, 111, 216, 0.25);
    outline: 1px solid #3b6fd8;
  }
  .list.drop {
    background: rgba(59, 111, 216, 0.12);
  }
  .ghost {
    position: fixed;
    z-index: 30;
    padding: 3px 8px;
    border-radius: 5px;
    background: #3b6fd8;
    font-size: 12px;
    pointer-events: none;
    opacity: 0.9;
  }
  .empty {
    margin: 12px;
    opacity: 0.5;
  }
</style>
