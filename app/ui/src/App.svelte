<script lang="ts">
  import { onMount } from "svelte";
  import {
    folderCreate,
    folderDelete,
    folderMove,
    folderRename,
    getAppInfo,
    importSshConfig,
    libraryLoad,
    onHostKeyPrompt,
    onSessionExit,
    sessionDelete,
    sessionMove,
    sessionSave,
    sshTrustResponse,
  } from "./lib/ipc";
  import Sidebar from "./lib/Sidebar.svelte";
  import SessionDialog from "./lib/SessionDialog.svelte";
  import type { Result as SessionResult } from "./lib/SessionDialog.svelte";
  import PasswordPrompt from "./lib/PasswordPrompt.svelte";
  import type { LibraryView } from "./lib/bindings/LibraryView";
  import type { SavedSession } from "./lib/bindings/SavedSession";
  import TerminalPane from "./lib/TerminalPane.svelte";
  import FilePanel from "./lib/FilePanel.svelte";
  import ConnectDialog from "./lib/ConnectDialog.svelte";
  import TrustDialog from "./lib/TrustDialog.svelte";
  import Icon from "./lib/Icon.svelte";
  import SettingsDialog from "./lib/SettingsDialog.svelte";
  import { settings } from "./lib/settings.svelte";
  import { actionFor, isMac, type Action } from "./lib/keys";
  import type { AppInfo } from "./lib/bindings/AppInfo";
  import type { HostKeyPrompt } from "./lib/bindings/HostKeyPrompt";
  import type { SshTarget } from "./lib/bindings/SshTarget";

  type Saved = { id: number; label: string; secret?: string | null; remember?: boolean };
  type Tab = { key: number; title: string; ssh: SshTarget | null; saved: Saved | null; files?: boolean; pane?: TerminalPane };

  let info = $state<AppInfo | null>(null);
  let tabs = $state<Tab[]>([]);
  let activeKey = $state(0);
  let counter = 0;
  let editing = $state<number | null>(null);
  let connecting = $state(false);
  let trustPrompt = $state<HostKeyPrompt | null>(null);
  let showSettings = $state(false);
  let newMenu = $state(false);

  function newTab(ssh: SshTarget | null = null, saved: Saved | null = null) {
    const key = ++counter;
    const title = saved ? saved.label : ssh ? `${ssh.user}@${ssh.host}` : `Shell ${key}`;
    tabs.push({ key, title, ssh, saved });
    activeKey = key;
  }

  // ---- saved sessions ----
  let library = $state<LibraryView>({ folders: [], sessions: [] });
  let query = $state("");
  type Toast = { id: number; kind: "info" | "success" | "error"; text: string };
  let toasts = $state<Toast[]>([]);
  let toastId = 0;
  let sidebarOpen = $state(readSidebarPref());
  let editing_session = $state<{ session: SavedSession | null; folderId: number | null } | null>(null);
  let dialogError = $state<string | null>(null);
  let passwordFor = $state<SavedSession | null>(null);

  function readSidebarPref() {
    try {
      return localStorage.getItem("quay.sidebar") !== "closed";
    } catch {
      return true;
    }
  }
  function toggleSidebar() {
    sidebarOpen = !sidebarOpen;
    try {
      localStorage.setItem("quay.sidebar", sidebarOpen ? "open" : "closed");
    } catch {
      /* preference is optional */
    }
  }
  function toast(text: string, kind: Toast["kind"] = "info") {
    const id = ++toastId;
    toasts.push({ id, kind, text });
    setTimeout(() => dismiss(id), kind === "error" ? 8000 : 4500);
  }
  const dismiss = (id: number) => (toasts = toasts.filter((t) => t.id !== id));
  const say = (e: unknown) => toast(`${e}`, "error");
  async function refresh() {
    try {
      library = await libraryLoad(query.trim() || undefined);
    } catch (e) {
      say(e);
    }
  }
  // Re-run the search shortly after typing stops.
  let searchTimer: ReturnType<typeof setTimeout>;
  $effect(() => {
    query;
    clearTimeout(searchTimer);
    searchTimer = setTimeout(refresh, 150);
    return () => clearTimeout(searchTimer);
  });

  function connectSaved(s: SavedSession, secret: string | null = null, remember = false) {
    if (s.auth === "password" && !s.has_secret && secret === null) {
      passwordFor = s;
      return;
    }
    newTab(null, { id: s.id, label: s.name, secret, remember });
  }

  async function submitSession(r: SessionResult) {
    dialogError = null;
    try {
      const saved = await sessionSave(editing_session?.session?.id ?? null, r.input, r.secret);
      editing_session = null;
      await refresh();
      if (r.connect) connectSaved(saved, r.transient);
    } catch (e) {
      dialogError = `${e}`;
      await refresh();
    }
  }
  async function removeSession(s: SavedSession) {
    try {
      await sessionDelete(s.id);
      await refresh();
    } catch (e) {
      say(e);
    }
  }
  async function doImport() {
    try {
      const r = await importSshConfig();
      if (r.added === 0 && r.skipped === 0) toast("No hosts found in ~/.ssh/config.");
      else if (r.added === 0) toast(`Nothing new to import: ${r.skipped} host(s) already saved.`);
      else toast(`Imported ${r.added} host(s)${r.skipped ? `, ${r.skipped} already saved` : ""}.`, "success");
      await refresh();
    } catch (e) {
      say(e);
    }
  }
  const folderAction = (run: () => Promise<unknown>) => async () => {
    try {
      await run();
      await refresh();
    } catch (e) {
      say(e);
    }
  };

  const activeTab = $derived(tabs.find((t) => t.key === activeKey));

  function connectSsh(target: SshTarget) {
    connecting = false;
    newTab(target);
  }

  function answerTrust(trust: boolean) {
    if (trustPrompt) sshTrustResponse(trustPrompt.prompt_id, trust);
    trustPrompt = null;
  }

  function closeTab(key: number) {
    const i = tabs.findIndex((t) => t.key === key);
    if (i < 0) return;
    tabs.splice(i, 1);
    if (activeKey === key) activeKey = tabs[Math.min(i, tabs.length - 1)]?.key ?? 0;
    if (tabs.length === 0) newTab();
  }

  function runAction(a: Action) {
    const i = tabs.findIndex((t) => t.key === activeKey);
    switch (a) {
      case "new_tab": return newTab();
      case "close_tab": return closeTab(activeKey);
      case "next_tab": return (activeKey = tabs[(i + 1) % tabs.length].key);
      case "prev_tab": return (activeKey = tabs[(i - 1 + tabs.length) % tabs.length].key);
      case "toggle_sidebar": return toggleSidebar();
      case "toggle_files":
        if (activeTab && (activeTab.ssh || activeTab.saved)) activeTab.files = !activeTab.files;
        return;
      case "settings": return (showSettings = !showSettings);
      case "zoom_in": return zoom(1);
      case "zoom_out": return zoom(-1);
      case "zoom_reset": return settings.update({ ...settings.value, font_size: 14 });
    }
  }
  function zoom(d: number) {
    const size = Math.min(36, Math.max(8, settings.value.font_size + d));
    return settings.update({ ...settings.value, font_size: size });
  }
  function onkeydown(e: KeyboardEvent) {
    if (newMenu && e.key === "Escape") newMenu = false;
    if (showSettings) return; // the dialog records shortcuts itself
    const a = actionFor(e, settings.bindings);
    if (a) {
      e.preventDefault();
      runAction(a);
    }
  }

  onMount(() => {
    settings.load();
    getAppInfo().then((i) => (info = i));
    newTab();
    refresh();
    const unlisten = [
      onSessionExit(({ id, reason }) => {
        const tab = tabs.find((t) => t.pane?.sessionId() === id);
        tab?.pane?.notifyExited(reason);
      }),
      onHostKeyPrompt((p) => (trustPrompt = p)),
    ];
    return () => {
      unlisten.forEach((u) => u.then((f) => f()));
    };
  });
</script>

<svelte:window {onkeydown} onmousedown={(e) => newMenu && !(e.target as HTMLElement).closest(".newwrap") && (newMenu = false)} />

<main>
  <nav data-tauri-drag-region class:mac={isMac}>
    {#each tabs as tab (tab.key)}
      <div class="tab" class:active={tab.key === activeKey}>
        {#if editing === tab.key}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            autofocus
            bind:value={tab.title}
            onblur={() => (editing = null)}
            onkeydown={(e) => (e.key === "Enter" || e.key === "Escape") && (editing = null)}
          />
        {:else}
          <button class="label" onclick={() => (activeKey = tab.key)} ondblclick={() => (editing = tab.key)}>
            {tab.title}
          </button>
        {/if}
        <button class="close" aria-label="Close tab" onclick={() => closeTab(tab.key)}>×</button>
      </div>
    {/each}
    <div class="newwrap">
      <button class="add plus" aria-label="New tab" aria-haspopup="menu" aria-expanded={newMenu} title="New tab" onclick={() => (newMenu = !newMenu)}>
        <Icon name="plus" size={17} />
      </button>
      {#if newMenu}
        <div class="menu" role="menu">
          <button role="menuitem" onclick={() => ((newMenu = false), newTab())}>
            <Icon name="terminal" size={15} />Local shell<kbd>{settings.bindings.new_tab}</kbd>
          </button>
          <button role="menuitem" onclick={() => ((newMenu = false), (connecting = true))}>
            <Icon name="server" size={15} />SSH connection…
          </button>
        </div>
      {/if}
    </div>
    {#if activeTab && (activeTab.ssh || activeTab.saved)}
      <button class="add filesbtn" class:on={activeTab.files} aria-label="Files" title="Browse files on this host (SFTP)" onclick={() => (activeTab.files = !activeTab.files)}>
        <Icon name="folder-open" size={17} />
      </button>
    {/if}
    <button class="settings" title="Settings" aria-label="Settings" onclick={() => (showSettings = true)}><Icon name="settings" size={18} /></button>
  </nav>
  <div class="body">
      <Sidebar
        railOnly={!sidebarOpen}
        {library}
        bind:query
        oncollapse={toggleSidebar}
        openIds={tabs.flatMap((t) => (t.saved ? [t.saved.id] : []))}
        activeId={activeTab?.saved?.id ?? null}
        version={info ? `${info.name} v${info.version}` : ""}
        onconnect={(s) => connectSaved(s)}
        onedit={(s) => ((dialogError = null), (editing_session = { session: s, folderId: null }))}
        onnew={(folderId) => ((dialogError = null), (editing_session = { session: null, folderId }))}
        ondelete={removeSession}
        onimport={doImport}
        onnewfolder={(name, parentId) => folderAction(() => folderCreate(name, parentId))()}
        onrenamefolder={(id, name) => folderAction(() => folderRename(id, name))()}
        ondeletefolder={(id) => folderAction(() => folderDelete(id))()}
        onmovesession={(id, folderId) => folderAction(() => sessionMove(id, folderId))()}
        onmovefolder={(id, parentId) => folderAction(() => folderMove(id, parentId))()}
      />
    <div class="panes">
      {#each tabs as tab (tab.key)}
        <div class="tabview" class:hidden={tab.key !== activeKey}>
          <div class="term">
            <TerminalPane
              bind:this={tab.pane}
              active={tab.key === activeKey}
              ssh={tab.ssh}
              saved={tab.saved}
              onconnected={tab.saved?.remember ? refresh : undefined}
            />
          </div>
          {#if tab.files}
            <FilePanel ssh={tab.ssh} saved={tab.saved} active={tab.key === activeKey} onclose={() => (tab.files = false)} />
          {/if}
        </div>
      {/each}
    </div>
  </div>
  {#if editing_session}
    {#key editing_session}
      <SessionDialog
        session={editing_session.session}
        defaultFolderId={editing_session.folderId}
        folders={library.folders}
        error={dialogError}
        onsubmit={submitSession}
        oncancel={() => (editing_session = null)}
      />
    {/key}
  {/if}
  {#if passwordFor}
    <PasswordPrompt
      title={`${passwordFor.user}@${passwordFor.host}`}
      oncancel={() => (passwordFor = null)}
      onsubmit={(pw, remember) => {
        const s = passwordFor!;
        passwordFor = null;
        connectSaved(s, pw, remember);
      }}
    />
  {/if}
  {#if connecting}
    <ConnectDialog onconnect={connectSsh} oncancel={() => (connecting = false)} />
  {/if}
  <div class="toasts" aria-live="polite">
    {#each toasts as t (t.id)}
      <div class="toast {t.kind}" role="status">
        <Icon name={t.kind === "error" ? "info" : t.kind === "success" ? "check" : "info"} size={16} />
        <span>{t.text}</span>
        <button aria-label="Dismiss" onclick={() => dismiss(t.id)}><Icon name="x" size={14} /></button>
      </div>
    {/each}
  </div>
  {#if showSettings}
    <SettingsDialog onclose={() => (showSettings = false)} />
  {/if}
  {#if trustPrompt}
    <TrustDialog prompt={trustPrompt} onanswer={answerTrust} />
  {/if}
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  nav {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 40px;
    padding: 0 8px;
    background: var(--bg-deep);
    border-bottom: 1px solid var(--border);
    font-size: 13px;
  }
  /* macOS draws the traffic lights over the top-left of the window (overlay title bar). */
  nav.mac {
    padding-left: 82px;
  }
  .tab {
    display: flex;
    align-items: center;
    border-radius: 7px;
    border: 1px solid transparent;
  }
  .tab:hover {
    background: var(--hover);
  }
  .tab.active {
    background: var(--surface);
    border-color: var(--border);
  }
  button,
  input {
    font: inherit;
    color: inherit;
    background: none;
    border: 0;
  }
  .label {
    padding: 6px 4px 6px 10px;
    cursor: pointer;
  }
  input {
    width: 90px;
    margin: 4px 4px 4px 8px;
    padding: 2px 4px;
    background: var(--input);
  }
  .close,
  .add {
    padding: 4px 8px;
    cursor: pointer;
    opacity: 0.6;
  }
  .close:hover,
  .add:hover {
    opacity: 1;
  }
  .add.plus,
  .add.filesbtn {
    display: flex;
    padding: 6px;
    border-radius: 6px;
  }
  .newwrap {
    position: relative;
  }
  .menu {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    z-index: 20;
    min-width: 210px;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 10px;
    border-radius: 5px;
    text-align: left;
    cursor: pointer;
  }
  .menu button:hover {
    background: var(--hover);
  }
  .menu kbd {
    margin-left: auto;
    font: 11px ui-monospace, Menlo, monospace;
    opacity: 0.55;
  }
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    z-index: 40;
    display: flex;
    flex-direction: column;
    gap: 8px;
    pointer-events: none;
  }
  .toast {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    max-width: 360px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-left: 3px solid var(--accent);
    border-radius: 8px;
    background: var(--surface);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
    font-size: 13px;
    pointer-events: auto;
    animation: toast-in 0.15s ease-out;
  }
  .toast.success {
    border-left-color: #2f9e6b;
  }
  .toast.error {
    border-left-color: #d9534f;
  }
  .toast span {
    flex: 1;
    overflow-wrap: anywhere;
  }
  .toast button {
    display: flex;
    padding: 2px;
    cursor: pointer;
    opacity: 0.6;
  }
  .toast button:hover {
    opacity: 1;
  }
  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }
  .settings {
    display: flex;
    align-items: center;
    margin: 0 0 0 auto;
    padding: 5px 8px;
    border-radius: 6px;
    cursor: pointer;
    opacity: 0.55;
  }
  .settings:hover {
    opacity: 1;
    background: var(--hover);
  }
  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .tabview {
    position: absolute;
    inset: 0;
    display: flex;
  }
  .tabview.hidden {
    visibility: hidden;
    pointer-events: none;
  }
  .term {
    position: relative;
    flex: 1;
    min-width: 0;
  }
  .filesbtn.on {
    opacity: 1;
    background: var(--surface);
    border-radius: 6px;
  }
  .panes {
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 0;
  }
</style>
