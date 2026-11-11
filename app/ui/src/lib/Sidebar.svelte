<script lang="ts">
  import type { Snippet } from "svelte";
  import { focus } from "./focus";
  import { inkOn } from "./themes";
  import Icon from "./Icon.svelte";
  import Button from "./Button.svelte";
  import Menu from "./Menu.svelte";
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
    toggleTitle = "",
    onshell,
    onssh,
    shellKey = "",
    paletteKey = "",
    onimport,
    onconnect,
    onsplit,
    onduplicate,
    onedit,
    onnew,
    ondelete,
    onnewfolder,
    onrenamefolder,
    ondeletefolder,
    onmovesession,
    onmovefolder,
    tabs,
  }: {
    /** The open tabs, listed above the sessions (vertical tab layout). Not shown in the rail. */
    tabs?: Snippet;
    library: LibraryView;
    query: string;
    version?: string;
    railOnly?: boolean;
    /** Saved-session ids that have a tab open, and the one in the front tab. */
    openIds?: number[];
    activeId?: number | null;
    oncollapse: () => void;
    /** Tooltip of the hide/show button at the bottom, with its shortcut. */
    toggleTitle?: string;
    /** New tab: a local shell, or an SSH connection (opens the connect dialog). */
    onshell: () => void;
    onssh: () => void;
    /** Shortcut shown in the shell button's tooltip. */
    shellKey?: string;
    /** Shortcut of the command palette, named in the empty state. */
    paletteKey?: string;
    /** Import hosts from ~/.ssh/config (offered while the list is empty). */
    onimport?: () => void;
    onconnect: (s: SavedSession) => void;
    /** Open the session as a new pane in the front tab. */
    onsplit?: (s: SavedSession) => void;
    onduplicate?: (s: SavedSession) => void;
    onedit: (s: SavedSession) => void;
    /** `folderId` preselects a folder in the editor. */
    onnew: (folderId: number | null) => void;
    ondelete: (s: SavedSession) => void;
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
  /** Nothing saved yet (and not just a search with no hits): the empty state does the prompting. */
  const blank = $derived(!searching && library.sessions.length === 0 && library.folders.length === 0);
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

  // ---- right-click menus on a session or a folder (also Shift+F10 and the menu key) ----
  type Ctx = { x: number; y: number; session?: SavedSession; folder?: Folder; moving?: boolean };
  let ctx = $state<Ctx | null>(null);
  function openCtx(e: MouseEvent, what: { session?: SavedSession; folder?: Folder }) {
    e.preventDefault();
    // Opened from the keyboard there is no pointer position: use the row.
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const byKey = e.clientX === 0 && e.clientY === 0;
    ctx = { x: byKey ? r.left + 24 : e.clientX, y: byKey ? r.bottom : e.clientY, ...what };
  }
  /** Run a menu choice, then close the menu (in that order: the items read the menu's own state). */
  const pick = (run: () => void) => () => (run(), (ctx = null));
  /** Every folder with its depth, in tree order, for "Move to folder". */
  const folderList = $derived.by(() => {
    const out: { folder: Folder; depth: number }[] = [];
    const walk = (parent: number | null, depth: number) => {
      for (const f of library.folders.filter((x) => x.parent_id === parent)) {
        out.push({ folder: f, depth });
        walk(f.id, depth + 1);
      }
    };
    walk(null, 0);
    return out;
  });

  /** Tree keys: Up and Down walk the visible rows, Right opens a folder, Left closes it or goes to its parent. */
  function treeKey(e: KeyboardEvent) {
    const el = e.target as HTMLElement;
    if (el.getAttribute("role") !== "treeitem" || e.altKey || e.ctrlKey || e.metaKey) return;
    const items = [...(e.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('[role="treeitem"]')];
    const i = items.indexOf(el);
    const folder = el.dataset.folder ? Number(el.dataset.folder) : null;
    let to: HTMLElement | undefined;
    if (e.key === "ArrowDown") to = items[i + 1];
    else if (e.key === "ArrowUp") to = items[i - 1];
    else if (e.key === "Home") to = items[0];
    else if (e.key === "End") to = items[items.length - 1];
    else if (e.key === "ArrowRight" && folder !== null) {
      if (collapsed[folder]) collapsed[folder] = false;
      else if (items[i + 1]?.dataset.parent === el.dataset.folder) to = items[i + 1];
    } else if (e.key === "ArrowLeft") {
      if (folder !== null && !collapsed[folder]) collapsed[folder] = true;
      else if (el.dataset.parent) to = items.find((x) => x.dataset.folder === el.dataset.parent);
    } else return;
    e.preventDefault();
    to?.focus();
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
  <div class="item session" role="none" class:current={activeId === s.id} data-drop={s.folder_id ?? "root"} oncontextmenu={(e) => openCtx(e, { session: s })}>
    <button
      class="main"
      role="treeitem"
      aria-selected={activeId === s.id}
      data-parent={s.folder_id ?? ""}
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
      <button title="Connect (right-click for more)" aria-label={`Connect to ${s.name}`} onclick={() => onconnect(s)}><Icon name="play" size={13} /></button>
    </span>
  </div>
{/snippet}

{#snippet tree(parent: number | null)}
  {#each childrenOf(parent) as f, i (f.id)}
    <div class="item folder" role="none" class:drop={dropTarget === f.id} data-drop={f.id} oncontextmenu={(e) => renaming !== f.id && openCtx(e, { folder: f })}>
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
          role="treeitem"
          aria-selected="false"
          aria-expanded={!collapsed[f.id]}
          aria-owns={collapsed[f.id] ? undefined : `folder-${f.id}`}
          data-folder={f.id}
          data-parent={f.parent_id ?? ""}
          onpointerdown={(e) => pressed(e, { kind: "folder", id: f.id, label: f.name })}
          onclick={() => !justDragged && (collapsed[f.id] = !collapsed[f.id])}
        >
          <span class="caret"><Icon name={collapsed[f.id] ? "chevron-right" : "chevron-down"} size={13} /></span>
          <span class="ficon"><Icon name={collapsed[f.id] ? "folder" : "folder-open"} size={17} tint /></span>
          <span class="name">{f.name}</span>
        </button>
        <span class="acts">
          <button title="New session in this folder (right-click for more)" aria-label={`New session in ${f.name}`} onclick={() => onnew(f.id)}><Icon name="plus" size={14} /></button>
        </span>
      {/if}
    </div>
    {#if !collapsed[f.id]}
      <div class="children" role="group" id="folder-{f.id}" class:drop={dropTarget === f.id} data-drop={f.id}>
        {#if creating?.parent === f.id}{@render folderInput()}{/if}
        {@render tree(f.id)}
        {#each sessionsIn(f.id) as s (s.id)}{@render sessionRow(s)}{/each}
      </div>
    {/if}
    {#if i < childrenOf(parent).length - 1}<div class="sep"></div>{/if}
  {/each}
{/snippet}

{#snippet toggle()}
  <button
    class="sbtoggle"
    aria-label={railOnly ? "Show sidebar" : "Hide sidebar"}
    aria-expanded={!railOnly}
    title={toggleTitle || (railOnly ? "Show sidebar" : "Hide sidebar")}
    onclick={oncollapse}
  >
    <Icon name="panel-left" size={17} />
  </button>
{/snippet}

{#snippet newTabButtons(vertical: boolean)}
  <div class="dual" class:vertical role="group" aria-label="New tab">
    <button title={`New shell${shellKey ? ` (${shellKey})` : ""}`} aria-label="New local shell" onclick={onshell}><Icon name="terminal" size={15} /></button>
    <button title="New SSH connection…" aria-label="New SSH connection" onclick={onssh}><Icon name="server" size={15} /></button>
  </div>
{/snippet}

<div class="sb" class:rail={railOnly}>
<!-- Room for the window's traffic lights. -->
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
    </div>
    <div class="raildivider"></div>
    <div class="railhosts">
      {#each library.sessions as s (s.id)}
        <button
          class="host"
          class:current={activeId === s.id}
          style:--c={s.color ?? "var(--accent)"}
          style:--on={inkOn(s.color)}
          title={`${s.name}\n${s.user}@${s.host}`}
          aria-label={`Connect to ${s.name}`}
          onclick={() => onconnect(s)}
        >
          {initials(s.name)}
          {#if openIds.includes(s.id)}<span class="live" title="Open in a tab"></span>{/if}
        </button>
      {/each}
    </div>
    {@render toggle()}
  </aside>
{:else}
<aside class:dragging={drag !== null}>
  <div class="searchbox"><Icon name="search" size={14} /><input class="search" type="search" placeholder="Search sessions" bind:value={query} aria-label="Search sessions" /></div>
  {#if !blank || creating}
    <div class="tools">
      <Button size="sm" title="New session" aria-label="New session" onclick={() => onnew(null)}><Icon name="plus" size={13} />Session</Button>
      <Button size="sm" title="New folder" aria-label="New folder" onclick={() => startCreate(null)}><Icon name="plus" size={13} />Folder</Button>
    </div>
  {/if}
  {#if tabs}
    <div class="seclabel">Tabs</div>
    <div class="tabsec">{@render tabs()}</div>
    <div class="seclabel">Sessions</div>
  {/if}
  <!-- svelte-ignore a11y_interactive_supports_focus -->
  <div class="list" role="tree" aria-label="Saved sessions" class:drop={dropTarget === "root"} data-drop="root" onkeydown={treeKey}>
    {#if creating?.parent === null}{@render folderInput()}{/if}
    {#if searching}
      {#each library.sessions as s (s.id)}{@render sessionRow(s)}{:else}
        <p class="empty">No sessions match.</p>
      {/each}
    {:else}
      {@render tree(null)}
      {#if childrenOf(null).length > 0 && sessionsIn(null).length > 0}<div class="sep"></div>{/if}
      {#each sessionsIn(null) as s (s.id)}{@render sessionRow(s)}{/each}
      {#if blank && !creating}
        <div class="first">
          <p>No saved sessions yet.</p>
          <Button variant="primary" size="sm" onclick={() => onnew(null)}><Icon name="plus" size={13} />New session</Button>
          {#if onimport}<Button size="sm" onclick={onimport}><Icon name="import" size={13} />Import from ~/.ssh/config</Button>{/if}
          {#if paletteKey}<p class="tip">Press <kbd>{paletteKey}</kbd> to search every action, session and tab.</p>{/if}
        </div>
      {/if}
    {/if}
  </div>
  <div class="footer">{@render toggle()}<span class="version">{version}</span></div>
</aside>
{/if}
</div>

{#if ctx?.session}
  {@const s = ctx.session}
  {#key ctx.moving}
    {#if ctx.moving}
      <Menu label="Move {s.name} to a folder" left={ctx.x} top={ctx.y} scroll onclose={() => (ctx = null)}>
        <div class="mhead">Move to</div>
        <button role="menuitemradio" aria-checked={s.folder_id === null} onclick={pick(() => s.folder_id !== null && onmovesession(s.id, null))}>Top level</button>
        {#each folderList as { folder, depth } (folder.id)}
          <button role="menuitemradio" aria-checked={s.folder_id === folder.id} style:padding-left="calc(var(--space-3) + {depth} * var(--space-3))" onclick={pick(() => s.folder_id !== folder.id && onmovesession(s.id, folder.id))}>
            <Icon name="folder" size={14} />{folder.name}
          </button>
        {/each}
      </Menu>
    {:else}
      <Menu label={s.name} left={ctx.x} top={ctx.y} onclose={() => (ctx = null)}>
        <button role="menuitem" onclick={pick(() => onconnect(s))}>Connect</button>
        {#if onsplit}<button role="menuitem" onclick={pick(() => onsplit(s))}>Open in split</button>{/if}
        <hr />
        <button role="menuitem" onclick={pick(() => onedit(s))}>Edit…</button>
        {#if onduplicate}<button role="menuitem" onclick={pick(() => onduplicate(s))}>Duplicate</button>{/if}
        <button role="menuitem" disabled={library.folders.length === 0} onclick={() => (ctx = { ...ctx!, moving: true })}>Move to folder…</button>
        <hr />
        <button role="menuitem" onclick={pick(() => ondelete(s))}>Delete</button>
      </Menu>
    {/if}
  {/key}
{:else if ctx?.folder}
  {@const f = ctx.folder}
  <Menu label={f.name} left={ctx.x} top={ctx.y} onclose={() => (ctx = null)}>
    <button role="menuitem" onclick={pick(() => onnew(f.id))}>New session here</button>
    <button role="menuitem" onclick={pick(() => startCreate(f.id))}>New subfolder</button>
    <button role="menuitem" onclick={pick(() => startRename(f.id, f.name))}>Rename</button>
    <hr />
    <button role="menuitem" onclick={pick(() => ondeletefolder(f.id))}>Delete folder</button>
  </Menu>
{/if}

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
    padding: 0 var(--space-3);
  }
  /* New tab: a local shell or an SSH connection. Recessed pair, the hovered half rises. */
  .dual {
    display: flex;
    gap: 2px;
    padding: 2px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-l);
    background: var(--bg-deep);
  }
  .dual button {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 24px;
    border-radius: var(--radius-m);
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
    gap: var(--space-3);
    width: calc(var(--sidebar-w) - 1px);
    box-sizing: border-box;
    padding: var(--space-1) var(--space-3) var(--space-3);
    font-size: var(--text-m);
    min-height: 0;
  }
  /* Vertical tabs live here, so the window has one left column, not two. */
  .seclabel {
    flex: none;
    margin: 2px var(--space-1) calc(var(--space-1) * -1);
    font-size: var(--text-xs);
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--dim);
  }
  .tabsec {
    flex: 0 1 auto;
    min-height: 38px;
    max-height: 40%;
    margin: 0 calc(var(--space-1) * -1);
    padding: 0 var(--space-1);
    overflow-y: auto;
  }
  .searchbox {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: none;
    height: 34px;
    padding: 0 var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-l);
    background: var(--input);
    color: var(--dim);
  }
  .searchbox:focus-within {
    border-color: var(--focus);
  }
  .search {
    flex: 1;
    min-width: 0;
    padding: 0;
    font: inherit;
    font-size: var(--text-m);
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
  /* Wraps to a second line when the sidebar is dragged narrow, so no label is cut. */
  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    flex: none;
  }
  .tools :global(.btn) {
    flex: 1 0 auto;
    gap: var(--space-1);
    padding: var(--space-1) var(--space-1);
    background: var(--surface);
  }
  .tools :global(.btn:hover) {
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
    margin: 0 calc(var(--space-1) * -1);
    padding: 0 var(--space-1);
  }
  .item {
    position: relative;
    display: flex;
    align-items: center;
    margin: 1px 0;
    padding-right: var(--space-1);
    border-radius: var(--radius-m);
  }
  .item:hover,
  .item:focus-within,
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
    margin: var(--space-1) var(--space-2);
    background: var(--border);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
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
    gap: var(--space-2);
    flex: 0 1000 auto;
    min-width: 0;
    margin-left: auto;
  }
  .tags {
    display: flex;
    gap: var(--space-1);
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
    padding: 2px var(--space-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-l);
    background: var(--surface);
    color: var(--dim);
    font-size: var(--text-xs);
    line-height: 1.2;
  }
  .tag.more {
    flex: none;
  }
  .caret {
    display: flex;
    width: 13px;
    margin-left: -2px;
    color: var(--dim);
  }
  /* The row's actions lie over its right end, on an edge that fades into the row. The name and
     tags stay where they are, so nothing moves when the pointer comes in. */
  .acts {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    display: none;
    align-items: center;
    padding: 0 var(--space-1) 0 var(--space-5);
    border-radius: 0 var(--radius-m) var(--radius-m) 0;
    background: linear-gradient(to right, transparent, var(--hover) var(--space-5));
  }
  .item:hover .acts,
  .item:focus-within .acts {
    display: flex;
  }
  .acts button {
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 24px;
    min-height: 24px;
    padding: 0;
    border-radius: var(--radius-m);
    color: var(--dim);
  }
  .acts button:hover {
    background: var(--hover);
    color: var(--fg);
  }
  .item input {
    flex: 1;
    min-width: 0;
    margin: 2px var(--space-1);
    padding: 2px var(--space-1);
    font: inherit;
    color: inherit;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    background: var(--input);
  }
  aside.dragging {
    user-select: none;
    cursor: grabbing;
  }
  .children {
    margin-left: var(--space-4);
  }
  .item.drop,
  .children.drop {
    background: color-mix(in srgb, var(--accent) 25%, transparent);
    outline: 1px solid var(--focus);
  }
  .list.drop {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .ghost {
    position: fixed;
    z-index: 30;
    padding: 2px var(--space-2);
    border-radius: var(--radius-m);
    background: var(--accent);
    color: var(--on-accent);
    font-size: var(--text-s);
    pointer-events: none;
    opacity: 0.9;
  }
  .footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex: none;
    padding: var(--space-1) var(--space-1) 0 0;
    border-top: 1px solid var(--border);
  }
  /* Hide or show the sidebar: last thing in it, in the same corner in both states. */
  .sbtoggle {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 28px;
    height: 28px;
    border-radius: var(--radius-m);
    color: var(--dim);
  }
  .sbtoggle:hover {
    color: var(--fg);
    background: var(--hover);
  }
  .version {
    font-size: var(--text-xs);
    color: var(--dim);
  }
  .empty {
    margin: var(--space-3) var(--space-1);
    color: var(--dim);
  }
  /* Nothing saved yet: the two ways to get a first session, and where everything else is found. */
  .first {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-2);
    margin: var(--space-3) var(--space-1);
    color: var(--dim);
  }
  .first p {
    margin: 0;
  }
  .first .tip {
    margin-top: var(--space-2);
    font-size: var(--text-s);
    line-height: 1.5;
  }
  .first kbd {
    padding: 1px var(--space-1);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    color: var(--fg);
    font: inherit;
  }

  /* ---- collapsed rail ---- */
  aside.rail {
    align-items: center;
    gap: var(--space-2);
    width: calc(var(--rail-w) - 1px);
    padding: var(--space-1) 0 var(--space-2);
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
    border-radius: var(--radius-m);
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
    /* Full rail width: as a centred flex item it shrinks to the 30px button, and this scroll
       box then clips the status dot that hangs off the button's corner. */
    align-self: stretch;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) 0;
    overflow-y: auto;
    scrollbar-width: none;
  }
  .host {
    position: relative;
    flex: none;
    width: 30px;
    height: 30px;
    border-radius: var(--radius-l);
    background: var(--c);
    color: var(--on);
    font-size: var(--text-xs);
    font-weight: 700;
    letter-spacing: 0.3px;
  }
  .host:hover {
    filter: brightness(1.12);
  }
  /* The current host gets a slim bar on the rail's edge (no frame around the button). */
  .host.current::before {
    content: "";
    position: absolute;
    left: -7px;
    top: 7px;
    bottom: 7px;
    width: 3px;
    border-radius: var(--radius-s);
    background: var(--c);
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
