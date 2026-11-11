<script lang="ts">
  import type { JumpSecret } from "./bindings/JumpSecret";
  import { onMount, tick } from "svelte";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { open as pickFiles } from "@tauri-apps/plugin-dialog";
  import {
    onSftpTransfer,
    sftpCancel,
    sftpClose,
    sftpDelete,
    sftpDownload,
    sftpList,
    sftpMkdir,
    sftpOpen,
    sftpOpenSaved,
    sftpOpenShared,
    sftpRename,
    sftpUpload,
    sftpCanonical,
  } from "./ipc";
  import { focus } from "./focus";
  import Icon from "./Icon.svelte";
  import Button from "./Button.svelte";
  import Menu from "./Menu.svelte";
  import { crumbs, DEFAULT_SORT, isHidden, nextRate, nextSort, progressText, sizeText, sortEntries, type Rate, type Sort, type SortKey } from "./files";
  import type { Entry } from "./bindings/Entry";
  import type { SftpTransfer } from "./bindings/SftpTransfer";
  import type { SshTarget } from "./bindings/SshTarget";

  type Saved = { id: number; label: string; secret?: string | null; jumps?: JumpSecret[] };
  let {
    ssh = null,
    saved = null,
    active,
    terminalId,
  }: {
    ssh?: SshTarget | null;
    saved?: Saved | null;
    active: boolean;
    /** The terminal tab's session id, if it is connected: its login is reused. */
    terminalId?: () => number | null;
  } = $props();

  let conn = $state<number | null>(null);
  let path = $state("");
  let pathInput = $state("");
  let entries = $state<Entry[]>([]);
  let status = $state<"connecting" | "ready" | "failed">("connecting");
  let error = $state<string | null>(null);
  let loading = $state(false);
  let edit = $state<{ kind: "rename"; path: string; value: string } | { kind: "mkdir"; value: string } | null>(null);
  let confirmDelete = $state<string | null>(null);
  let transfers = $state<SftpTransfer[]>([]);
  let panel: HTMLElement;
  let dropping = $state(false);
  let selected = $state<string | null>(null);

  // ---- how the listing is shown: sort order and hidden files, remembered between visits ----
  function readPref<T>(key: string, fallback: T): T {
    try {
      const raw = localStorage.getItem(key);
      return raw === null ? fallback : (JSON.parse(raw) as T);
    } catch {
      return fallback;
    }
  }
  function writePref(key: string, value: unknown) {
    try {
      localStorage.setItem(key, JSON.stringify(value));
    } catch {
      /* the preference is optional */
    }
  }
  const savedSort = readPref<Partial<Sort>>("quay.files.sort", DEFAULT_SORT);
  let sort = $state<Sort>(savedSort.key === "name" || savedSort.key === "size" || savedSort.key === "date" ? { key: savedSort.key, asc: savedSort.asc !== false } : DEFAULT_SORT);
  let showHidden = $state(readPref<unknown>("quay.files.hidden", false) === true);
  const sortBy = (key: SortKey) => writePref("quay.files.sort", (sort = nextSort(sort, key)));
  const hiddenCount = $derived(entries.filter((x) => isHidden(x.name)).length);
  const shown = $derived(sortEntries(showHidden ? entries : entries.filter((x) => !isHidden(x.name)), sort));
  const columns: [SortKey, string][] = [
    ["name", "Name"],
    ["size", "Size"],
    ["date", "Modified"],
  ];

  // ---- the path: a breadcrumb to click, or a field to type in ----
  let typing = $state(false);
  let crumbsEl = $state<HTMLElement>();
  // A deep path keeps its end (where you are) in view.
  $effect(() => {
    void path;
    if (!typing) tick().then(() => crumbsEl && (crumbsEl.scrollLeft = crumbsEl.scrollWidth));
  });
  function typePath() {
    pathInput = path;
    typing = true;
  }

  /** The "more" menu of a row (also its right-click menu). */
  let rowMenu = $state<{ x: number; y: number; entry: Entry } | null>(null);
  function openRowMenu(e: MouseEvent, entry: Entry) {
    e.preventDefault();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const byButton = e.type === "click" || (e.clientX === 0 && e.clientY === 0);
    rowMenu = { x: byButton ? r.left : e.clientX, y: byButton ? r.bottom + 2 : e.clientY, entry };
  }
  const dateText = (s: number | null) => {
    if (s == null) return "";
    const d = new Date(s * 1000);
    // "Oct 10" for this year, "Oct 10, 24" for older files.
    return d.toLocaleDateString(undefined, d.getFullYear() === new Date().getFullYear() ? { month: "short", day: "2-digit" } : { month: "short", day: "2-digit", year: "2-digit" });
  };
  const join = (dir: string, name: string) => (dir.endsWith("/") ? dir + name : `${dir}/${name}`);
  const parent = (p: string) => p.replace(/\/[^/]+\/?$/, "") || "/";

  /** A server that never answers must not leave the panel on "Connecting…" for ever. */
  function withTimeout<T>(p: Promise<T>, ms = 45000): Promise<T> {
    return new Promise((resolve, reject) => {
      const t = setTimeout(() => reject("The server did not answer the SFTP request in time."), ms);
      p.then(resolve, reject).finally(() => clearTimeout(t));
    });
  }

  async function connect() {
    status = "connecting";
    error = null;
    try {
      // Reuse the terminal's login when it is still connected. Sign in again only when that login
      // is gone: if the server refused SFTP, a second login would fail the same way and hide why.
      const tid = terminalId?.() ?? null;
      let shared: Awaited<ReturnType<typeof sftpOpenShared>> | null = null;
      if (tid !== null) {
        try {
          shared = await withTimeout(sftpOpenShared(tid));
        } catch (e) {
          if (!`${e}`.includes("connection is closed")) throw e;
        }
      }
      const opened =
        shared ?? (await withTimeout(saved ? sftpOpenSaved(saved.id, saved.secret ?? null, saved.jumps ?? []) : sftpOpen(ssh!)));
      conn = opened.id;
      status = "ready";
      await go(opened.home);
    } catch (e) {
      status = "failed";
      error = `${e}`;
    }
  }

  async function go(to: string) {
    if (conn === null) return;
    loading = true;
    try {
      const target = await sftpCanonical(conn, to);
      entries = await sftpList(conn, target);
      path = pathInput = target;
      error = null;
    } catch (e) {
      error = `${e}`;
      pathInput = path;
    } finally {
      loading = false;
    }
  }
  const reload = () => go(path);

  async function attempt(run: () => Promise<unknown>) {
    try {
      await run();
      error = null;
    } catch (e) {
      error = `${e}`;
    }
  }

  async function commitEdit() {
    const e = edit;
    edit = null;
    if (!e || conn === null || !e.value.trim() || e.value.includes("/")) return;
    const id = conn;
    await attempt(() => (e.kind === "mkdir" ? sftpMkdir(id, join(path, e.value.trim())) : sftpRename(id, e.path, join(parent(e.path), e.value.trim()))));
    await reload();
  }

  async function remove(p: string) {
    confirmDelete = null;
    const id = conn!;
    await attempt(() => sftpDelete(id, p));
    await reload();
  }

  /** Where each download is written on this computer, by transfer id, for "Show in folder". */
  let locals = $state<Record<number, string>>({});
  /** The folder last chosen with "Download to…", offered again next time. */
  let lastDir: string | undefined;
  async function download(entry: Entry, dir: string | null = null) {
    await attempt(async () => {
      const queued = await sftpDownload(conn!, entry.path, dir);
      locals[queued.id] = queued.path;
    });
  }
  async function downloadTo(entry: Entry) {
    const picked = await pickFiles({ multiple: false, directory: true, title: `Download ${entry.name} to`, defaultPath: lastDir }).catch(() => null);
    if (typeof picked !== "string") return;
    lastDir = picked;
    await download(entry, picked);
  }
  const reveal = (id: number) => attempt(() => revealItemInDir(locals[id]));

  async function upload(locals: string[]) {
    if (conn === null || locals.length === 0) return;
    await attempt(() => sftpUpload(conn!, locals, path));
  }

  async function chooseFiles() {
    const picked = await pickFiles({ multiple: true, directory: false });
    if (picked) await upload(Array.isArray(picked) ? picked : [picked]);
  }
  async function chooseFolder() {
    const picked = await pickFiles({ multiple: false, directory: true });
    if (typeof picked === "string") await upload([picked]);
  }

  /** Speed of each running transfer, worked out from its progress reports. */
  let rates = $state<Record<number, Rate>>({});
  function track(t: SftpTransfer) {
    if (t.conn !== conn) return;
    if (t.state === "running") rates[t.id] = nextRate(rates[t.id], performance.now(), t.done);
    else delete rates[t.id];
    const i = transfers.findIndex((x) => x.id === t.id);
    if (i >= 0) transfers[i] = t;
    else transfers.push(t);
    // A finished upload changes the listing.
    if (t.state === "done" && t.direction === "upload") reload();
  }
  const finished = (t: SftpTransfer) => t.state === "done" || t.state === "cancelled" || t.state === "failed";
  const clearFinished = () => (transfers = transfers.filter((t) => !finished(t)));
  const percent = (t: SftpTransfer) => (t.state === "done" ? 100 : t.total ? Math.min(100, Math.floor((t.done / t.total) * 100)) : 0);

  onMount(() => {
    connect();
    const un = [
      onSftpTransfer(track),
      // OS files dropped on this panel are uploaded to the current folder.
      getCurrentWebview().onDragDropEvent((ev) => {
        const p = ev.payload;
        if (p.type === "leave") return void (dropping = false);
        const r = panel?.getBoundingClientRect();
        const ratio = window.devicePixelRatio || 1;
        const inside =
          active && !!r && p.position.x / ratio >= r.left && p.position.x / ratio <= r.right && p.position.y / ratio >= r.top && p.position.y / ratio <= r.bottom;
        if (p.type === "drop") {
          dropping = false;
          if (inside && status === "ready") upload(p.paths);
        } else {
          dropping = inside && status === "ready";
        }
      }),
    ];
    return () => {
      un.forEach((u) => u.then((f) => f()));
      if (conn !== null) sftpClose(conn);
    };
  });
</script>

<aside bind:this={panel} class:dropping>
  <div class="pathbar">
    {#if status !== "ready"}
      <span class="path dim">{status === "connecting" ? "Connecting…" : "Not connected"}</span>
    {:else if typing}
      <input
        class="path"
        aria-label="Remote path"
        use:focus
        bind:value={pathInput}
        onfocus={(e) => e.currentTarget.select()}
        onkeydown={(e) => {
          if (e.key === "Enter") ((typing = false), go(pathInput));
          else if (e.key === "Escape") (e.stopPropagation(), (typing = false));
        }}
        onblur={() => (typing = false)}
        spellcheck="false"
        autocapitalize="off"
      />
    {:else}
      <!-- Each folder on the way here is a button; the empty space after them opens the field to type a path. -->
      <nav class="crumbs" aria-label="Remote path" bind:this={crumbsEl}>
        {#each crumbs(path) as c, i (c.path)}
          {#if i > 1}<span class="sep" aria-hidden="true">/</span>{/if}
          <button class="crumb" class:here={c.path === path} aria-current={c.path === path ? "location" : undefined} title={c.path} onclick={() => (c.path === path ? typePath() : go(c.path))}>{c.name}</button>
        {/each}
        <button class="crumbfill" title="Type a path" aria-label="Type a path" onclick={typePath}></button>
      </nav>
      <Button variant="icon" title="Type a path" aria-label="Type a path" onclick={typePath}><Icon name="pencil" size={14} /></Button>
      <Button variant="icon" title="Parent folder" aria-label="Parent folder" disabled={path === "/"} onclick={() => go(parent(path))}><Icon name="arrow-up" size={15} /></Button>
      <Button variant="icon" title="Refresh" aria-label="Refresh" onclick={reload}><Icon name="refresh" size={15} /></Button>
    {/if}
  </div>
  <!-- A thin moving line while a folder is being read; the old listing stays readable under it. -->
  <div class="busy" class:on={loading || status === "connecting"} role="progressbar" aria-label="Loading" aria-hidden={!(loading || status === "connecting")}></div>

  {#if status === "connecting"}
    <ul class="list skeleton" aria-hidden="true">
      {#each [62, 40, 74, 48, 56, 34, 66] as w (w)}
        <li class="row"><span class="bone ic"></span><span class="bone" style:width="{w}%"></span></li>
      {/each}
    </ul>
  {:else if status === "failed"}
    <p class="msg err">{error}</p>
    <p class="msg"><Button size="sm" onclick={connect}>Try again</Button></p>
    <div class="grow"></div>
  {:else}
    <div class="actions">
      <Button size="sm" onclick={() => (edit = { kind: "mkdir", value: "" })}>New folder</Button>
      <Button size="sm" onclick={chooseFiles}>Upload files</Button>
      <Button size="sm" onclick={chooseFolder}>Upload folder</Button>
      <Button
        size="sm"
        variant="ghost"
        active={showHidden}
        aria-pressed={showHidden}
        title={showHidden ? "Hide files whose name starts with a dot" : `Show files whose name starts with a dot${hiddenCount ? ` (${hiddenCount} here)` : ""}`}
        onclick={() => writePref("quay.files.hidden", (showHidden = !showHidden))}>Hidden files</Button>
    </div>
    {#if error}<p class="msg err">{error}</p>{/if}

    <div class="cols" role="row">
      {#each columns as [key, label] (key)}
        <button class="col {key}" role="columnheader" aria-sort={sort.key === key ? (sort.asc ? "ascending" : "descending") : "none"} title="Sort by {label.toLowerCase()}" onclick={() => sortBy(key)}>
          {label}{#if sort.key === key}<Icon name={sort.asc ? "arrow-up" : "arrow-down"} size={11} />{/if}
        </button>
      {/each}
      <span class="col acts-space"></span>
    </div>
    <ul class="list" aria-busy={loading}>
      {#if edit?.kind === "mkdir"}
        <li class="row">
          <span class="icon folder"><Icon name="folder" size={16} /></span>
          <input
            class="name-input"
            use:focus
            bind:value={edit.value}
            placeholder="Folder name"
            onkeydown={(e) => (e.key === "Enter" ? commitEdit() : e.key === "Escape" && (e.stopPropagation(), (edit = null)))}
            onblur={() => (edit = null)}
          />
        </li>
      {/if}
      {#each shown as entry (entry.path)}
        <li class="row" class:selected={selected === entry.path} class:hid={isHidden(entry.name)} oncontextmenu={(e) => openRowMenu(e, entry)}>
          <span class="icon" class:folder={entry.kind === "dir"} title={entry.kind === "symlink" ? "Symbolic link" : undefined}>
            <Icon name={entry.kind === "dir" ? "folder" : "file"} size={16} />
          </span>
          {#if edit?.kind === "rename" && edit.path === entry.path}
            <input
              class="name-input"
              use:focus
              bind:value={edit.value}
              onkeydown={(e) => (e.key === "Enter" ? commitEdit() : e.key === "Escape" && (e.stopPropagation(), (edit = null)))}
              onblur={() => (edit = null)}
            />
          {:else}
            <button
              class="name"
              title={entry.path}
              ondblclick={() => (entry.kind === "dir" ? go(entry.path) : download(entry))}
              onclick={(e) => ((selected = entry.path), e.detail === 1 && entry.kind === "dir" && go(entry.path))}
            >
              {entry.name}
            </button>
          {/if}
          {#if confirmDelete === entry.path}
            <span class="confirm">
              <button class="danger" onclick={() => remove(entry.path)}>Delete</button>
              <button onclick={() => (confirmDelete = null)}>Keep</button>
            </span>
          {:else}
            <span class="meta size">{entry.kind === "dir" ? "—" : sizeText(entry.size)}</span>
            <span class="meta date">{dateText(entry.modified)}</span>
            <!-- Their own column, so the size and date stay where they are when the pointer comes in. -->
            <span class="acts">
              <button title="Download to your Downloads folder" aria-label="Download {entry.name}" onclick={() => download(entry)}><Icon name="arrow-down" size={14} /></button>
              <button title="More" aria-label="More actions for {entry.name}" aria-haspopup="menu" onclick={(e) => openRowMenu(e, entry)}><Icon name="more" size={14} /></button>
            </span>
          {/if}
        </li>
      {:else}
        {#if loading && entries.length === 0}
          {#each [58, 42, 70] as w (w)}<li class="row skeleton" aria-hidden="true"><span class="bone ic"></span><span class="bone" style:width="{w}%"></span></li>{/each}
        {:else if entries.length > 0}
          <li class="empty">Only hidden files here. <button class="link" onclick={() => writePref("quay.files.hidden", (showHidden = true))}>Show {hiddenCount} hidden</button></li>
        {:else if !loading}
          <li class="empty">This folder is empty. Drop files here to upload.</li>
        {/if}
      {/each}
    </ul>
    {#if rowMenu}
      {@const m = rowMenu.entry}
      <Menu label={m.name} left={rowMenu.x} top={rowMenu.y} onclose={() => (rowMenu = null)}>
        {#if m.kind === "dir"}<button role="menuitem" onclick={() => (go(m.path), (rowMenu = null))}>Open</button>{/if}
        <button role="menuitem" onclick={() => (download(m), (rowMenu = null))}>Download</button>
        <button role="menuitem" onclick={() => (downloadTo(m), (rowMenu = null))}>Download to…</button>
        <hr />
        <button role="menuitem" onclick={() => ((edit = { kind: "rename", path: m.path, value: m.name }), (rowMenu = null))}>Rename</button>
        <button role="menuitem" onclick={() => ((confirmDelete = m.path), (rowMenu = null))}>Delete…</button>
      </Menu>
    {/if}

    {#if transfers.length}
      <div class="transfers">
        <div class="thead">
          <span>Transfers</span>
          <button onclick={clearFinished}>Clear finished</button>
        </div>
        {#each transfers as t (t.id)}
          {@const running = t.state === "queued" || t.state === "running"}
          <div class="t" class:ok={t.state === "done"} class:bad={t.state === "failed"}>
            <div class="trow">
              <span class="ticon">
                <Icon name={t.state === "done" ? "check" : t.state === "failed" ? "alert" : t.state === "cancelled" ? "stop" : t.direction === "upload" ? "arrow-up" : "arrow-down"} size={14} />
              </span>
              <span class="tname" title={t.name}>{t.name}</span>
              {#if t.state === "failed"}
                <span class="small err" title={t.message}>{t.message}</span>
              {:else if t.state === "cancelled"}
                <span class="small">Cancelled</span>
              {:else if running}
                <span class="small">{progressText(t.done, t.total, rates[t.id]?.bps ?? 0) || "Waiting"}</span>
              {:else if t.total > 0}
                <span class="small">{sizeText(t.total)}</span>
              {/if}
              {#if t.state === "done" && t.direction === "download" && locals[t.id]}
                <Button variant="icon" title="Show in folder: {locals[t.id]}" aria-label="Show {t.name} in its folder" onclick={() => reveal(t.id)}><Icon name="folder-open" size={13} /></Button>
              {/if}
              {#if running}
                <Button variant="icon" title="Cancel" aria-label="Cancel transfer" onclick={() => sftpCancel(conn!, t.id)}><Icon name="stop" size={13} /></Button>
              {/if}
            </div>
            {#if t.state !== "failed" && t.state !== "cancelled"}
              <div class="bar" role="progressbar" aria-label={t.name} aria-valuemin="0" aria-valuemax="100" aria-valuenow={percent(t)}>
                <div class="fill" style:width="{percent(t)}%"></div>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</aside>

<style>
  /* Fills the dock that App draws; the dock owns the width, the edge and the close button. */
  aside {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    font-size: var(--text-m);
    /* The date column gives way when the panel is dragged narrow (see the container rule below). */
    container-type: inline-size;
  }
  aside.dropping {
    outline: 2px dashed var(--accent);
    outline-offset: -4px;
  }
  button,
  input {
    font: inherit;
    color: inherit;
  }
  button {
    background: none;
    border: 0;
    cursor: pointer;
    padding: 2px var(--space-2);
    border-radius: var(--radius-s);
  }
  button:hover:not(:disabled) {
    background: var(--hover);
  }
  button:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .pathbar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: none;
    padding: var(--space-3) var(--space-3) var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--border);
  }
  /* The way here, one button per folder. It scrolls sideways without a scrollbar when it is long. */
  .crumbs {
    display: flex;
    align-items: center;
    flex: 1;
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: none;
    font-family: var(--font-mono);
    font-size: var(--text-s);
  }
  .crumbs::-webkit-scrollbar {
    display: none;
  }
  .crumb {
    flex: none;
    max-width: 160px;
    padding: 2px var(--space-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--dim);
  }
  .crumb:hover:not(:disabled),
  .crumb.here {
    color: var(--fg);
  }
  .crumb.here {
    font-weight: 600;
  }
  .sep {
    flex: none;
    color: var(--dim);
  }
  .crumbfill {
    flex: 1;
    align-self: stretch;
    min-width: var(--space-4);
    min-height: 22px;
    padding: 0;
    cursor: text;
  }
  .crumbfill:hover:not(:disabled) {
    background: none;
  }
  .busy {
    flex: none;
    height: 2px;
    margin-top: -1px;
    overflow: hidden;
  }
  .busy.on::before {
    content: "";
    display: block;
    width: 35%;
    height: 100%;
    background: var(--accent);
    animation: busy 1.1s ease-in-out infinite;
  }
  @keyframes busy {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(300%);
    }
  }
  /* Column names; click one to sort by it. The widths match the rows below. */
  .cols {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: none;
    margin-top: var(--space-1);
    padding: 0 var(--space-4) 2px;
    border-bottom: 1px solid var(--border);
    color: var(--dim);
    font-size: var(--text-xs);
  }
  .col {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 2px 0;
    white-space: nowrap;
  }
  .col.name {
    flex: 1;
    min-width: 0;
    /* Lines up with the names: past the row's icon. */
    padding-left: var(--space-6);
  }
  .col.size,
  .col.date {
    justify-content: flex-end;
  }
  .col[aria-sort="ascending"],
  .col[aria-sort="descending"] {
    color: var(--fg);
  }
  .size {
    flex: none;
    width: 60px;
  }
  .date {
    flex: none;
    width: 62px;
  }
  .acts,
  .acts-space {
    flex: none;
    width: 48px;
  }
  @container (max-width: 330px) {
    .date {
      display: none;
    }
  }
  /* Placeholder rows while the first listing is on its way. */
  .bone {
    height: 10px;
    border-radius: var(--radius-s);
    background: var(--hover);
    animation: bone 1.2s ease-in-out infinite;
  }
  .bone.ic {
    flex: none;
    width: 16px;
    height: 14px;
  }
  .skeleton .row,
  .row.skeleton {
    min-height: 24px;
  }
  .skeleton .row:hover,
  .row.skeleton:hover {
    background: none;
  }
  @keyframes bone {
    50% {
      opacity: 0.45;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .bone,
    .busy.on::before {
      animation: none;
    }
    .busy.on::before {
      width: 100%;
      opacity: 0.6;
    }
  }
  .path,
  .name-input {
    flex: 1;
    min-width: 0;
    border: 1px solid transparent;
    border-radius: var(--radius-s);
    background: none;
    padding: 2px var(--space-1);
    font-family: var(--font-mono);
    font-size: var(--text-s);
    font-weight: 600;
  }
  .path:hover {
    border-color: var(--border);
  }
  .path:focus,
  .name-input {
    background: var(--input);
    border-color: var(--border-strong);
    outline: 0;
  }
  .path:focus {
    border-color: var(--focus);
  }
  .path.dim {
    color: var(--dim);
    font-weight: 400;
  }
  .name-input {
    font-family: inherit;
    font-weight: 400;
    font-size: inherit;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    flex: none;
    padding: var(--space-2) var(--space-3) var(--space-1);
  }
  .msg {
    margin: var(--space-2) var(--space-3);
  }
  .err {
    color: var(--red-text);
    word-break: break-word;
  }
  .grow {
    flex: 1;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    list-style: none;
    margin: var(--space-1) 0 0;
    padding: 0 var(--space-2) var(--space-2);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-m);
  }
  .row:hover,
  .row.selected {
    background: var(--hover);
  }
  .icon {
    display: flex;
    flex: none;
    color: var(--dim);
  }
  .icon.folder {
    color: var(--yellow-text);
  }
  .name {
    flex: 1;
    min-width: 0;
    padding: 0;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name:hover:not(:disabled) {
    background: none;
  }
  .meta {
    color: var(--dim);
    font-size: var(--text-s);
    white-space: nowrap;
    text-align: right;
  }
  /* A dot file, when they are shown: a little quieter than the rest. */
  .row.hid .name,
  .row.hid .icon {
    opacity: 0.65;
  }
  .acts {
    display: flex;
    justify-content: flex-end;
    visibility: hidden;
  }
  .row:hover .acts,
  .row:focus-within .acts {
    visibility: visible;
  }
  .confirm {
    display: flex;
    flex: none;
  }
  .link {
    padding: 0;
    color: var(--accent-text);
    text-decoration: underline;
  }
  .acts button {
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 24px;
    min-height: 24px;
    padding: 0;
    color: var(--dim);
  }
  .acts button:hover {
    color: var(--fg);
  }
  .danger {
    color: var(--red-text);
  }
  .empty {
    padding: var(--space-3) var(--space-3);
    color: var(--dim);
  }
  .transfers {
    flex: none;
    max-height: 190px;
    overflow-y: auto;
    padding: var(--space-3) var(--space-4) var(--space-3);
    background: var(--bg-alt);
    border-top: 1px solid var(--border);
  }
  .thead {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--space-2);
    color: var(--dim);
    font-size: var(--text-s);
    letter-spacing: 0.8px;
    text-transform: uppercase;
  }
  .thead button {
    color: inherit;
    font-size: var(--text-xs);
    letter-spacing: 0;
    text-transform: none;
  }
  .t {
    margin-bottom: var(--space-2);
  }
  .t:last-child {
    margin-bottom: 0;
  }
  .trow {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-bottom: var(--space-1);
  }
  .ticon {
    display: flex;
    flex: none;
    color: var(--accent-text);
  }
  .t.ok .ticon {
    color: var(--green-text);
  }
  .t.bad .ticon {
    color: var(--red-text);
  }
  .tname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .small {
    flex: none;
    max-width: 55%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--dim);
    font-size: var(--text-s);
  }
  .small.err {
    color: var(--red-text);
  }
  .bar {
    height: 5px;
    overflow: hidden;
    border-radius: var(--radius-s);
    background: var(--hover);
  }
  .fill {
    height: 100%;
    border-radius: var(--radius-s);
    background: var(--accent);
    transition: width 120ms linear;
  }
  .t.ok .fill {
    background: var(--green);
  }
  @media (prefers-reduced-motion: reduce) {
    .fill {
      transition: none;
    }
  }
</style>
