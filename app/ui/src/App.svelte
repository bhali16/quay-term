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
    sshAuthResponse,
    onAuthPrompt,
    snippetDelete,
    snippetSave,
    snippetsLoad,
  } from "./lib/ipc";
  import Sidebar from "./lib/Sidebar.svelte";
  import SessionDialog from "./lib/SessionDialog.svelte";
  import type { Result as SessionResult } from "./lib/SessionDialog.svelte";
  import PasswordPrompt from "./lib/PasswordPrompt.svelte";
  import type { LibraryView } from "./lib/bindings/LibraryView";
  import type { SavedSession } from "./lib/bindings/SavedSession";
  import TerminalPane from "./lib/TerminalPane.svelte";
  import FilePanel from "./lib/FilePanel.svelte";
  import TunnelPanel from "./lib/TunnelPanel.svelte";
  import ConnectDialog from "./lib/ConnectDialog.svelte";
  import TrustDialog from "./lib/TrustDialog.svelte";
  import AuthPromptDialog from "./lib/AuthPromptDialog.svelte";
  import type { AuthPrompt } from "./lib/bindings/AuthPrompt";
  import Icon from "./lib/Icon.svelte";
  import SettingsDialog from "./lib/SettingsDialog.svelte";
  import SnippetDialog from "./lib/SnippetDialog.svelte";
  import type { Target } from "./lib/SnippetDialog.svelte";
  import type { Snippet } from "./lib/bindings/Snippet";
  import { settings } from "./lib/settings.svelte";
  import { actionFor, isMac, type Action } from "./lib/keys";
  import * as P from "./lib/panes";
  import type { AppInfo } from "./lib/bindings/AppInfo";
  import type { HostKeyPrompt } from "./lib/bindings/HostKeyPrompt";
  import type { SshTarget } from "./lib/bindings/SshTarget";

  type Saved = { id: number; label: string; secret?: string | null; remember?: boolean };
  // `layout` is the split tree; every leaf is a terminal that opens the tab's own target (a new login for SSH).
  type Tab = {
    key: number;
    title: string;
    ssh: SshTarget | null;
    saved: Saved | null;
    files?: boolean;
    tunnels?: boolean;
    layout: P.Node;
    focus: number;
    panes: Record<number, TerminalPane>;
  };

  let info = $state<AppInfo | null>(null);
  let tabs = $state<Tab[]>([]);
  let activeKey = $state(0);
  let counter = 0;
  let paneCounter = 0;
  let editing = $state<number | null>(null);
  let connecting = $state(false);
  let trustPrompt = $state<HostKeyPrompt | null>(null);
  let authPrompt = $state<AuthPrompt | null>(null);
  let showSettings = $state(false);

  // ---- multi-exec and snippets ----
  // Panes whose typing is mirrored to each other. Pane ids are unique across tabs, so a tab needs no part in the key.
  let broadcast = $state<number[]>([]);
  let snippets = $state<Snippet[]>([]);
  let showSnippets = $state(false);
  const paneById = (id: number) => tabs.map((t) => t.panes[id]).find(Boolean);
  function toggleBroadcast(ids: number[]) {
    const all = ids.every((i) => broadcast.includes(i));
    broadcast = all ? broadcast.filter((i) => !ids.includes(i)) : [...new Set([...broadcast, ...ids])];
  }
  // Typing in a selected pane goes to the other selected ones; typing elsewhere stays put.
  function mirror(from: number, data: string) {
    if (broadcast.includes(from)) for (const id of broadcast) if (id !== from) paneById(id)?.send(data);
  }
  function sendSnippet(text: string, target: Target) {
    const tab = activeTab;
    if (!tab) return;
    const ids =
      target === "focused" ? [tab.focus] : target === "tab" ? P.leaves(tab.layout) : broadcast;
    for (const id of ids) paneById(id)?.send(text);
    showSnippets = false;
  }
  async function saveSnippet(id: number | null, name: string, folder: string, body: string) {
    try {
      await snippetSave(id, name, folder, body);
      snippets = await snippetsLoad();
      return null;
    } catch (e) {
      return `${e}`;
    }
  }
  async function removeSnippet(s: Snippet) {
    try {
      await snippetDelete(s.id);
      snippets = await snippetsLoad();
    } catch (e) {
      say(e);
    }
  }

  function newTab(ssh: SshTarget | null = null, saved: Saved | null = null) {
    const key = ++counter;
    const title = saved ? saved.label : ssh ? `${ssh.user}@${ssh.host}` : `Shell ${key}`;
    // A saved session comes back with the split layout it was last closed with.
    const layout = (saved && loadShape(saved.id)) || { leaf: ++paneCounter };
    tabs.push({ key, title, ssh, saved, layout, focus: P.leaves(layout)[0], panes: {} });
    activeKey = key;
  }

  const shapeKey = (id: number) => `quay.layout.${id}`;
  function loadShape(id: number): P.Node | null {
    try {
      const raw = localStorage.getItem(shapeKey(id));
      return raw ? P.fromShape(JSON.parse(raw), () => ++paneCounter) : null;
    } catch {
      return null;
    }
  }
  function setLayout(tab: Tab, layout: P.Node) {
    tab.layout = layout;
    if (!tab.saved) return;
    try {
      if (P.isLeaf(layout)) localStorage.removeItem(shapeKey(tab.saved.id));
      else localStorage.setItem(shapeKey(tab.saved.id), JSON.stringify(P.shapeOf(layout)));
    } catch {
      /* layout memory is optional */
    }
  }
  const focusedPane = (tab: Tab | undefined) => tab?.panes[tab.focus];

  function splitPane(tab: Tab, axis: P.Axis) {
    const id = ++paneCounter;
    setLayout(tab, P.split(tab.layout, tab.focus, id, axis));
    tab.focus = id;
  }
  function closePane(tab: Tab, id: number) {
    const next = P.remove(tab.layout, id);
    if (!next) return closeTab(tab.key);
    const order = P.leaves(tab.layout);
    setLayout(tab, next);
    delete tab.panes[id];
    broadcast = broadcast.filter((b) => b !== id);
    if (tab.focus === id) tab.focus = P.leaves(next)[Math.min(order.indexOf(id), order.length - 2)];
  }
  function moveFocus(tab: Tab, dir: P.Dir) {
    const to = P.neighbor(P.layout(tab.layout), tab.focus, dir);
    if (to !== null) tab.focus = to;
  }
  function dragDivider(e: PointerEvent, tab: Tab, d: P.Divider) {
    const area = (e.currentTarget as HTMLElement).parentElement!.getBoundingClientRect();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const move = (m: PointerEvent) => {
      const row = d.axis === "row";
      const pos = row ? (m.clientX - area.left) / area.width : (m.clientY - area.top) / area.height;
      const start = row ? d.parent.x : d.parent.y;
      const size = row ? d.parent.w : d.parent.h;
      setLayout(tab, P.setRatio(tab.layout, d.path, (pos - start) / size));
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
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

  function answerAuth(answers: string[] | null) {
    if (authPrompt) sshAuthResponse(authPrompt.prompt_id, answers);
    authPrompt = null;
  }

  function closeTab(key: number) {
    const i = tabs.findIndex((t) => t.key === key);
    if (i < 0) return;
    const gone = P.leaves(tabs[i].layout);
    broadcast = broadcast.filter((b) => !gone.includes(b));
    tabs.splice(i, 1);
    if (activeKey === key) activeKey = tabs[Math.min(i, tabs.length - 1)]?.key ?? 0;
    if (tabs.length === 0) newTab();
  }

  function runAction(a: Action) {
    const i = tabs.findIndex((t) => t.key === activeKey);
    switch (a) {
      case "new_tab": return newTab();
      case "close_tab": return activeTab && closePane(activeTab, activeTab.focus);
      case "broadcast_pane": return activeTab && toggleBroadcast([activeTab.focus]);
      case "broadcast_tab": return activeTab && toggleBroadcast(P.leaves(activeTab.layout));
      case "snippets": return (showSnippets = !showSnippets);
      case "split_right": return activeTab && splitPane(activeTab, "row");
      case "split_down": return activeTab && splitPane(activeTab, "col");
      case "focus_left": return activeTab && moveFocus(activeTab, "left");
      case "focus_right": return activeTab && moveFocus(activeTab, "right");
      case "focus_up": return activeTab && moveFocus(activeTab, "up");
      case "focus_down": return activeTab && moveFocus(activeTab, "down");
      case "resize_left": return activeTab && setLayout(activeTab, P.nudge(activeTab.layout, activeTab.focus, "left"));
      case "resize_right": return activeTab && setLayout(activeTab, P.nudge(activeTab.layout, activeTab.focus, "right"));
      case "resize_up": return activeTab && setLayout(activeTab, P.nudge(activeTab.layout, activeTab.focus, "up"));
      case "resize_down": return activeTab && setLayout(activeTab, P.nudge(activeTab.layout, activeTab.focus, "down"));
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
    snippetsLoad().then((s) => (snippets = s)).catch(say);
    const unlisten = [
      onSessionExit(({ id, reason }) => {
        for (const t of tabs) for (const p of Object.values(t.panes)) if (p?.sessionId() === id) p.notifyExited(reason);
      }),
      onHostKeyPrompt((p) => (trustPrompt = p)),
      onAuthPrompt((p) => (authPrompt = p)),
    ];
    return () => {
      unlisten.forEach((u) => u.then((f) => f()));
    };
  });
</script>

<svelte:window {onkeydown} />

<main class:mac={isMac} class:rail={!sidebarOpen}>
  <!-- One button for both states, next to the traffic lights, so it never moves when the sidebar does. -->
  <button
    class="sbtoggle"
    aria-label={sidebarOpen ? "Hide sidebar" : "Show sidebar"}
    aria-expanded={sidebarOpen}
    title={`${sidebarOpen ? "Hide" : "Show"} sidebar (${settings.bindings.toggle_sidebar})`}
    onclick={toggleSidebar}
  >
    <Icon name="panel-left" size={17} />
  </button>
  <Sidebar
    railOnly={!sidebarOpen}
    {library}
    bind:query
    oncollapse={toggleSidebar}
    onshell={() => newTab()}
    onssh={() => (connecting = true)}
    shellKey={settings.bindings.new_tab}
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
  <div class="col">
  <nav class="tabs" data-tauri-drag-region>
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
            {#if tab.ssh || tab.saved}<span class="sshdot" title="SSH session"></span>{/if}
            <span class="ttl">{tab.title}</span>
          </button>
        {/if}
        <button class="close" aria-label="Close tab" onclick={() => closeTab(tab.key)}>×</button>
      </div>
    {/each}
    <div class="right">
      {#if activeTab && (activeTab.ssh || activeTab.saved)}
        <button class="filesbtn" class:on={activeTab.files} aria-label="Files" aria-pressed={!!activeTab.files} title="Browse files on this host (SFTP)" onclick={() => (activeTab.files = !activeTab.files)}>
          <Icon name="folder" size={16} />Files
        </button>
        <button class="filesbtn" class:on={activeTab.tunnels} aria-label="Tunnels" aria-pressed={!!activeTab.tunnels} title="Port forwards over this connection" onclick={() => (activeTab.tunnels = !activeTab.tunnels)}>
          <Icon name="swap" size={16} />Tunnels
        </button>
      {/if}
      {#if broadcast.length}
        <button class="filesbtn on warn" title="Stop broadcasting" aria-label="Stop broadcasting to {broadcast.length} panes" onclick={() => (broadcast = [])}>
          <Icon name="x" size={14} />Broadcast {broadcast.length}
        </button>
      {/if}
      <button class="filesbtn" title={`Snippets (${settings.bindings.snippets})`} aria-label="Snippets" onclick={() => (showSnippets = true)}>
        <Icon name="terminal" size={16} />Snippets
      </button>
      <button class="settings" title="Settings" aria-label="Settings" onclick={() => (showSettings = true)}><Icon name="settings" size={18} /></button>
    </div>
  </nav>
  <div class="panes">
    {#each tabs as tab (tab.key)}
      {@const lay = P.layout(tab.layout)}
      {@const solo = lay.panes.length === 1}
      <div class="tabview" class:hidden={tab.key !== activeKey}>
        <div class="term">
          {#each lay.panes as p (p.id)}
            <div
              class="leaf"
              class:focused={!solo && tab.focus === p.id}
              style:left="{p.rect.x * 100}%"
              style:top="{p.rect.y * 100}%"
              style:width="{p.rect.w * 100}%"
              style:height="{p.rect.h * 100}%"
            >
              <TerminalPane
                bind:this={tab.panes[p.id]}
                active={tab.key === activeKey}
                focused={tab.focus === p.id}
                compact={!solo}
                onfocus={() => (tab.focus = p.id)}
                oninput={(d) => mirror(p.id, d)}
                broadcasting={broadcast.includes(p.id)}
                ssh={tab.ssh}
                saved={tab.saved}
                onconnected={tab.saved?.remember ? refresh : undefined}
              />
            </div>
          {/each}
          {#each lay.dividers as d (d.path)}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="divider {d.axis}"
              style:left="{d.rect.x * 100}%"
              style:top="{d.rect.y * 100}%"
              style:width={d.axis === "col" ? `${d.rect.w * 100}%` : undefined}
              style:height={d.axis === "row" ? `${d.rect.h * 100}%` : undefined}
              onpointerdown={(e) => dragDivider(e, tab, d)}
            ></div>
          {/each}
        </div>
        {#if tab.tunnels}
          <TunnelPanel
            terminalId={() => focusedPane(tab)?.sessionId() ?? null}
            saved={library.sessions.find((x) => x.id === tab.saved?.id)?.tunnels ?? []}
            active={tab.key === activeKey}
            onclose={() => (tab.tunnels = false)}
          />
        {/if}
        {#if tab.files}
          <FilePanel ssh={tab.ssh} saved={tab.saved} active={tab.key === activeKey} terminalId={() => focusedPane(tab)?.sessionId() ?? null} onclose={() => (tab.files = false)} />
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
        sessions={library.sessions}
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
        <span class="ticon"><Icon name={t.kind === "error" ? "x" : t.kind === "success" ? "check" : "info"} size={16} /></span>
        <span class="msg">{t.text}</span>
        <button aria-label="Dismiss" onclick={() => dismiss(t.id)}><Icon name="x" size={14} /></button>
      </div>
    {/each}
  </div>
  {#if showSnippets}
    <SnippetDialog
      {snippets}
      tabPanes={activeTab ? P.leaves(activeTab.layout).length : 0}
      broadcast={broadcast.length}
      onsend={sendSnippet}
      onsave={saveSnippet}
      ondelete={removeSnippet}
      onclose={() => (showSnippets = false)}
    />
  {/if}
  {#if showSettings}
    <SettingsDialog onclose={() => (showSettings = false)} />
  {/if}
  {#if trustPrompt}
    <TrustDialog prompt={trustPrompt} onanswer={answerTrust} />
  {/if}
  {#if authPrompt}
    {#key authPrompt}
      <AuthPromptDialog prompt={authPrompt} onanswer={answerAuth} />
    {/key}
  {/if}
</main>

<style>
  /* No title bar: the sidebar runs the full height of the window and the tab strip
     starts after it. macOS draws the traffic lights over the top-left corner (overlay
     title bar, positioned in tauri.conf.json), and the sidebar toggle sits right after them. */
  main {
    --toggle: 28px;
    --toggle-x: 12px;
    position: relative;
    display: flex;
    height: 100%;
  }
  main.mac {
    --toggle-x: 86px;
  }
  .sbtoggle {
    position: absolute;
    z-index: 5;
    top: calc((var(--chrome-h) - var(--toggle)) / 2);
    left: var(--toggle-x);
    display: flex;
    align-items: center;
    justify-content: center;
    width: var(--toggle);
    height: var(--toggle);
    border-radius: 6px;
    color: var(--dim);
    cursor: pointer;
  }
  .sbtoggle:hover {
    color: var(--fg);
    background: var(--hover);
  }
  .col {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }
  nav {
    display: flex;
    align-items: flex-end;
    gap: 4px;
    flex: none;
    box-sizing: border-box;
    height: var(--chrome-h);
    padding: 0 10px;
    background: var(--bg-alt);
    border-bottom: 1px solid var(--border);
    font-size: 14px;
    transition: padding-left 160ms ease-out;
  }
  /* The collapsed rail is narrower than the traffic lights plus the toggle, so the tabs
     start after the toggle instead of under it. */
  main.rail nav {
    padding-left: max(10px, calc(var(--toggle-x) + var(--toggle) + 8px - var(--rail-w)));
  }
  @media (prefers-reduced-motion: reduce) {
    nav {
      transition: none;
    }
  }
  .tab {
    display: flex;
    align-items: center;
    flex: 0 1 auto;
    min-width: 0;
    max-width: 220px;
    height: 34px;
    border-radius: 8px 8px 0 0;
    color: var(--dim);
  }
  .tab:hover {
    background: var(--hover);
  }
  /* Covers the strip's bottom border so the active tab and the terminal read as one surface. */
  .tab.active {
    height: 35px;
    margin-bottom: -1px;
    background: var(--bg);
    color: var(--fg);
  }
  .sshdot {
    flex: none;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--green);
  }
  button,
  input {
    font: inherit;
    color: inherit;
    background: none;
    border: 0;
  }
  .label {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 100%;
    min-width: 0;
    padding: 0 4px 0 16px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    cursor: pointer;
  }
  .ttl {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  input {
    width: 90px;
    margin: 0 4px 0 12px;
    padding: 2px 4px;
    border-radius: 4px;
    background: var(--input);
    color: var(--fg);
  }
  .close {
    padding: 4px 10px 4px 6px;
    opacity: 0.7;
    cursor: pointer;
  }
  .close:hover {
    opacity: 1;
  }
  /* Centred in the strip, on the same line as the traffic lights and the sidebar toggle. */
  .right {
    display: flex;
    align-items: center;
    align-self: center;
    gap: 6px;
    margin-left: auto;
    padding-right: 2px;
  }
  .filesbtn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px;
    border-radius: 6px;
    color: var(--dim);
    font-size: 13px;
    cursor: pointer;
  }
  .filesbtn:hover {
    color: var(--fg);
    background: var(--hover);
  }
  .filesbtn.on {
    color: var(--accent);
  }
  .filesbtn.warn {
    color: var(--red);
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
    border-left-color: var(--green);
  }
  .toast.error {
    border-left-color: var(--red);
  }
  .ticon {
    display: flex;
    flex: none;
    margin-top: 1px;
    color: var(--accent);
  }
  .toast.success .ticon {
    color: var(--green);
  }
  .toast.error .ticon {
    color: var(--red);
  }
  .toast .msg {
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
  @media (prefers-reduced-motion: reduce) {
    .toast {
      animation: none;
    }
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
    padding: 5px 6px;
    border-radius: 6px;
    color: var(--dim);
    cursor: pointer;
  }
  .settings:hover {
    color: var(--fg);
    background: var(--hover);
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
  /* Each pane sits at its fraction of the tab area, so splitting never remounts (and never restarts) a terminal. */
  .leaf {
    position: absolute;
    box-sizing: border-box;
    border: 1px solid transparent;
  }
  .leaf.focused {
    border-color: var(--accent);
  }
  .divider {
    position: absolute;
    z-index: 2;
    touch-action: none;
  }
  .divider.row {
    width: 7px;
    margin-left: -3px;
    cursor: col-resize;
  }
  .divider.col {
    height: 7px;
    margin-top: -3px;
    cursor: row-resize;
  }
  .divider:hover {
    background: var(--accent);
    opacity: 0.4;
  }
  .panes {
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 0;
  }
</style>
