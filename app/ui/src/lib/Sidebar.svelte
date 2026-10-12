<script lang="ts">
  import { focus } from "./focus";
  import Icon from "./Icon.svelte";
  import type { Folder } from "./bindings/Folder";
  import type { LibraryView } from "./bindings/LibraryView";
  import type { SavedSession } from "./bindings/SavedSession";

  let {
    library,
    query = $bindable(""),
    version = "",
    railOnly = false,
    openIds = [],
    activeId = null,
    oncollapse,
    onshell,
    onssh,
    shellKey = "",
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
    version?: string;
    railOnly?: boolean;
    /** Saved-session ids that have a tab open, and the one in the front tab. */
    openIds?: number[];
    activeId?: number | null;
    oncollapse: () => void;
    /** New tab: a local shell, or an SSH connection (opens the connect dialog). */
    onshell: () => void;
    onssh: () => void;
    /** Shortcut shown in the shell button's tooltip. */
    shellKey?: string;
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

  /** Short label for a host's rail button. A trailing number is what tells
   *  "web-01" from "web-02", so it is kept whole instead of its leading zero. */
  const initials = (name: string) => {
    const words = name.split(/[^\p{L}\p{N}]+/u).filter(Boolean);
    if (words.length < 2) return (words[0] ?? name).slice(0, 2).toUpperCase();
    const second = /^\d+$/.test(words[1]) ? words[1].replace(/^0+(?=\d)/, "").slice(-2) : words[1][0];
    return (words[0][0] + second).toUpperCase();
  };

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
      <span class="sicon" style:--c={s.color ?? "var(--accent)"}><Icon name="server" size={15} /></span>
      <span class="name">{s.name}</span>
      <span class="meta">
        {#if s.tags.length > 0}
          <span class="tags">
            <span class="tag" title={s.tags.join(", ")}>{s.tags[0]}</span>
            {#if s.tags.length > 1}<span class="tag more" title={s.tags.join(", ")}>+{s.tags.length - 1}</span>{/if}
          </span>
        {/if}
        {#if openIds.includes(s.id)}<span class="live" title="Open in a tab"></span>{/if}
      </span>
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

{#snippet newTabButtons(vertical: boolean)}
  <div class="dual" class:vertical role="group" aria-label="New tab">
    <button title={`New shell${shellKey ? ` (${shellKey})` : ""}`} aria-label="New local shell" onclick={onshell}><Icon name="terminal" size={15} /></button>
    <button title="New SSH connection…" aria-label="New SSH connection" onclick={onssh}><Icon name="server" size={15} /></button>
  </div>
{/snippet}

<div class="sb" class:rail={railOnly}>
<!-- Room for the window's traffic lights and the toggle that App draws over them. -->
<div class="head" data-tauri-drag-region>
  {#if !railOnly}{@render newTabButtons(false)}{/if}
</div>
{#if railOnly}
  <aside class="rail">
    {@render newTabButtons(true)}
    <div class="railtools">
      <button title="Search sessions" aria-label="Search sessions" onclick={oncollapse}><Icon name="search" size={17} /></button>
      <button title="New session" aria-label="New session" onclick={() => onnew(null)}><Icon name="plus" size={17} /></button>
      <button title="New folder" aria-label="New folder" onclick={() => (oncollapse(), startCreate(null))}><Icon name="folder-plus" size={17} /></button>
      <button title="Import hosts from ~/.ssh/config" aria-label="Import from SSH config" onclick={onimport}><Icon name="import" size={17} /></button>
    </div>
    <div class="raildivider"></div>
    <div class="railhosts">
      {#each library.sessions as s (s.id)}
        <button
          class="host"
          class:current={activeId === s.id}
          style:--c={s.color ?? "var(--accent)"}
          title={`${s.name}\n${s.user}@${s.host}`}
          aria-label={`Connect to ${s.name}`}
          onclick={() => onconnect(s)}
        >
          {initials(s.name)}
          {#if openIds.includes(s.id)}<span class="live" title="Open in a tab"></span>{/if}
        </button>
      {/each}
    </div>
  </aside>
{:else}
<aside class:dragging={drag !== null}>
  <div class="searchbox"><Icon name="search" size={14} /><input class="search" type="search" placeholder="Search sessions" bind:value={query} aria-label="Search sessions" /></div>
  <div class="tools">
    <button title="New session" aria-label="New session" onclick={() => onnew(null)}><Icon name="plus" size={13} />Session</button>
    <button title="New folder" aria-label="New folder" onclick={() => startCreate(null)}><Icon name="plus" size={13} />Folder</button>
    <button title="Import hosts from ~/.ssh/config" aria-label="Import from SSH config" onclick={onimport}><Icon name="import" size={13} />Import</button>
  </div>
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
  {#if version}<div class="footer"><span class="version">{version}</span></div>{/if}
</aside>
{/if}
</div>

{#if drag}
  <div class="ghost" style:left={`${dragPos.x + 12}px`} style:top={`${dragPos.y + 12}px`}>{drag.label}</div>
{/if}

<style>
  /* The wrapper animates 260 -> 48; the asides inside keep a fixed width so
     their content is clipped, not reflowed, while it moves. */
  .sb {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: none;
    box-sizing: border-box;
    width: var(--sidebar-w);
    overflow: hidden;
    background: var(--bg-alt);
    transition: width 160ms ease-out;
  }
  .sb.rail {
    width: var(--rail-w);
  }
  /* The divider is drawn here, not as a border: in the rail it starts below the header,
     because the traffic lights straddle the rail's edge and a line must not run between them. */
  .sb::after {
    content: "";
    position: absolute;
    inset: 0 0 0 auto;
    width: 1px;
    background: var(--border);
    pointer-events: none;
  }
  .sb.rail::after {
    top: var(--chrome-h);
  }
  @media (prefers-reduced-motion: reduce) {
    .sb {
      transition: none;
    }
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    flex: none;
    height: var(--chrome-h);
    padding: 0 12px;
  }
  /* New tab: a local shell or an SSH connection. Recessed pair, the hovered half rises. */
  .dual {
    display: flex;
    gap: 2px;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--bg-deep);
  }
  .dual button {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 24px;
    border-radius: 7px;
    color: var(--dim);
  }
  .dual button:hover {
    color: var(--fg);
    background: var(--surface);
    box-shadow: 0 0 0 1px var(--border);
  }
  .dual.vertical {
    flex-direction: column;
    flex: none;
  }
  .dual.vertical button {
    width: 32px;
  }
  aside {
    display: flex;
    flex-direction: column;
    flex: 1;
    gap: 10px;
    width: calc(var(--sidebar-w) - 1px);
    box-sizing: border-box;
    padding: 4px 12px 12px;
    font-size: 13px;
    min-height: 0;
  }
  .searchbox {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
    height: 34px;
    padding: 0 11px;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--input);
    color: var(--dim);
  }
  .searchbox:focus-within {
    border-color: var(--accent);
  }
  .search {
    flex: 1;
    min-width: 0;
    padding: 0;
    font: inherit;
    font-size: 13.5px;
    color: var(--fg);
    border: 0;
    outline: 0;
    background: none;
  }
  .search::placeholder {
    color: var(--dim);
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
    gap: 6px;
    flex: none;
  }
  .tools button {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 6px 4px;
    font-size: 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
  }
  .tools button:hover {
    background: var(--hover);
  }
  .ficon,
  .sicon {
    display: flex;
    flex: none;
  }
  /* Neutral, so a session's own colour is the only colour in the tree. */
  .ficon {
    color: var(--dim);
  }
  .sicon {
    color: var(--c);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    margin: 0 -4px;
    padding: 0 4px;
  }
  .item {
    display: flex;
    align-items: center;
    margin: 1px 0;
    padding-right: 4px;
    border-radius: 6px;
  }
  .item:hover,
  .item.current {
    background: var(--hover);
  }
  /* The open session gets an accent edge so it is not mistaken for a hover. */
  .item.current {
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .live {
    width: 7px;
    height: 7px;
    flex: none;
    border-radius: 50%;
    background: var(--green);
  }
  .sep {
    height: 1px;
    margin: 6px 8px;
    background: var(--border);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    text-align: left;
  }
  .name {
    min-width: 40px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Tags give way before the name does: a row must never lose its name. */
  .meta {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 0 1000 auto;
    min-width: 0;
    margin-left: auto;
  }
  .tags {
    display: flex;
    gap: 4px;
    min-width: 0;
    overflow: hidden;
  }
  .tag {
    flex: 0 1 auto;
    min-width: 0;
    max-width: 72px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 2px 8px;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--surface);
    color: var(--dim);
    font-size: 11px;
    line-height: 1.2;
  }
  .tag.more {
    flex: none;
  }
  .item:hover .tags,
  .item:focus-within .tags {
    display: none;
  }
  .caret {
    display: flex;
    width: 13px;
    margin-left: -2px;
    color: var(--dim);
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
    border-radius: 4px;
    background: var(--input);
  }
  aside.dragging {
    user-select: none;
    cursor: grabbing;
  }
  .children {
    margin-left: 16px;
  }
  .item.drop,
  .children.drop {
    background: color-mix(in srgb, var(--accent) 25%, transparent);
    outline: 1px solid var(--accent);
  }
  .list.drop {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .ghost {
    position: fixed;
    z-index: 30;
    padding: 3px 8px;
    border-radius: 5px;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 12px;
    pointer-events: none;
    opacity: 0.9;
  }
  .footer {
    flex: none;
    padding: 10px 4px 0;
    border-top: 1px solid var(--border);
  }
  .version {
    font-size: 11px;
    color: var(--dim);
  }
  .empty {
    margin: 12px 4px;
    color: var(--dim);
  }

  /* ---- collapsed rail ---- */
  aside.rail {
    align-items: center;
    gap: 6px;
    width: calc(var(--rail-w) - 1px);
    padding: 4px 0 10px;
  }
  .railtools {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }
  .railtools button {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 6px;
    color: var(--dim);
  }
  .railtools button:hover {
    color: var(--fg);
    background: var(--hover);
  }
  .raildivider {
    flex: none;
    width: 24px;
    height: 1px;
    background: var(--border);
  }
  .railhosts {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 2px 0;
    overflow-y: auto;
    scrollbar-width: none;
  }
  .host {
    position: relative;
    flex: none;
    width: 30px;
    height: 30px;
    border-radius: 8px;
    background: var(--c);
    color: #fff;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.3px;
  }
  .host:hover {
    filter: brightness(1.12);
  }
  .host.current {
    outline: 2px solid var(--fg);
    outline-offset: 2px;
  }
  .host .live {
    position: absolute;
    right: -3px;
    bottom: -3px;
    box-sizing: border-box;
    border: 2px solid var(--bg-alt);
    width: 11px;
    height: 11px;
  }
</style>
