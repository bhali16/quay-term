<script lang="ts">
  import type { JumpSecret } from "./bindings/JumpSecret";
  import { onMount } from "svelte";
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

  const sizeText = (n: number) =>
    n < 1024 ? `${n} B` : n < 1024 ** 2 ? `${(n / 1024).toFixed(1)} KB` : n < 1024 ** 3 ? `${(n / 1024 ** 2).toFixed(1)} MB` : `${(n / 1024 ** 3).toFixed(1)} GB`;
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

  async function download(entry: Entry) {
    await attempt(async () => {
      await sftpDownload(conn!, entry.path);
    });
  }

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

  function track(t: SftpTransfer) {
    if (t.conn !== conn) return;
    const i = transfers.findIndex((x) => x.id === t.id);
    if (i >= 0) transfers[i] = t;
    else transfers.push(t);
    // A finished upload changes the listing.
    if (t.state === "done" && t.direction === "upload") reload();
  }
  const finished = (t: SftpTransfer) => t.state === "done" || t.state === "cancelled" || t.state === "failed";
  const clearFinished = () => (transfers = transfers.filter((t) => !finished(t)));

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
    <span class="plabel">Path</span>
    {#if status === "ready"}
      <input
        class="path"
        aria-label="Remote path"
        bind:value={pathInput}
        onkeydown={(e) => e.key === "Enter" && go(pathInput)}
        spellcheck="false"
      />
      <Button variant="icon" title="Parent folder" aria-label="Parent folder" disabled={path === "/"} onclick={() => go(parent(path))}><Icon name="arrow-up" size={15} /></Button>
      <Button variant="icon" title="Refresh" aria-label="Refresh" onclick={reload}><Icon name="refresh" size={15} /></Button>
    {:else}
      <span class="path dim">{status === "connecting" ? "Connecting…" : "Not connected"}</span>
    {/if}
  </div>

  {#if status === "connecting"}
    <div class="grow"></div>
  {:else if status === "failed"}
    <p class="msg err">{error}</p>
    <p class="msg"><Button size="sm" onclick={connect}>Try again</Button></p>
    <div class="grow"></div>
  {:else}
    <div class="actions">
      <Button size="sm" onclick={() => (edit = { kind: "mkdir", value: "" })}>New folder</Button>
      <Button size="sm" onclick={chooseFiles}>Upload files</Button>
      <Button size="sm" onclick={chooseFolder}>Upload folder</Button>
    </div>
    {#if error}<p class="msg err">{error}</p>{/if}

    <ul class="list" class:loading>
      {#if edit?.kind === "mkdir"}
        <li class="row">
          <span class="icon folder"><Icon name="folder" size={16} /></span>
          <input
            class="name-input"
            use:focus
            bind:value={edit.value}
            placeholder="Folder name"
            onkeydown={(e) => (e.key === "Enter" ? commitEdit() : e.key === "Escape" && (edit = null))}
            onblur={() => (edit = null)}
          />
        </li>
      {/if}
      {#each entries as entry (entry.path)}
        <li class="row" class:selected={selected === entry.path}>
          <span class="icon" class:folder={entry.kind === "dir"} title={entry.kind === "symlink" ? "Symbolic link" : undefined}>
            <Icon name={entry.kind === "dir" ? "folder" : "file"} size={16} />
          </span>
          {#if edit?.kind === "rename" && edit.path === entry.path}
            <input
              class="name-input"
              use:focus
              bind:value={edit.value}
              onkeydown={(e) => (e.key === "Enter" ? commitEdit() : e.key === "Escape" && (edit = null))}
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
          <span class="meta size">{entry.kind === "dir" ? "—" : sizeText(entry.size)}</span>
          <span class="meta date">{dateText(entry.modified)}</span>
          {#if confirmDelete === entry.path}
            <span class="acts">
              <button class="danger" onclick={() => remove(entry.path)}>Delete</button>
              <button onclick={() => (confirmDelete = null)}>Keep</button>
            </span>
          {:else}
            <span class="acts">
              <button title="Download to your Downloads folder" aria-label="Download {entry.name}" onclick={() => download(entry)}><Icon name="arrow-down" size={14} /></button>
              <button title="Rename" aria-label="Rename {entry.name}" onclick={() => (edit = { kind: "rename", path: entry.path, value: entry.name })}><Icon name="pencil" size={13} /></button>
              <button title="Delete" aria-label="Delete {entry.name}" onclick={() => (confirmDelete = entry.path)}><Icon name="trash" size={14} /></button>
            </span>
          {/if}
        </li>
      {:else}
        {#if !loading}<li class="empty">This folder is empty. Drop files here to upload.</li>{/if}
      {/each}
    </ul>

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
              {:else if t.total > 0}
                <span class="small">{sizeText(t.total)}</span>
              {/if}
              {#if running}
                <Button variant="icon" title="Cancel" aria-label="Cancel transfer" onclick={() => sftpCancel(conn!, t.id)}><Icon name="stop" size={13} /></Button>
              {/if}
            </div>
            {#if t.state !== "failed" && t.state !== "cancelled"}
              <div class="bar" role="progressbar" aria-label={t.name} aria-valuemin="0" aria-valuemax="100" aria-valuenow={t.state === "done" ? 100 : t.total ? Math.round((t.done / t.total) * 100) : 0}>
                <div class="fill" style:width={`${t.state === "done" ? 100 : t.total ? Math.min(100, (t.done / t.total) * 100) : 0}%`}></div>
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
  .plabel {
    flex: none;
    color: var(--dim);
    font-family: var(--font-mono);
    font-size: var(--text-s);
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
  .list.loading {
    opacity: 0.5;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
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
  .size {
    min-width: 56px;
  }
  .date {
    min-width: 52px;
  }
  .acts {
    display: none;
    flex: none;
  }
  .row:hover .acts,
  .row:focus-within .acts {
    display: flex;
  }
  /* Hover actions take the place of the size and date. */
  .row:hover .meta,
  .row:focus-within .meta {
    display: none;
  }
  .acts button {
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 24px;
    min-height: 24px;
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
