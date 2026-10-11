<script lang="ts">
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
    sftpRename,
    sftpUpload,
    sftpCanonical,
  } from "./ipc";
  import { focus } from "./focus";
  import type { Entry } from "./bindings/Entry";
  import type { SftpTransfer } from "./bindings/SftpTransfer";
  import type { SshTarget } from "./bindings/SshTarget";

  type Saved = { id: number; label: string; secret?: string | null };
  let {
    ssh = null,
    saved = null,
    active,
    onclose,
  }: { ssh?: SshTarget | null; saved?: Saved | null; active: boolean; onclose: () => void } = $props();

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

  const sizeText = (n: number) =>
    n < 1024 ? `${n} B` : n < 1024 ** 2 ? `${(n / 1024).toFixed(1)} KB` : n < 1024 ** 3 ? `${(n / 1024 ** 2).toFixed(1)} MB` : `${(n / 1024 ** 3).toFixed(1)} GB`;
  const dateText = (s: number | null) => (s == null ? "" : new Date(s * 1000).toLocaleDateString());
  const join = (dir: string, name: string) => (dir.endsWith("/") ? dir + name : `${dir}/${name}`);
  const parent = (p: string) => p.replace(/\/[^/]+\/?$/, "") || "/";

  async function connect() {
    status = "connecting";
    error = null;
    try {
      const opened = saved ? await sftpOpenSaved(saved.id, saved.secret ?? null) : await sftpOpen(ssh!);
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
  <header>
    <strong>Files</strong>
    <button title="Close file panel" aria-label="Close file panel" onclick={onclose}>×</button>
  </header>

  {#if status === "connecting"}
    <p class="msg">Connecting…</p>
  {:else if status === "failed"}
    <p class="msg err">{error}</p>
    <p class="msg"><button class="btn" onclick={connect}>Try again</button></p>
  {:else}
    <div class="bar">
      <button title="Parent folder" aria-label="Parent folder" disabled={path === "/"} onclick={() => go(parent(path))}>↑</button>
      <input
        class="path"
        aria-label="Remote path"
        bind:value={pathInput}
        onkeydown={(e) => e.key === "Enter" && go(pathInput)}
        spellcheck="false"
      />
      <button title="Refresh" aria-label="Refresh" onclick={reload}>⟳</button>
    </div>
    <div class="bar actions">
      <button class="btn" onclick={() => (edit = { kind: "mkdir", value: "" })}>New folder</button>
      <button class="btn" onclick={chooseFiles}>Upload files</button>
      <button class="btn" onclick={chooseFolder}>Upload folder</button>
    </div>
    {#if error}<p class="msg err">{error}</p>{/if}

    <ul class="list" class:loading>
      {#if edit?.kind === "mkdir"}
        <li class="row">
          <span class="icon">📁</span>
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
        <li class="row">
          <span class="icon">{entry.kind === "dir" ? "📁" : entry.kind === "symlink" ? "🔗" : "📄"}</span>
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
              onclick={(e) => e.detail === 1 && entry.kind === "dir" && go(entry.path)}
            >
              {entry.name}
            </button>
          {/if}
          <span class="meta">{entry.kind === "dir" ? "" : sizeText(entry.size)}</span>
          <span class="meta date">{dateText(entry.modified)}</span>
          {#if confirmDelete === entry.path}
            <span class="acts">
              <button class="danger" onclick={() => remove(entry.path)}>Delete</button>
              <button onclick={() => (confirmDelete = null)}>Keep</button>
            </span>
          {:else}
            <span class="acts">
              <button title="Download to your Downloads folder" aria-label="Download {entry.name}" onclick={() => download(entry)}>⬇</button>
              <button title="Rename" aria-label="Rename {entry.name}" onclick={() => (edit = { kind: "rename", path: entry.path, value: entry.name })}>✎</button>
              <button title="Delete" aria-label="Delete {entry.name}" onclick={() => (confirmDelete = entry.path)}>🗑</button>
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
          <div class="t">
            <span class="tname">{t.direction === "upload" ? "⬆" : "⬇"} {t.name}</span>
            {#if t.state === "failed"}
              <span class="err small" title={t.message}>{t.message}</span>
            {:else if t.state === "done"}
              <span class="small">Done</span>
            {:else if t.state === "cancelled"}
              <span class="small">Cancelled</span>
            {:else}
              <progress max={t.total || 1} value={t.done}></progress>
              <button title="Cancel" aria-label="Cancel transfer" onclick={() => sftpCancel(conn!, t.id)}>×</button>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</aside>

<style>
  aside {
    width: 380px;
    min-width: 280px;
    display: flex;
    flex-direction: column;
    background: #1b1c21;
    border-left: 1px solid #2b2d36;
    font-size: 12.5px;
  }
  aside.dropping {
    outline: 2px dashed #6ea8fe;
    outline-offset: -4px;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 10px;
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
    padding: 2px 6px;
    border-radius: 4px;
  }
  button:hover:not(:disabled) {
    background: #2b2d36;
  }
  button:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .bar {
    display: flex;
    gap: 4px;
    padding: 2px 8px;
  }
  .path,
  .name-input {
    flex: 1;
    min-width: 0;
    background: #111;
    border: 1px solid #2b2d36;
    border-radius: 4px;
    padding: 3px 6px;
  }
  .btn {
    background: #2b2d36;
  }
  .msg {
    margin: 8px 10px;
  }
  .err {
    color: #ff8a80;
    word-break: break-word;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    list-style: none;
    margin: 6px 0 0;
    padding: 0;
  }
  .list.loading {
    opacity: 0.5;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 8px;
  }
  .row:hover {
    background: #22232a;
  }
  .name {
    flex: 1;
    min-width: 0;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    opacity: 0.55;
    font-size: 11.5px;
    white-space: nowrap;
  }
  .date {
    min-width: 62px;
    text-align: right;
  }
  .acts {
    display: flex;
    opacity: 0;
  }
  .row:hover .acts,
  .row:focus-within .acts {
    opacity: 1;
  }
  .danger {
    color: #ff8a80;
  }
  .empty {
    padding: 14px 10px;
    opacity: 0.55;
  }
  .transfers {
    border-top: 1px solid #2b2d36;
    max-height: 160px;
    overflow-y: auto;
    padding: 4px 8px;
  }
  .thead {
    display: flex;
    justify-content: space-between;
    opacity: 0.7;
  }
  .t {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 0;
  }
  .tname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .small {
    font-size: 11.5px;
    max-width: 55%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  progress {
    width: 110px;
  }
</style>
