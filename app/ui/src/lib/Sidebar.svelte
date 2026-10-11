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
  }: {
    library: LibraryView;
    query: string;
    notice?: string | null;
    onconnect: (s: SavedSession) => void;
    onedit: (s: SavedSession) => void;
    onnew: () => void;
    ondelete: (s: SavedSession) => void;
    onimport: () => void;
    onnewfolder: (name: string) => void;
    onrenamefolder: (id: number, name: string) => void;
    ondeletefolder: (id: number) => void;
  } = $props();

  let collapsed = $state<Record<number, boolean>>({});
  let editingFolder = $state<number | "new" | null>(null);
  let folderName = $state("");

  const searching = $derived(query.trim() !== "");
  const childrenOf = (parent: number | null) => library.folders.filter((f) => f.parent_id === parent);
  const sessionsIn = (folder: number | null) => library.sessions.filter((s) => s.folder_id === folder);

  function startFolderEdit(target: number | "new", current = "") {
    editingFolder = target;
    folderName = current;
  }
  function commitFolder() {
    const name = folderName.trim();
    if (editingFolder === "new" && name) onnewfolder(name);
    else if (typeof editingFolder === "number" && name) onrenamefolder(editingFolder, name);
    editingFolder = null;
  }
  function confirmDelete(s: SavedSession) {
    if (confirm(`Delete "${s.name}"? Its saved password, if any, is removed too.`)) ondelete(s);
  }
</script>

{#snippet sessionRow(s: SavedSession)}
  <div class="item session">
    <button class="main" title={`${s.user}@${s.host}:${s.port}`} ondblclick={() => onconnect(s)} onkeydown={(e) => e.key === "Enter" && onconnect(s)}>
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

{#snippet tree(parent: number | null, depth: number)}
  {#each childrenOf(parent) as f (f.id)}
    <div class="item folder" style:padding-left={`${6 + depth * 12}px`}>
      {#if editingFolder === f.id}
        
        <input
          use:focus
          bind:value={folderName}
          onblur={commitFolder}
          onkeydown={(e) => (e.key === "Enter" && commitFolder()) || (e.key === "Escape" && (editingFolder = null))}
        />
      {:else}
        <button class="main" onclick={() => (collapsed[f.id] = !collapsed[f.id])}>
          <span class="caret">{collapsed[f.id] ? "▸" : "▾"}</span>
          <span class="name">{f.name}</span>
        </button>
        <span class="acts">
          <button title="Rename" aria-label={`Rename ${f.name}`} onclick={() => startFolderEdit(f.id, f.name)}>✎</button>
          <button
            title="Delete folder"
            aria-label={`Delete folder ${f.name}`}
            onclick={() => confirm(`Delete folder "${f.name}"? Its sessions are kept.`) && ondeletefolder(f.id)}>×</button>
        </span>
      {/if}
    </div>
    {#if !collapsed[f.id]}
      <div style:padding-left={`${12 + depth * 12}px`}>
        {@render tree(f.id, depth + 1)}
        {#each sessionsIn(f.id) as s (s.id)}{@render sessionRow(s)}{/each}
      </div>
    {/if}
  {/each}
{/snippet}

<aside>
  <div class="bar">
    <input class="search" type="search" placeholder="Search sessions" bind:value={query} aria-label="Search sessions" />
    <button title="New session" aria-label="New session" onclick={onnew}>+</button>
    <button title="New folder" aria-label="New folder" onclick={() => startFolderEdit("new")}>🗀</button>
    <button title="Import from ~/.ssh/config" aria-label="Import from SSH config" onclick={onimport}>⇩</button>
  </div>
  {#if notice}<p class="notice">{notice}</p>{/if}
  <div class="list">
    {#if editingFolder === "new"}
      <div class="item folder">
        
        <input
          use:focus
          placeholder="Folder name"
          bind:value={folderName}
          onblur={commitFolder}
          onkeydown={(e) => (e.key === "Enter" && commitFolder()) || (e.key === "Escape" && (editingFolder = null))}
        />
      </div>
    {/if}
    {#if searching}
      {#each library.sessions as s (s.id)}{@render sessionRow(s)}{:else}
        <p class="empty">No sessions match.</p>
      {/each}
    {:else}
      {@render tree(null, 0)}
      {#each sessionsIn(null) as s (s.id)}{@render sessionRow(s)}{/each}
      {#if library.sessions.length === 0 && library.folders.length === 0}
        <p class="empty">No saved sessions yet. Press + to add one, or import your ~/.ssh/config.</p>
      {/if}
    {/if}
  </div>
</aside>

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
  .empty {
    margin: 12px;
    opacity: 0.5;
  }
</style>
