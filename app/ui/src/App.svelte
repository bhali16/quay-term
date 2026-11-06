<script lang="ts">
  import { onMount } from "svelte";
  import { onAction, sendNotification } from "@tauri-apps/plugin-notification";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { commandBody, ensurePermission, isLong, shouldNotify } from "./lib/notify";
  import {
    folderCreate,
    folderDelete,
    folderMove,
    folderRename,
    getAppInfo,
    importSshConfig,
    libraryLoad,
    jumpSecretsMissing,
    onHostKeyPrompt,
    onSessionExit,
    sessionDelete,
    sessionMove,
    sessionSave,
    sshTrustResponse,
    sshAuthResponse,
    onAuthPrompt,
    snippetDelete,
    vaultStatus,
    snippetSave,
    snippetsLoad,
    tunnelList,
  } from "./lib/ipc";
  import Sidebar from "./lib/Sidebar.svelte";
  import SessionDialog from "./lib/SessionDialog.svelte";
  import type { Result as SessionResult } from "./lib/SessionDialog.svelte";
  import type { SessionInput } from "./lib/bindings/SessionInput";
  import PasswordPrompt from "./lib/PasswordPrompt.svelte";
  import { updates } from "./lib/updates.svelte";
  import type { LibraryView } from "./lib/bindings/LibraryView";
  import type { SavedSession } from "./lib/bindings/SavedSession";
  import TerminalPane from "./lib/TerminalPane.svelte";
  import type { PaneStatus } from "./lib/TerminalPane.svelte";
  import Palette from "./lib/Palette.svelte";
  import type { PaletteItem } from "./lib/Palette.svelte";
  import FilePanel from "./lib/FilePanel.svelte";
  import TunnelPanel from "./lib/TunnelPanel.svelte";
  import ConnectDialog from "./lib/ConnectDialog.svelte";
  import TrustDialog from "./lib/TrustDialog.svelte";
  import AuthPromptDialog from "./lib/AuthPromptDialog.svelte";
  import type { AuthPrompt } from "./lib/bindings/AuthPrompt";
  import Icon from "./lib/Icon.svelte";
  import SettingsDialog from "./lib/SettingsDialog.svelte";
  import SnippetDialog from "./lib/SnippetDialog.svelte";
  import UnlockDialog from "./lib/UnlockDialog.svelte";
  import type { VaultStatus } from "./lib/bindings/VaultStatus";
  import type { Target } from "./lib/SnippetDialog.svelte";
  import type { Snippet } from "./lib/bindings/Snippet";
  import { settings } from "./lib/settings.svelte";
  import { actionFor, actionLabels, isMac, type Action } from "./lib/keys";
  import * as P from "./lib/panes";
  import { runningTunnels, tabState, type TabState } from "./lib/tabs";
  import type { AppInfo } from "./lib/bindings/AppInfo";
  import type { HostKeyPrompt } from "./lib/bindings/HostKeyPrompt";
  import type { SshTarget } from "./lib/bindings/SshTarget";
  import type { JumpSecret } from "./lib/bindings/JumpSecret";
  import type { MissingSecret } from "./lib/bindings/MissingSecret";

  type Saved = { id: number; label: string; secret?: string | null; remember?: boolean; jumps?: JumpSecret[] };
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
    /** Port forwards running over this tab's connection, polled while the tab is open. */
    tunnelCount: number;
    /** Pane shown full-tab while zoomed; the others stay mounted and keep running, only hidden. */
    zoom: number | null;
    panes: Record<number, TerminalPane>;
    /** Connection state of each pane, reported by the pane itself. */
    status: Record<number, PaneStatus>;
    /** Output arrived while this tab was in the background. */
    activity: boolean;
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
  // Without a system keychain, secrets sit in a master-password file that must be unlocked first.
  let vault = $state<VaultStatus | null>(null);
  let showUnlock = $state(false);
  const loadVault = () => vaultStatus().then((v) => (vault = v)).catch(say);
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
  /** "Add to snippets" from a command block: saved under its first words; rename it in the Snippets dialog. */
  async function addCommandSnippet(command: string) {
    const name = command.replace(/\s+/g, " ").slice(0, 40);
    const err = await saveSnippet(null, name, "", command);
    toast(err ?? `Saved snippet "${name}".`, err ? "error" : "success");
  }
  async function removeSnippet(s: Snippet) {
    try {
      await snippetDelete(s.id);
      snippets = await snippetsLoad();
    } catch (e) {
      say(e);
    }
  }

  // ---- desktop notifications (window or tab in the background, pane not muted) ----
  // Panes whose alerts are off. Pane ids are unique across tabs.
  let muted = $state<number[]>([]);
  const toggleMute = (id: number) => (muted = muted.includes(id) ? muted.filter((m) => m !== id) : [...muted, id]);
  // What a click on a notification should show; the plugin reports the notification id back.
  const notified = new Map<number, { tab: number; pane: number }>();
  let notifyId = 0;
  let lastNotified: { tab: number; pane: number } | null = null;
  async function notify(tab: Tab, pane: number, title: string, body: string) {
    if (!shouldNotify({ enabled: settings.value.notifications, windowFocused: document.hasFocus(), tabActive: tab.key === activeKey, muted: muted.includes(pane) })) return;
    // Permission refused: say nothing here; Settings shows the note.
    if ((await ensurePermission()) !== "granted") return;
    try {
      const id = ++notifyId;
      lastNotified = { tab: tab.key, pane };
      notified.set(id, lastNotified);
      if (notified.size > 50) notified.delete(notified.keys().next().value!);
      sendNotification({ id, title, body });
    } catch {
      /* notifications are a convenience; never break the terminal */
    }
  }
  /** Click on a notification: bring the window forward and select the tab and pane it came from. */
  async function showNotified(id: number | undefined) {
    const t = (id !== undefined ? notified.get(id) : undefined) ?? lastNotified;
    try {
      const w = getCurrentWindow();
      await w.show();
      await w.unminimize();
      await w.setFocus();
    } catch {
      /* the tab switch below still helps */
    }
    const tab = t && tabs.find((x) => x.key === t.tab);
    if (!tab) return;
    activeKey = tab.key;
    if (tab.panes[t.pane]) tab.focus = t.pane;
  }
  function commandDone(tab: Tab, pane: number, c: { command: string; exit: number | null; ms: number }) {
    if (!isLong(c.ms, settings.value.notify_after_secs)) return;
    const body = commandBody({ ...c, session: tab.title, showCommand: settings.value.notify_show_command });
    notify(tab, pane, c.exit === null || c.exit === 0 ? "Command finished" : "Command failed", body);
  }

  function newTab(ssh: SshTarget | null = null, saved: Saved | null = null) {
    const key = ++counter;
    const title = saved ? saved.label : ssh ? `${ssh.user}@${ssh.host}` : `Shell ${key}`;
    // A saved session comes back with the split layout it was last closed with.
    const layout = (saved && loadShape(saved.id)) || { leaf: ++paneCounter };
    tabs.push({ key, title, ssh, saved, layout, focus: P.leaves(layout)[0], zoom: null, tunnelCount: 0, panes: {}, status: {}, activity: false });
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

  function toggleZoom(tab: Tab) {
    if (tab.zoom !== null) tab.zoom = null;
    else if (P.leaves(tab.layout).length > 1) tab.zoom = tab.focus;
  }
  function splitPane(tab: Tab, axis: P.Axis) {
    tab.zoom = null;
    const id = ++paneCounter;
    setLayout(tab, P.split(tab.layout, tab.focus, id, axis));
    tab.focus = id;
  }
  function closePane(tab: Tab, id: number) {
    const next = P.remove(tab.layout, id);
    if (!next) return closeTab(tab.key);
    const order = P.leaves(tab.layout);
    if (tab.zoom === id) tab.zoom = null;
    setLayout(tab, next);
    delete tab.panes[id];
    delete tab.status[id];
    broadcast = broadcast.filter((b) => b !== id);
    muted = muted.filter((m) => m !== id);
    if (tab.focus === id) tab.focus = P.leaves(next)[Math.min(order.indexOf(id), order.length - 2)];
  }
  function moveFocus(tab: Tab, dir: P.Dir) {
    if (tab.zoom !== null) return;
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

  // Drag a pane by its title strip onto an edge of another pane. Pointer events, not HTML5 drag and drop,
  // so it behaves the same in every webview.
  let drag = $state<{ tab: number; id: number; over: { id: number; edge: P.Dir } | null } | null>(null);
  function dragPane(e: PointerEvent, tab: Tab, id: number) {
    if (e.button !== 0 || tab.zoom !== null) return;
    const el = e.currentTarget as HTMLElement;
    const area = el.closest(".term")!.getBoundingClientRect();
    const x0 = e.clientX;
    const y0 = e.clientY;
    let moved = false;
    el.setPointerCapture(e.pointerId);
    const at = (m: PointerEvent) =>
      P.dropTarget(P.layout(tab.layout), (m.clientX - area.left) / area.width, (m.clientY - area.top) / area.height, id);
    const onMove = (m: PointerEvent) => {
      if (!moved && Math.hypot(m.clientX - x0, m.clientY - y0) < 5) return;
      moved = true;
      drag = { tab: tab.key, id, over: at(m) };
    };
    const end = (m: PointerEvent | null) => {
      el.removeEventListener("pointermove", onMove);
      el.removeEventListener("pointerup", onUp);
      el.removeEventListener("pointercancel", onCancel);
      drag = null;
      return m;
    };
    const onUp = (m: PointerEvent) => {
      const over = moved ? at(m) : null;
      end(m);
      if (!over) return;
      setLayout(tab, P.move(tab.layout, id, over.id, over.edge));
      tab.focus = id;
    };
    const onCancel = () => end(null);
    el.addEventListener("pointermove", onMove);
    el.addEventListener("pointerup", onUp);
    el.addEventListener("pointercancel", onCancel);
  }

  // ---- saved sessions ----
  let library = $state<LibraryView>({ folders: [], sessions: [] });
  let query = $state("");
  type Toast = { id: number; kind: "info" | "success" | "error"; text: string };
  let toasts = $state<Toast[]>([]);
  let toastId = 0;
  let sidebarOpen = $state(readSidebarPref());
  let editing_session = $state<{ session: SavedSession | null; folderId: number | null; draft?: SshTarget | null } | null>(null);
  /** Right-click menu on a tab. */
  let tabMenu = $state<{ x: number; y: number; key: number } | null>(null);
  let dialogError = $state<string | null>(null);
  let passwordFor = $state<SavedSession | null>(null);
  let jumpFor = $state<{
    session: SavedSession;
    secret: string | null;
    remember: boolean;
    jumps: JumpSecret[];
    hop: MissingSecret;
  } | null>(null);

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

  // A jump host with no saved password is asked for one at a time before the tab opens.
  async function connectSaved(
    s: SavedSession,
    secret: string | null = null,
    remember = false,
    jumps: JumpSecret[] = [],
  ) {
    if (s.auth === "password" && !s.has_secret && secret === null) {
      passwordFor = s;
      return;
    }
    const missing = await jumpSecretsMissing(s.id).catch(() => [] as MissingSecret[]);
    const hop = missing.find((m) => !jumps.some((j) => j.id === m.id));
    if (hop) {
      jumpFor = { session: s, secret, remember, jumps, hop };
      return;
    }
    newTab(null, { id: s.id, label: s.name, secret, remember, jumps });
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

  const vertical = $derived(settings.value.tab_layout === "vertical");
  const activeTab = $derived(tabs.find((t) => t.key === activeKey));
  // A tab you switch to has nothing unseen any more.
  $effect(() => {
    if (activeTab) activeTab.activity = false;
  });

  // ---- session colour, environment and connection state ----
  const savedOf = (tab: Tab) => (tab.saved ? library.sessions.find((x) => x.id === tab.saved!.id) : undefined);
  const envOf = (tab: Tab) => savedOf(tab)?.env ?? null;
  const colorOf = (tab: Tab) => savedOf(tab)?.color ?? null;
  const stateOf = (tab: Tab): TabState =>
    tabState(P.leaves(tab.layout).map((id) => tab.status[id]), !!(tab.ssh || tab.saved));
  const stateLabel: Record<TabState, string> = { connecting: "Connecting", connected: "Connected", ended: "Disconnected", local: "" };

  // ---- command palette ----
  let showPalette = $state(false);
  let paletteSessions = $state<SavedSession[]>([]);
  let snippetPick = $state<Snippet | null>(null);
  const RECENT_KEY = "quay.palette.recent";
  function readRecent(): string[] {
    try {
      const v = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
      return Array.isArray(v) ? v.filter((x) => typeof x === "string").slice(0, 8) : [];
    } catch {
      return [];
    }
  }
  let recent = $state<string[]>(readRecent());
  function remember(id: string) {
    recent = [id, ...recent.filter((x) => x !== id)].slice(0, 8);
    try {
      localStorage.setItem(RECENT_KEY, JSON.stringify(recent));
    } catch {
      /* recents are optional */
    }
  }
  function openPalette() {
    showPalette = true;
    paletteSessions = library.sessions;
    // The sidebar search may have narrowed `library`; the palette searches everything.
    libraryLoad().then((l) => (paletteSessions = l.sessions)).catch(() => {});
  }
  const paletteItems = $derived.by((): PaletteItem[] => {
    const acts = (Object.keys(actionLabels) as Action[])
      .filter((a) => a !== "palette")
      .map((a): PaletteItem => ({ id: `action:${a}`, group: "Actions", title: actionLabels[a], hint: settings.bindings[a], run: () => runAction(a) }));
    const extra: PaletteItem[] = [
      { id: "cmd:ssh", group: "Actions", title: "New SSH connection…", run: () => (connecting = true) },
      { id: "cmd:session", group: "Actions", title: "New saved session…", run: () => ((dialogError = null), (editing_session = { session: null, folderId: null })) },
      ...(activeTab?.ssh && !activeTab.saved
        ? [{ id: "cmd:save-connection", group: "Actions" as const, title: "Save this connection…", run: () => saveConnection(activeTab) }]
        : []),
      { id: "cmd:import", group: "Actions", title: "Import hosts from ~/.ssh/config", run: doImport },
      { id: "cmd:tunnels", group: "Actions", title: "Toggle tunnels panel", run: () => activeTab && (activeTab.ssh || activeTab.saved) && (activeTab.tunnels = !activeTab.tunnels) },
    ];
    const sess = paletteSessions.map((s): PaletteItem => ({
      id: `session:${s.id}`,
      group: "Sessions",
      title: s.name,
      subtitle: `${s.user}@${s.host}${s.port === 22 ? "" : `:${s.port}`}${s.tags.length ? `  ·  ${s.tags.join(", ")}` : ""}${s.env ? `  ·  ${s.env}` : ""}`,
      run: () => connectSaved(s),
    }));
    const snips = snippets.map((sn): PaletteItem => ({
      id: `snippet:${sn.id}`,
      group: "Snippets",
      title: sn.name,
      subtitle: sn.body.split("\n")[0].slice(0, 80),
      run: () => ((snippetPick = sn), (showSnippets = true)),
    }));
    const open = tabs.map((t): PaletteItem => ({
      id: `tab:${t.key}`,
      group: "Tabs",
      title: t.title,
      subtitle: t.key === activeKey ? "current tab" : t.ssh ? `${t.ssh.user}@${t.ssh.host}` : t.saved ? "saved session" : "local shell",
      run: () => (activeKey = t.key),
    }));
    return [...acts, ...extra, ...sess, ...snips, ...open];
  });
  function runPalette(item: PaletteItem) {
    showPalette = false;
    remember(item.id);
    // Let the palette unmount first so focus returns to the terminal, not to the closing dialog.
    queueMicrotask(() => {
      item.run();
      if (!showSnippets && !editing_session && !connecting) requestAnimationFrame(() => focusedPane(activeTab)?.focusTerminal());
    });
  }

  // ---- command marks ----
  const NO_MARKS =
    "No command marks in this pane yet. Local zsh and bash get them automatically (Settings > Terminal); for a remote shell, run \"Enable command marks in this remote shell\" from the palette.";
  function paneAction(fn: (p: TerminalPane) => unknown) {
    const p = focusedPane(activeTab);
    if (!p) return;
    if (!p.hasMarks()) return toast(NO_MARKS);
    return fn(p);
  }

  async function connectSsh(target: SshTarget, keep: { save: boolean; keepSecret: boolean }) {
    connecting = false;
    if (!keep.save) return newTab(target);
    const a = target.auth;
    const typed = a.method === "password" ? a.password : a.method === "key" ? (a.passphrase ?? "") : "";
    const input: SessionInput = {
      name: "",
      folder_id: null,
      host: target.host,
      port: target.port,
      user: target.user,
      auth: a.method,
      key_path: a.method === "key" ? a.path : null,
      color: null,
      env: null,
      tags: [],
      jump_id: null,
      tunnels: [],
    };
    const store = keep.keepSecret && typed !== "";
    try {
      const saved = await sessionSave(null, input, store ? { action: "set", value: typed } : { action: "unchanged" });
      await refresh();
      toast(`Saved ${saved.name} to sessions.`, "success");
      connectSaved(saved, !store && typed !== "" ? typed : null);
    } catch (e) {
      // Saving is a convenience: still open the connection that was asked for.
      say(`Could not save the session: ${e}`);
      newTab(target);
    }
  }

  /** Open the session editor prefilled from an open quick-connect tab. */
  function saveConnection(tab: Tab | undefined) {
    if (!tab?.ssh) return;
    dialogError = null;
    editing_session = { session: null, folderId: null, draft: tab.ssh };
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
    muted = muted.filter((m) => !gone.includes(m));
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
      case "mute_pane": return activeTab && toggleMute(activeTab.focus);
      case "zoom_pane": return activeTab && toggleZoom(activeTab);
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
      case "palette": return showPalette ? (showPalette = false) : openPalette();
      case "prev_prompt": return paneAction((p) => p.prevPrompt() || toast("No earlier prompt."));
      case "next_prompt": return paneAction((p) => p.nextPrompt());
      case "copy_last_output":
        return paneAction((p) => {
          const n = p.copyLastOutput();
          toast(n === null ? "No finished command yet." : n === 0 ? "The last command printed nothing." : `Copied ${n} line${n === 1 ? "" : "s"} of output.`, n ? "success" : "info");
        });
      case "select_last_output": return paneAction((p) => p.selectLastOutput() || toast("No output to select."));
      case "copy_last_command":
        return paneAction((p) => {
          const c = p.copyLastCommand();
          toast(c ? `Copied: ${c.length > 60 ? c.slice(0, 60) + "…" : c}` : "No finished command yet.", c ? "success" : "info");
        });
      case "rerun_last": return paneAction((p) => p.retypeLast() ?? toast("No command to type again."));
      case "select_block": return paneAction((p) => p.selectBlock() || toast("No output to select."));
      case "enable_integration": {
        const p = focusedPane(activeTab);
        if (!p || p.isEnded() || p.sessionId() === null) return toast("This pane is not connected.");
        if (!(activeTab?.ssh || activeTab?.saved)) return toast("Local shells get command marks automatically when the setting is on.");
        p.enableIntegration();
        return toast("Typed a one-line hook into this shell (bash or zsh). It defines functions for this session only and writes no files.");
      }
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
    settings.load().then(() => {
      if (!settings.value.check_for_updates) return;
      // Let the first tab connect before touching the network.
      setTimeout(async () => {
        await updates.check(true);
        if (updates.state.kind === "available") toast(`quay-term ${updates.state.version} is available. Install it from Settings > About.`, "info");
      }, 5000);
    });
    getAppInfo().then((i) => (info = i));
    // Tunnel indicator: ask each connected remote tab how many forwards run. Cheap, and quiet on failure.
    const pollTunnels = setInterval(async () => {
      for (const t of tabs) {
        if (!(t.ssh || t.saved)) continue;
        const sid = Object.values(t.panes).map((p) => p?.sessionId() ?? null).find((x) => x !== null) ?? null;
        if (sid === null) {
          t.tunnelCount = 0;
          continue;
        }
        try {
          t.tunnelCount = runningTunnels(await tunnelList(sid));
        } catch {
          /* the next tick tries again */
        }
      }
    }, 3000);
    newTab();
    refresh();
    snippetsLoad().then((s) => (snippets = s)).catch(say);
    loadVault().then(() => {
      if (vault?.mode === "locked" && !vault.is_new) showUnlock = true;
    });
    const unlisten = [
      onSessionExit(({ id, reason }) => {
        for (const t of tabs)
          for (const [pid, p] of Object.entries(t.panes))
            if (p?.sessionId() === id) {
              p.notifyExited(reason);
              if (reason || t.ssh || t.saved) notify(t, Number(pid), t.title, reason ? `Disconnected: ${reason}` : "Session ended");
            }
      }),
      onHostKeyPrompt((p) => (trustPrompt = p)),
      onAuthPrompt((p) => (authPrompt = p)),
      onAction((e) => showNotified(e.notification?.id)).then((l) => () => l.unregister()),
    ];
    return () => {
      clearInterval(pollTunnels);
      unlisten.forEach((u) => u.then((f) => f()));
    };
  });
</script>

<svelte:window onkeydowncapture={(e) => e.key === "Escape" && tabMenu && ((tabMenu = null), e.stopPropagation())} {onkeydown} />

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
  {#snippet tabItem(tab: Tab)}
      {@const st = stateOf(tab)}
      {@const tcolor = colorOf(tab)}
      {@const tenv = envOf(tab)}
      {@const host = tab.ssh?.host ?? savedOf(tab)?.host}
      <div
        class="tab"
        role="presentation"
        class:active={tab.key === activeKey}
        style:--tc={tcolor ?? "transparent"}
        oncontextmenu={(e) => (e.preventDefault(), (tabMenu = { x: e.clientX, y: e.clientY, key: tab.key }))}
      >
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
            {#if st !== "local" || tab.ssh || tab.saved}
              <span class="sshdot {st}" role="img" aria-label={stateLabel[st] || "Local shell"} title={stateLabel[st] || "Local shell"}></span>
            {:else if st === "local" && Object.values(tab.status).length && Object.values(tab.status).every((x) => x === "ended")}
              <span class="sshdot ended" role="img" aria-label="Exited" title="Exited"></span>
            {/if}
            <span class="ttl">{tab.title}</span>
            {#if vertical && host}<span class="sub">{host}</span>{/if}
            {#if tenv}<span class="envpill" class:prod={/^prod/i.test(tenv)} style:--c={tcolor ?? "var(--accent)"}>{tenv}</span>{/if}
            {#if tab.tunnelCount > 0}<span class="tunneldot" role="img" aria-label="{tab.tunnelCount} port forward{tab.tunnelCount === 1 ? '' : 's'} running" title="{tab.tunnelCount} port forward{tab.tunnelCount === 1 ? '' : 's'} running"><Icon name="swap" size={12} /></span>{/if}
            {#if tab.activity}<span class="activity" role="img" aria-label="New output" title="New output"></span>{/if}
          </button>
        {/if}
        <button class="close" aria-label="Close tab" onclick={() => closeTab(tab.key)}>×</button>
      </div>
  {/snippet}
  <nav class="tabs" data-tauri-drag-region>
    {#if !vertical}{#each tabs as tab (tab.key)}{@render tabItem(tab)}{/each}{/if}
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
  <div class="body">
  {#if vertical}
    <aside class="vtabs" aria-label="Tabs">{#each tabs as tab (tab.key)}{@render tabItem(tab)}{/each}</aside>
  {/if}
  <div class="panes">
    {#each tabs as tab (tab.key)}
      {@const lay = P.layout(tab.layout)}
      {@const solo = lay.panes.length === 1}
      {@const zoomed = solo ? null : tab.zoom}
      <div class="tabview" class:hidden={tab.key !== activeKey}>
        <div class="term">
          {#each lay.panes as p (p.id)}
            {@const v = P.zoomView(p.rect, p.id, zoomed)}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="leaf"
              class:focused={!solo && tab.focus === p.id}
              class:dragging={drag?.id === p.id}
              class:hidden={v.hidden}
              style:left="{v.rect.x * 100}%"
              style:top="{v.rect.y * 100}%"
              style:width="{v.rect.w * 100}%"
              style:height="{v.rect.h * 100}%"
              onpointerenter={(e) => {
                if (settings.value.focus_follows_mouse && e.pointerType === "mouse" && !drag && !v.hidden && tab.focus !== p.id && tab.key === activeKey) tab.focus = p.id;
              }}
            >
              {#if !solo && zoomed === null}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div class="grip" title="Drag onto another pane's edge to move this pane" onpointerdown={(e) => dragPane(e, tab, p.id)}></div>
              {/if}
              {#if zoomed === p.id || muted.includes(p.id)}
                <div class="leafbadges">
                  {#if zoomed === p.id}
                    <button class="zoombadge" title="Zoomed. Click or press {settings.bindings.zoom_pane} to restore" onclick={() => (tab.zoom = null)}>Zoomed</button>
                  {/if}
                  {#if muted.includes(p.id)}
                    <button class="zoombadge mutebadge" title="Alerts from this pane are off. Click or press {settings.bindings.mute_pane} to turn them on" onclick={() => toggleMute(p.id)}>Muted</button>
                  {/if}
                </div>
              {/if}
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
                env={envOf(tab)}
                color={colorOf(tab)}
                confirmPaste={/^prod/i.test(envOf(tab) ?? "")}
                onstatus={(s) => (tab.status[p.id] = s)}
                onactivity={() => (tab.activity = true)}
                oncommand={(c) => commandDone(tab, p.id, c)}
                onsnippet={addCommandSnippet}
                onfiles={tab.ssh || tab.saved ? () => (tab.files = true) : undefined}
                onconnected={tab.saved?.remember || tab.saved?.jumps?.some((j) => j.remember) ? refresh : undefined}
              />
            </div>
          {/each}
          {#if drag && drag.tab === tab.key && drag.over}
            {@const target = lay.panes.find((q) => q.id === drag!.over!.id)!}
            {@const r = P.edgeRect(target.rect, drag.over.edge)}
            <div class="dropmark" style:left="{r.x * 100}%" style:top="{r.y * 100}%" style:width="{r.w * 100}%" style:height="{r.h * 100}%"></div>
          {/if}
          {#each zoomed === null ? lay.dividers : [] as d (d.path)}
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
  </div>
  {#if tabMenu}
    {@const mt = tabs.find((t) => t.key === tabMenu!.key)}
    <div class="tabmenu-bg" role="presentation" onclick={() => (tabMenu = null)} oncontextmenu={(e) => (e.preventDefault(), (tabMenu = null))}></div>
    <div class="tabmenu" role="menu" style:left="{tabMenu.x}px" style:top="{tabMenu.y}px">
      {#if mt?.ssh && !mt.saved}
        <button role="menuitem" onclick={() => ((tabMenu = null), saveConnection(mt))}>Save this connection…</button>
      {/if}
      <button role="menuitem" onclick={() => ((editing = tabMenu!.key), (tabMenu = null))}>Rename tab</button>
      <button role="menuitem" onclick={() => (closeTab(tabMenu!.key), (tabMenu = null))}>Close tab</button>
    </div>
  {/if}
  {#if editing_session}
    {#key editing_session}
      <SessionDialog
        session={editing_session.session}
        defaultFolderId={editing_session.folderId}
        draft={editing_session.draft ?? null}
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
  {#if jumpFor}
    <PasswordPrompt
      title={`${jumpFor.hop.user}@${jumpFor.hop.host} (jump host ${jumpFor.hop.name})`}
      oncancel={() => (jumpFor = null)}
      onsubmit={(pw, remember) => {
        const j = jumpFor!;
        jumpFor = null;
        connectSaved(j.session, j.secret, j.remember, [...j.jumps, { id: j.hop.id, secret: pw, remember }]);
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
      initial={snippetPick}
      tabPanes={activeTab ? P.leaves(activeTab.layout).length : 0}
      broadcast={broadcast.length}
      onsend={sendSnippet}
      onsave={saveSnippet}
      ondelete={removeSnippet}
      onclose={() => ((showSnippets = false), (snippetPick = null))}
    />
  {/if}
  {#if showPalette}
    <Palette items={paletteItems} {recent} onrun={runPalette} onclose={() => ((showPalette = false), requestAnimationFrame(() => focusedPane(activeTab)?.focusTerminal()))} />
  {/if}
  {#if showUnlock && vault}
    <UnlockDialog isNew={vault.is_new} ondone={() => ((showUnlock = false), loadVault(), refresh())} oncancel={() => (showUnlock = false)} />
  {/if}
  {#if showSettings}
    <SettingsDialog
      onclose={() => (showSettings = false)}
      {vault}
      onunlock={() => ((showSettings = false), (showUnlock = true))}
      onimported={() => (refresh(), snippetsLoad().then((x) => (snippets = x)))}
    />
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
  .tabmenu-bg {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
  .tabmenu {
    position: fixed;
    z-index: 41;
    min-width: 180px;
    padding: 4px;
    background: var(--bg-alt);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
  }
  .tabmenu button {
    display: block;
    width: 100%;
    padding: 6px 10px;
    text-align: left;
    background: none;
    border: 0;
    border-radius: 5px;
    color: var(--fg);
    cursor: pointer;
  }
  .tabmenu button:hover {
    background: var(--hover);
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
    box-sizing: border-box;
    border-radius: 50%;
    background: var(--green);
  }
  .sshdot.connecting {
    background: var(--yellow);
    animation: pulse 1s ease-in-out infinite;
  }
  .sshdot.ended {
    background: transparent;
    border: 1.5px solid var(--red);
  }
  @keyframes pulse {
    50% {
      opacity: 0.3;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .sshdot.connecting {
      animation: none;
    }
  }
  .envpill {
    flex: none;
    padding: 0 5px;
    border-radius: 4px;
    background: var(--c);
    color: var(--on-accent, #fff);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .envpill.prod {
    background: var(--red);
    color: var(--bg);
  }
  .tunneldot {
    display: flex;
    flex: none;
    color: var(--accent);
  }
  .sub {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--dim);
    font-size: 11px;
  }
  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  /* Vertical tab layout: the same tabs, listed down the left side. */
  .vtabs {
    display: flex;
    flex: none;
    flex-direction: column;
    gap: 2px;
    box-sizing: border-box;
    width: 200px;
    padding: 6px;
    overflow-y: auto;
    background: var(--bg-alt);
    border-right: 1px solid var(--border);
    font-size: 14px;
  }
  .vtabs .tab,
  .vtabs .tab.active {
    flex: none;
    width: 100%;
    max-width: none;
    height: 36px;
    margin: 0;
    border-radius: 6px;
    box-shadow: inset 2px 0 0 var(--tc, transparent);
  }
  .vtabs .label {
    flex: 1;
    padding-left: 12px;
  }
  .vtabs .ttl {
    flex: 0 1 auto;
  }
  .vtabs .sub {
    flex: 1;
  }
  .activity {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }
  /* The session colour underlines its tab. */
  .tab {
    box-shadow: inset 0 -2px 0 var(--tc, transparent);
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
  /* Drawn above the divider lines, so the focused pane shows its ring on all four sides. */
  .leaf.focused::after {
    content: "";
    position: absolute;
    inset: 0;
    z-index: 3;
    box-sizing: border-box;
    border: 1px solid var(--accent);
    pointer-events: none;
  }
  /* Hidden by a zoom: same rectangle as before, so the terminal keeps its size and keeps receiving output. */
  .leaf.hidden {
    visibility: hidden;
    pointer-events: none;
  }
  .leaf.dragging {
    opacity: 0.6;
  }
  .grip {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 8px;
    z-index: 7;
    cursor: grab;
    touch-action: none;
    transition: background 0.12s;
  }
  .grip:hover {
    background: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .leaf.dragging .grip {
    cursor: grabbing;
  }
  .dropmark {
    position: absolute;
    z-index: 5;
    box-sizing: border-box;
    pointer-events: none;
    background: color-mix(in srgb, var(--accent) 28%, transparent);
    border: 2px solid var(--accent);
  }
  .leafbadges {
    position: absolute;
    top: 5px;
    left: 10px;
    z-index: 8;
    display: flex;
    gap: 6px;
  }
  .mutebadge {
    background: var(--dim);
  }
  .zoombadge {
    all: unset;
    padding: 1px 7px;
    border-radius: 4px;
    background: var(--accent);
    color: var(--on-accent, #fff);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    cursor: pointer;
  }
  .zoombadge:focus-visible {
    outline: 2px solid var(--fg);
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
  /* A thin line along every split, so the panes read as a grid. None on the outer edge of the tab. */
  .divider::before {
    content: "";
    position: absolute;
    background: var(--border);
    pointer-events: none;
  }
  .divider.row::before {
    left: 3px;
    top: 0;
    bottom: 0;
    width: 1px;
  }
  .divider.col::before {
    top: 3px;
    left: 0;
    right: 0;
    height: 1px;
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
