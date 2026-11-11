<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
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
    snippetDefaults,
    snippetDelete,
    vaultStatus,
    snippetSave,
    snippetsLoad,
    tunnelList,
    historyAdd,
    historyDelete,
    historyList,
  } from "./lib/ipc";
  import { compileDeny, recordable } from "./lib/history";
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
  import type { HistorySource, PaletteItem } from "./lib/Palette.svelte";
  import FilePanel from "./lib/FilePanel.svelte";
  import TunnelPanel from "./lib/TunnelPanel.svelte";
  import ConnectDialog from "./lib/ConnectDialog.svelte";
  import TrustDialog from "./lib/TrustDialog.svelte";
  import AuthPromptDialog from "./lib/AuthPromptDialog.svelte";
  import type { AuthPrompt } from "./lib/bindings/AuthPrompt";
  import Icon from "./lib/Icon.svelte";
  import SettingsDialog from "./lib/SettingsDialog.svelte";
  import Toasts from "./lib/Toasts.svelte";
  import { toasts, type ToastOptions } from "./lib/toastStore.svelte";
  import type { ToastKind } from "./lib/toasts";
  import SnippetDialog from "./lib/SnippetDialog.svelte";
  import UnlockDialog from "./lib/UnlockDialog.svelte";
  import Confirm from "./lib/Confirm.svelte";
  import Button from "./lib/Button.svelte";
  import Menu from "./lib/Menu.svelte";
  import type { VaultStatus } from "./lib/bindings/VaultStatus";
  import type { Target } from "./lib/SnippetDialog.svelte";
  import type { Snippet } from "./lib/bindings/Snippet";
  import { byShortName, fill, variables } from "./lib/snippets";
  import { settings } from "./lib/settings.svelte";
  import { actionFor, actionLabels, formatShortcut, isMac, type Action } from "./lib/keys";
  import * as P from "./lib/panes";
  import { inkOn } from "./lib/themes";
  import { setFocusFallback } from "./lib/modal";
  import { roving } from "./lib/roving";
  import { DOCK, dockFit, dragWidth, NARROW, readWidth, SIDEBAR, TOOLS_COMPACT, writeWidth } from "./lib/layout";
  import { runningTunnels, tabState, type TabState } from "./lib/tabs";
  import { pushRecent, unavailable, type Checked } from "./lib/available";
  import type { AppInfo } from "./lib/bindings/AppInfo";
  import type { HostKeyPrompt } from "./lib/bindings/HostKeyPrompt";
  import type { SshTarget } from "./lib/bindings/SshTarget";
  import type { JumpSecret } from "./lib/bindings/JumpSecret";
  import type { MissingSecret } from "./lib/bindings/MissingSecret";

  type Dock = "files" | "tunnels";
  type Saved = { id: number; label: string; secret?: string | null; remember?: boolean; jumps?: JumpSecret[] };
  // `layout` is the split tree; every leaf is a terminal that opens the tab's own target (a new login for SSH).
  type Tab = {
    key: number;
    title: string;
    ssh: SshTarget | null;
    saved: Saved | null;
    /** Which panel the dock on the right shows; null when it is closed. */
    dock: Dock | null;
    /** Panels opened since the dock was shown. They stay mounted while the other one is in front. */
    files?: boolean;
    tunnels?: boolean;
    layout: P.Node;
    /** Panes opened with "Open in split": they connect to their own saved session, not the tab's. */
    targets: Record<number, Saved>;
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
  /** The name being typed while a tab is renamed; the tab keeps its title until it is committed. */
  let renameDraft = $state("");
  function startRename(tab: Tab) {
    renameDraft = tab.title;
    editing = tab.key;
  }
  /** Enter or clicking away keeps the new name (an empty one keeps the old); Escape drops it. */
  function endRename(tab: Tab, keep: boolean) {
    if (editing !== tab.key) return;
    editing = null;
    const name = renameDraft.trim();
    if (keep && name) tab.title = name;
    requestAnimationFrame(() => focusedPane(activeTab)?.focusTerminal());
  }
  let connecting = $state(false);
  let trustPrompt = $state<HostKeyPrompt | null>(null);
  let authPrompt = $state<AuthPrompt | null>(null);
  let showSettings = $state(false);
  /** Settings page to open on; cleared when the dialog closes. */
  let settingsTab = $state<"terminal" | "history" | "about" | undefined>(undefined);
  function openSettings(tab?: typeof settingsTab) {
    settingsTab = tab;
    showSettings = true;
  }

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
  function sendSnippet(text: string, enter: boolean, target: Target) {
    const tab = activeTab;
    if (!tab) return;
    const ids =
      target === "focused" ? [tab.focus] : target === "tab" ? P.leaves(tab.layout) : broadcast;
    for (const id of ids) paneById(id)?.sendSnippet(text, enter);
    closeSnippets();
  }
  function closeSnippets() {
    showSnippets = false;
    snippetPick = null;
    snippetDraft = null;
    requestAnimationFrame(() => focusedPane(activeTab)?.focusTerminal());
  }
  async function rememberSnippet(id: number, session: number, values: Record<string, string>) {
    await snippetDefaults(id, session, values);
    snippets = await snippetsLoad();
  }
  /** `;name` typed at a prompt: a snippet with variables opens its form, one without is typed in (not run). */
  function expandSnippet(tab: Tab, pane: number, short: string): boolean {
    const sn = byShortName(snippets, short);
    if (!sn) return false;
    tab.focus = pane;
    if (variables(sn.body).length) {
      snippetPick = sn;
      showSnippets = true;
    } else requestAnimationFrame(() => paneById(pane)?.sendSnippet(fill(sn.body, {}), false));
    return true;
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
  /** "Add to snippets" from a command block: opens a new snippet with the command, to name it and mark its variables. */
  let snippetDraft = $state<{ name: string; body: string } | null>(null);
  function addCommandSnippet(command: string) {
    snippetPick = null;
    snippetDraft = { name: command.replace(/\s+/g, " ").slice(0, 40), body: command };
    showSnippets = true;
  }
  /** Deleting is immediate, with an Undo that puts the snippet back (and its remembered values). */
  async function removeSnippet(s: Snippet) {
    try {
      await snippetDelete(s.id);
      snippets = await snippetsLoad();
      toast(`Deleted snippet "${s.name}".`, "info", { action: { label: "Undo", run: () => restoreSnippet(s) } });
    } catch (e) {
      say(e);
    }
  }
  async function restoreSnippet(s: Snippet) {
    try {
      const back = await snippetSave(null, s.name, s.folder, s.body);
      for (const [session, values] of Object.entries(s.defaults)) await snippetDefaults(back.id, Number(session), values);
      snippets = await snippetsLoad();
    } catch (e) {
      say(`Could not restore the snippet: ${e}`);
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
  // ---- command history (F7) ----
  const deny = $derived(compileDeny(settings.value.history_deny_patterns));
  const hostOf = (tab: Tab, pane: number) => paneSaved(tab, pane)?.label ?? (tab.ssh ? `${tab.ssh.user}@${tab.ssh.host}` : "local");
  function recordCommand(tab: Tab, pane: number, c: { command: string; exit: number | null; spaced: boolean; cwd: string | null }) {
    if (!settings.value.record_history || c.spaced) return;
    const command = recordable(c.command, deny);
    if (!command) return;
    historyAdd({ command, host: hostOf(tab, pane), session_id: paneSaved(tab, pane)?.id ?? null, cwd: c.cwd, exit: c.exit }).catch((e) =>
      console.warn("history not saved", e),
    );
  }

  function commandDone(tab: Tab, pane: number, c: { command: string; exit: number | null; ms: number; spaced: boolean; cwd: string | null }) {
    recordCommand(tab, pane, c);
    if (!isLong(c.ms, settings.value.notify_after_secs)) return;
    const body = commandBody({ ...c, session: tab.title, showCommand: settings.value.notify_show_command });
    notify(tab, pane, c.exit === null || c.exit === 0 ? "Command finished" : "Command failed", body);
  }

  function newTab(ssh: SshTarget | null = null, saved: Saved | null = null) {
    const key = ++counter;
    const title = saved ? saved.label : ssh ? `${ssh.user}@${ssh.host}` : `Shell ${key}`;
    // A saved session comes back with the split layout it was last closed with.
    const layout = (saved && loadShape(saved.id)) || { leaf: ++paneCounter };
    tabs.push({ key, title, ssh, saved, layout, targets: {}, focus: P.leaves(layout)[0], zoom: null, dock: null, tunnelCount: 0, panes: {}, status: {}, activity: false });
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
    // Only a layout of the tab's own session is remembered: one with another session in it could not be reopened as it was.
    if (!tab.saved || Object.keys(tab.targets).length) return;
    try {
      if (P.isLeaf(layout)) localStorage.removeItem(shapeKey(tab.saved.id));
      else localStorage.setItem(shapeKey(tab.saved.id), JSON.stringify(P.shapeOf(layout)));
    } catch {
      /* layout memory is optional */
    }
  }
  const focusedPane = (tab: Tab | undefined) => tab?.panes[tab.focus];
  /** What a pane connects to: the session it was opened with ("Open in split"), else the tab's own. */
  const paneSaved = (tab: Tab, id: number) => tab.targets[id] ?? tab.saved;
  const paneSsh = (tab: Tab, id: number) => (tab.targets[id] ? null : tab.ssh);

  function toggleZoom(tab: Tab) {
    if (tab.zoom !== null) tab.zoom = null;
    else if (P.leaves(tab.layout).length > 1) tab.zoom = tab.focus;
  }
  function splitPane(tab: Tab, axis: P.Axis) {
    tab.zoom = null;
    const id = ++paneCounter;
    // A split of a pane that holds another session opens that session again, like any other split.
    if (tab.targets[tab.focus]) tab.targets[id] = tab.targets[tab.focus];
    setLayout(tab, P.split(tab.layout, tab.focus, id, axis));
    tab.focus = id;
  }
  /** "Open in split": a saved session as a new pane beside the focused one. */
  function splitWith(tab: Tab, target: Saved) {
    tab.zoom = null;
    const id = ++paneCounter;
    tab.targets[id] = target;
    setLayout(tab, P.split(tab.layout, tab.focus, id, "row"));
    tab.focus = id;
  }
  function closePane(tab: Tab, id: number) {
    const next = P.remove(tab.layout, id);
    if (!next) return closeTab(tab.key);
    const order = P.leaves(tab.layout);
    if (tab.zoom === id) tab.zoom = null;
    delete tab.targets[id];
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
  let sidebarOpen = $state(readSidebarPref());
  let editing_session = $state<{ session: SavedSession | null; folderId: number | null; draft?: SshTarget | null } | null>(null);
  /** Right-click menu on a tab. */
  let tabMenu = $state<{ x: number; y: number; key: number } | null>(null);
  let dialogError = $state<string | null>(null);
  type Into = "tab" | "split";
  let passwordFor = $state<{ session: SavedSession; into: Into } | null>(null);
  let jumpFor = $state<{
    into: Into;
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
  /** Width of the app's own area (the window's content). */
  let winW = $state(document.documentElement.clientWidth);
  const narrow = $derived(winW < NARROW);
  /** The sidebar was folded to the rail by a small window, not by the user. */
  let autoRail = false;
  function toggleSidebar() {
    autoRail = false;
    sidebarOpen = !sidebarOpen;
    // A choice made in a small window is for that window only.
    if (narrow) return;
    try {
      localStorage.setItem("quay.sidebar", sidebarOpen ? "open" : "closed");
    } catch {
      /* preference is optional */
    }
  }
  // A small window has no room for the full sidebar: start it as the rail, and bring it back when there is room.
  $effect(() => {
    const small = narrow;
    untrack(() => {
      if (small && sidebarOpen) {
        sidebarOpen = false;
        autoRail = true;
      } else if (!small && autoRail) {
        sidebarOpen = true;
        autoRail = false;
      }
    });
  });

  // ---- widths the user can drag ----
  let sidebarW = $state(readWidth("quay.sidebar.w", SIDEBAR));
  let dockW = $state(readWidth("quay.dock.w", DOCK));
  let resizing = $state(false);
  /** Width of the area the terminal and the dock share. */
  let panesW = $state(0);
  function dragSidebar(e: PointerEvent) {
    resizing = true;
    dragWidth(e, sidebarW, 1, SIDEBAR, (w) => (sidebarW = w), (w) => ((resizing = false), writeWidth("quay.sidebar.w", w)));
  }
  function dragDock(e: PointerEvent, shown: number) {
    resizing = true;
    dragWidth(e, shown, -1, DOCK, (w) => (dockW = w), (w) => ((resizing = false), writeWidth("quay.dock.w", w)));
  }
  const resetSidebarW = () => writeWidth("quay.sidebar.w", (sidebarW = SIDEBAR.def));
  const resetDockW = () => writeWidth("quay.dock.w", (dockW = DOCK.def));

  // ---- the dock on the right: Files and Tunnels share one place ----
  const canDock = (tab: Tab) => !!(tab.ssh || tab.saved);
  function showDock(tab: Tab, which: Dock) {
    if (!canDock(tab)) return;
    tab[which] = true;
    tab.dock = which;
  }
  function closeDock(tab: Tab) {
    tab.dock = null;
    tab.files = tab.tunnels = false;
    requestAnimationFrame(() => focusedPane(tab)?.focusTerminal());
  }
  const toggleDock = (tab: Tab, which: Dock) => (tab.dock === which ? closeDock(tab) : showDock(tab, which));

  // ---- the tab strip: tabs keep a readable width and the strip scrolls ----
  let navW = $state(0);
  let stripEl = $state<HTMLDivElement>();
  let stripOverflows = $state(false);
  /** The list of all tabs, opened from the end of the strip when some are scrolled out of view. */
  let tabList = $state<{ right: number; top: number } | null>(null);
  const checkStrip = () => (stripOverflows = !!stripEl && stripEl.scrollWidth > stripEl.clientWidth + 1);
  $effect(() => {
    if (!stripEl) return;
    // Its own size, and anything that changes what is inside it (a tab added, closed or renamed).
    // A new width (the all-tabs button appeared, the window changed) can push the front tab out of view.
    const ro = new ResizeObserver(() => (checkStrip(), revealActive()));
    const mo = new MutationObserver(() => (checkStrip(), revealActive()));
    ro.observe(stripEl);
    mo.observe(stripEl, { childList: true, subtree: true, characterData: true });
    return () => (ro.disconnect(), mo.disconnect());
  });
  const revealActive = () => stripEl?.querySelector(".tab.active")?.scrollIntoView({ block: "nearest", inline: "nearest" });
  // The tab you switch to is scrolled into view, and stays in view when the strip changes width.
  $effect(() => {
    void [activeKey, tabs.length, navW];
    // Once now, and once after the next frame: a tab that was just added has its final place only then.
    tick().then(() => (revealActive(), requestAnimationFrame(revealActive)));
  });
  const toast = (text: string, kind: ToastKind = "info", opts: ToastOptions = {}) => toasts.show(text, kind, opts);
  /** An error: one plain sentence that stays until dismissed, the raw text behind "Details". */
  const say = (e: unknown) => toasts.error(e);
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
    into: Into = "tab",
  ) {
    if (s.auth === "password" && !s.has_secret && secret === null) {
      passwordFor = { session: s, into };
      return;
    }
    const missing = await jumpSecretsMissing(s.id).catch(() => [] as MissingSecret[]);
    const hop = missing.find((m) => !jumps.some((j) => j.id === m.id));
    if (hop) {
      jumpFor = { into, session: s, secret, remember, jumps, hop };
      return;
    }
    rememberSession(s.id);
    const target: Saved = { id: s.id, label: s.name, secret, remember, jumps };
    if (into === "split" && activeTab) splitWith(activeTab, target);
    else newTab(null, target);
  }

  // ---- sessions connected to lately, offered in the new-tab menu ----
  const RECENT_SESSIONS = "quay.sessions.recent";
  function readRecentSessions(): number[] {
    try {
      const v = JSON.parse(localStorage.getItem(RECENT_SESSIONS) ?? "[]");
      return Array.isArray(v) ? v.filter((x) => Number.isInteger(x)).slice(0, 6) : [];
    } catch {
      return [];
    }
  }
  let recentIds = $state<number[]>(readRecentSessions());
  function rememberSession(id: number) {
    recentIds = pushRecent(recentIds, id, 6);
    try {
      localStorage.setItem(RECENT_SESSIONS, JSON.stringify(recentIds));
    } catch {
      /* recents are optional */
    }
  }
  /** The menu beside the new-tab button. */
  let newMenu = $state<{ left: number; top: number } | null>(null);
  let newMenuSessions = $state<SavedSession[]>([]);
  function openNewMenu(e: MouseEvent) {
    if (newMenu) return void (newMenu = null);
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    newMenu = { left: r.left, top: r.bottom + 4 };
    const pick = (all: SavedSession[]) =>
      recentIds.map((id) => all.find((x) => x.id === id)).filter((x): x is SavedSession => !!x && !goneSessions.includes(x.id));
    newMenuSessions = pick(library.sessions);
    // The sidebar search may have narrowed `library`; recents come from the whole list.
    if (query.trim()) libraryLoad().then((l) => newMenu && (newMenuSessions = pick(l.sessions))).catch(() => {});
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
  // Deleting a session or folder hides it at once and shows Undo. The delete itself runs when
  // the toast goes away, because a deleted session's saved password cannot be read back.
  let goneSessions = $state<number[]>([]);
  let goneFolders = $state<number[]>([]);
  /** The library without what is waiting to be deleted; a deleted folder's contents move up, as they will. */
  const shown = $derived.by((): LibraryView => {
    if (!goneSessions.length && !goneFolders.length) return library;
    const lift = (id: number | null): number | null => {
      while (id !== null && goneFolders.includes(id)) id = library.folders.find((f) => f.id === id)?.parent_id ?? null;
      return id;
    };
    return {
      folders: library.folders.filter((f) => !goneFolders.includes(f.id)).map((f) => ({ ...f, parent_id: lift(f.parent_id) })),
      sessions: library.sessions.filter((x) => !goneSessions.includes(x.id)).map((x) => ({ ...x, folder_id: lift(x.folder_id) })),
    };
  });
  function removeSession(s: SavedSession) {
    if (goneSessions.includes(s.id)) return;
    goneSessions.push(s.id);
    toast(`Deleted "${s.name}"${s.has_secret ? " and its saved password" : ""}.`, "info", {
      action: { label: "Undo", run: () => {} },
      onclose: async (undone) => {
        if (!undone) await sessionDelete(s.id).then(refresh).catch(say);
        goneSessions = goneSessions.filter((id) => id !== s.id);
      },
    });
  }
  function removeFolder(id: number) {
    const f = library.folders.find((x) => x.id === id);
    if (!f || goneFolders.includes(id)) return;
    goneFolders.push(id);
    toast(`Deleted folder "${f.name}". Its sessions and subfolders are kept.`, "info", {
      action: { label: "Undo", run: () => {} },
      onclose: async (undone) => {
        if (!undone) await folderDelete(id).then(refresh).catch(say);
        goneFolders = goneFolders.filter((x) => x !== id);
      },
    });
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
  /** A copy of a saved session, beside the original. Its saved password stays with the original: it cannot be read back. */
  async function duplicateSession(s: SavedSession) {
    const { id: _id, has_secret: _has, ...rest } = s;
    try {
      const made = await sessionSave(null, { ...rest, name: `${s.name} copy` }, { action: "unchanged" });
      await refresh();
      toast(`Duplicated as "${made.name}".${s.has_secret ? " Its saved password was not copied." : ""}`, "success", {
        action: { label: "Edit", run: () => ((dialogError = null), (editing_session = { session: made, folderId: null })) },
      });
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

  // Vertical tabs are a section of the sidebar. With the sidebar folded to the rail they go back to the strip.
  const vertical = $derived(settings.value.tab_layout === "vertical" && sidebarOpen);
  const activeTab = $derived(tabs.find((t) => t.key === activeKey));
  // A tab you switch to has nothing unseen any more.
  $effect(() => {
    if (activeTab) activeTab.activity = false;
  });

  // ---- session colour, environment and connection state ----
  const savedOf = (tab: Tab) => (tab.saved ? library.sessions.find((x) => x.id === tab.saved!.id) : undefined);
  /** The saved session a pane is connected to (its own, for a pane opened in a split). */
  const sessionOf = (tab: Tab, pane: number) => library.sessions.find((x) => x.id === paneSaved(tab, pane)?.id);
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
  let paletteInitial = $state("");
  function openPalette(initial = "") {
    paletteInitial = initial;
    showPalette = true;
    paletteSessions = library.sessions;
    // The sidebar search may have narrowed `library`; the palette searches everything.
    libraryLoad().then((l) => (paletteSessions = l.sessions)).catch(() => {});
  }
  /** Why an action cannot run in the front tab (one sentence), or null when it can. */
  const whyNot = (a: Checked) =>
    unavailable(a, { remote: !!activeTab && canDock(activeTab), panes: activeTab ? P.leaves(activeTab.layout).length : 0, tabs: tabs.length });
  /** A binding as shown in tooltips and the palette. */
  const keyOf = (a: Action) => formatShortcut(settings.bindings[a]);
  /** A tooltip with the action's shortcut after it, when it has one. */
  const tip = (text: string, a: Action) => (settings.bindings[a] ? `${text} (${keyOf(a)})` : text);
  const paletteItems = $derived.by((): PaletteItem[] => {
    const acts = (Object.keys(actionLabels) as Action[])
      .filter((a) => a !== "palette")
      .map((a): PaletteItem => ({ id: `action:${a}`, group: "Actions", title: actionLabels[a], hint: keyOf(a), disabled: whyNot(a) ?? undefined, run: () => runAction(a) }));
    const extra: PaletteItem[] = [
      { id: "cmd:ssh", group: "Actions", title: "New SSH connection…", run: () => (connecting = true) },
      { id: "cmd:session", group: "Actions", title: "New saved session…", run: () => ((dialogError = null), (editing_session = { session: null, folderId: null })) },
      ...(activeTab?.ssh && !activeTab.saved
        ? [{ id: "cmd:save-connection", group: "Actions" as const, title: "Save this connection…", run: () => saveConnection(activeTab) }]
        : []),
      { id: "cmd:import", group: "Actions", title: "Import hosts from ~/.ssh/config", run: doImport },
      { id: "cmd:tunnels", group: "Actions", title: "Toggle tunnels panel", disabled: whyNot("toggle_tunnels") ?? undefined, run: () => activeTab && toggleDock(activeTab, "tunnels") },
    ];
    const sess = paletteSessions.filter((s) => !goneSessions.includes(s.id)).map((s): PaletteItem => ({
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
  const historySource: HistorySource = {
    get recording() {
      return settings.value.record_history;
    },
    load: historyList,
    insert(command) {
      showPalette = false;
      queueMicrotask(() => {
        const p = focusedPane(activeTab);
        if (!p || p.isEnded()) return toast("This pane has no running shell to type into.");
        p.sendSnippet(command, false);
        requestAnimationFrame(() => p.focusTerminal());
      });
    },
    remove: historyDelete,
    onsettings() {
      showPalette = false;
      openSettings("history");
    },
  };
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
  /** Says there are no marks, with the next step as a button: turn them on here (remote) or in Settings (local). */
  function noMarks() {
    const remote = !!(activeTab?.ssh || activeTab?.saved);
    toast("No command marks in this pane yet.", "info", {
      action: remote ? { label: "Enable here", run: () => runAction("enable_integration") } : { label: "Open Settings", run: () => openSettings("terminal") },
    });
  }
  function paneAction(fn: (p: TerminalPane) => unknown) {
    const p = focusedPane(activeTab);
    if (!p) return;
    if (!p.hasMarks()) return noMarks();
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

  // ---- closing: ask first when it would end a running command or several connected panes ----
  let closeAsk = $state<{ title: string; body: string; label: string; run: () => void } | null>(null);
  const busyIn = (tab: Tab, ids: number[]) => ids.filter((id) => tab.panes[id]?.isBusy()).length;
  function confirmClose(ask: NonNullable<typeof closeAsk>) {
    if (settings.value.confirm_close) closeAsk = ask;
    else ask.run();
  }
  function answerClose(yes: boolean, never = false) {
    const ask = closeAsk;
    closeAsk = null;
    if (yes && never) settings.update({ ...settings.value, confirm_close: false });
    if (yes) ask?.run();
    else requestAnimationFrame(() => focusedPane(activeTab)?.focusTerminal());
  }
  function requestCloseTab(key: number) {
    const tab = tabs.find((t) => t.key === key);
    if (!tab) return;
    const ids = P.leaves(tab.layout);
    const busy = busyIn(tab, ids);
    const live = ids.filter((id) => tab.status[id] === "connected").length;
    if (!busy && live < 2) return closeTab(key);
    const body = busy
      ? `${busy === 1 ? "A command is" : `Commands are`} still running in ${busy === 1 && ids.length === 1 ? "this tab" : busy === 1 ? "one of its panes" : `${busy} of its panes`}. Closing the tab stops ${busy === 1 ? "it" : "them"}.`
      : `This closes all ${live} connected panes in the tab.`;
    confirmClose({ title: `Close ${tab.title}?`, body, label: ids.length > 1 ? `Close ${ids.length} panes` : "Close tab", run: () => closeTab(key) });
  }
  /** Close several tabs (the tab menu's "Close others"), asking once if any of them would ask on its own. */
  function requestCloseTabs(keys: number[]) {
    const list = tabs.filter((t) => keys.includes(t.key));
    if (list.length < 2) return list[0] && requestCloseTab(list[0].key);
    const risky = list.filter((t) => {
      const ids = P.leaves(t.layout);
      return busyIn(t, ids) > 0 || ids.filter((id) => t.status[id] === "connected").length > 1;
    });
    const run = () => list.forEach((t) => closeTab(t.key));
    if (!risky.length) return run();
    const who = risky.length === 1 ? `"${risky[0].title}" has` : `${risky.length} of them have`;
    confirmClose({ title: `Close ${list.length} tabs?`, body: `${who} a running command or several connected panes. Closing stops them.`, label: `Close ${list.length} tabs`, run });
  }
  /** The same thing again in a new tab: the same session or host, or another local shell. */
  const duplicateTab = (tab: Tab) => newTab(tab.ssh, tab.saved);
  const canReconnect = (tab: Tab) => Object.values(tab.status).some((x) => x === "ended");
  /** Start every ended pane of the tab again. */
  function reconnectTab(tab: Tab) {
    for (const p of Object.values(tab.panes)) p?.reconnect();
    activeKey = tab.key;
  }
  function requestClosePane(tab: Tab, id: number) {
    // The last pane takes the tab with it.
    if (P.leaves(tab.layout).length < 2) return requestCloseTab(tab.key);
    if (!busyIn(tab, [id])) return closePane(tab, id);
    confirmClose({ title: "Close this pane?", body: "A command is still running in it. Closing the pane stops it.", label: "Close pane", run: () => closePane(tab, id) });
  }

  function runAction(a: Action) {
    // A shortcut that cannot apply here says why, instead of doing nothing.
    const why = whyNot(a);
    if (why) return toast(why);
    const i = tabs.findIndex((t) => t.key === activeKey);
    switch (a) {
      case "new_tab": return newTab();
      case "close_tab": return activeTab && requestClosePane(activeTab, activeTab.focus);
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
        return activeTab && toggleDock(activeTab, "files");
      case "settings": return (showSettings = !showSettings);
      case "zoom_in": return zoom(1);
      case "zoom_out": return zoom(-1);
      case "zoom_reset": return settings.update({ ...settings.value, font_size: 14 });
      case "palette": return showPalette ? (showPalette = false) : openPalette();
      case "history_search": return showPalette ? (showPalette = false) : openPalette("$ ");
      case "find": return focusedPane(activeTab)?.openFind();
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
        if (!(activeTab?.ssh || activeTab?.saved)) return toast("Local shells get command marks from a setting.", "info", { action: { label: "Open Settings", run: () => openSettings("terminal") } });
        p.enableIntegration();
        return toast("Typed a one-line hook into this shell. It lasts for this session and writes no files.");
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
    setFocusFallback(() => focusedPane(activeTab)?.focusTerminal());
    settings.load().then(() => {
      if (!settings.value.check_for_updates) return;
      // Let the first tab connect before touching the network.
      setTimeout(async () => {
        await updates.check(true);
        if (updates.state.kind === "available") toast(`Quay Term ${updates.state.version} is available.`, "info", { sticky: true, action: { label: "Open Settings", run: () => openSettings("about") } });
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

<svelte:window
  onpagehide={() => toasts.flush()}
  {onkeydown}
/>

<!-- One tab, drawn the same in the strip and in the sidebar's list. -->
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
            aria-label="Tab name"
            bind:value={renameDraft}
            onfocus={(e) => e.currentTarget.select()}
            onblur={() => endRename(tab, true)}
            onkeydown={(e) => (e.key === "Enter" ? endRename(tab, true) : e.key === "Escape" && (e.stopPropagation(), endRename(tab, false)))}
          />
        {:else}
          <button
            class="label"
            role="tab"
            id="tab-{tab.key}"
            aria-selected={tab.key === activeKey}
            aria-controls="tabview-{tab.key}"
            tabindex={tab.key === activeKey ? 0 : -1}
            onclick={() => (activeKey = tab.key)}
            ondblclick={() => startRename(tab)}
            onkeydown={(e) => {
              if (e.key === "F2") startRename(tab);
              else if (e.key === "Delete") requestCloseTab(tab.key);
              else if (e.key === "Enter") (e.preventDefault(), focusedPane(tab)?.focusTerminal());
            }}
          >
            {#if st !== "local" || tab.ssh || tab.saved}
              <span class="sshdot {st}" role="img" aria-label={stateLabel[st] || "Local shell"} title={stateLabel[st] || "Local shell"}></span>
            {:else if st === "local" && Object.values(tab.status).length && Object.values(tab.status).every((x) => x === "ended")}
              <span class="sshdot ended" role="img" aria-label="Exited" title="Exited"></span>
            {/if}
            <span class="ttl">{tab.title}</span>
            {#if vertical && host}<span class="sub">{host}</span>{/if}
            {#if tenv}<span class="envpill" class:prod={/^prod/i.test(tenv)} style:--c={tcolor ?? "var(--accent)"} style:--on={inkOn(tcolor)}>{tenv}</span>{/if}
            {#if tab.tunnelCount > 0}<span class="tunneldot" role="img" aria-label="{tab.tunnelCount} port forward{tab.tunnelCount === 1 ? '' : 's'} running" title="{tab.tunnelCount} port forward{tab.tunnelCount === 1 ? '' : 's'} running"><Icon name="swap" size={12} /></span>{/if}
            {#if tab.activity}<span class="activity" role="img" aria-label="New output" title="New output"></span>{/if}
          </button>
        {/if}
        <button class="close" aria-label="Close {tab.title}" title="Close tab" tabindex={tab.key === activeKey ? 0 : -1} onclick={() => requestCloseTab(tab.key)}><Icon name="x" size={13} /></button>
      </div>
  {/snippet}
  {#snippet sideTabs()}
    <div class="vtabs" role="tablist" aria-label="Tabs" aria-orientation="vertical" use:roving={{ selector: '[role="tab"]', vertical: true, activate: true }}>{#each tabs as tab (tab.key)}{@render tabItem(tab)}{/each}</div>
  {/snippet}

<main class:mac={isMac} class:rail={!sidebarOpen} class:resizing style:--sidebar-w="{sidebarW}px" bind:clientWidth={winW}>
  <Sidebar
    railOnly={!sidebarOpen}
    library={shown}
    bind:query
    oncollapse={toggleSidebar}
    toggleTitle={tip(`${sidebarOpen ? "Hide" : "Show"} sidebar`, "toggle_sidebar")}
    onshell={() => newTab()}
    onssh={() => (connecting = true)}
    shellKey={keyOf("new_tab")}
    paletteKey={keyOf("palette")}
    onimport={doImport}
    openIds={tabs.flatMap((t) => [...(t.saved ? [t.saved.id] : []), ...Object.values(t.targets).map((x) => x.id)])}
    activeId={activeTab?.saved?.id ?? null}
    version={info ? `Quay Term v${info.version}` : ""}
    onconnect={(s) => connectSaved(s)}
    onsplit={(s) => connectSaved(s, null, false, [], "split")}
    onduplicate={duplicateSession}
    onedit={(s) => ((dialogError = null), (editing_session = { session: s, folderId: null }))}
    onnew={(folderId) => ((dialogError = null), (editing_session = { session: null, folderId }))}
    ondelete={removeSession}
    onnewfolder={(name, parentId) => folderAction(() => folderCreate(name, parentId))()}
    onrenamefolder={(id, name) => folderAction(() => folderRename(id, name))()}
    ondeletefolder={removeFolder}
    onmovesession={(id, folderId) => folderAction(() => sessionMove(id, folderId))()}
    onmovefolder={(id, parentId) => folderAction(() => folderMove(id, parentId))()}
    tabs={vertical ? sideTabs : undefined}
  />
  {#if sidebarOpen}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="sbgrip" title="Drag to resize the sidebar. Double-click to reset." onpointerdown={dragSidebar} ondblclick={resetSidebarW}></div>
  {/if}
  <div class="col">
  <nav class="tabs" class:compact={navW < TOOLS_COMPACT} bind:clientWidth={navW} data-tauri-drag-region>
    <!-- Tabs keep a readable width; when they do not all fit, the strip scrolls and the list button shows them all. -->
    <div
      class="strip"
      role={vertical ? undefined : "tablist"}
      aria-label={vertical ? undefined : "Tabs"}
      use:roving={{ selector: '[role="tab"]', activate: true }}
      bind:this={stripEl}
      data-tauri-drag-region
      onwheel={(e) => Math.abs(e.deltaY) > Math.abs(e.deltaX) && (e.currentTarget.scrollLeft += e.deltaY)}
    >
      {#if !vertical}{#each tabs as tab (tab.key)}{@render tabItem(tab)}{/each}{/if}
    </div>
    {#if !vertical && stripOverflows}
      <Button
        variant="icon"
        title="All tabs"
        aria-label="All tabs ({tabs.length})"
        aria-haspopup="menu"
        aria-expanded={!!tabList}
        onclick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          tabList = tabList ? null : { right: Math.max(8, window.innerWidth - r.right), top: r.bottom + 4 };
        }}
      >
        <Icon name="list" size={16} />
      </Button>
    {/if}
    <div class="newtab" role="group" aria-label="New tab">
      <Button variant="icon" title={tip("New local shell", "new_tab")} aria-label="New local shell" onclick={() => newTab()}><Icon name="plus" size={16} /></Button>
      <Button variant="icon" class="more" title="New SSH connection, or a recent session" aria-label="More ways to open a tab" aria-haspopup="menu" aria-expanded={!!newMenu} onclick={openNewMenu}>
        <Icon name="chevron-down" size={12} />
      </Button>
    </div>
    <!-- The empty part of the strip moves the window, like a title bar. -->
    <div class="fill" data-tauri-drag-region></div>
    <div class="right">
      <Button variant="ghost" title={tip("Command palette: search actions, sessions, snippets and tabs", "palette")} aria-label="Command palette" onclick={() => openPalette()}>
        <Icon name="command" size={16} />{#if keyOf("palette")}<span class="txt">{keyOf("palette")}</span>{/if}
      </Button>
      {#if activeTab && canDock(activeTab)}
        <Button variant="ghost" active={activeTab.dock === "files"} aria-label="Files" aria-pressed={activeTab.dock === "files"} title="Browse files on this host (SFTP)" onclick={() => toggleDock(activeTab, "files")}>
          <Icon name="folder" size={16} /><span class="txt">Files</span>
        </Button>
        <Button variant="ghost" active={activeTab.dock === "tunnels"} aria-label="Tunnels" aria-pressed={activeTab.dock === "tunnels"} title="Port forwards over this connection" onclick={() => toggleDock(activeTab, "tunnels")}>
          <Icon name="swap" size={16} /><span class="txt">Tunnels</span>
        </Button>
      {/if}
      {#if broadcast.length}
        <Button variant="ghost" class="warn" title="Stop broadcasting to {broadcast.length} panes" aria-label="Stop broadcasting to {broadcast.length} panes" onclick={() => (broadcast = [])}>
          <Icon name="stop" size={14} /><span class="txt">Broadcast</span> {broadcast.length}
        </Button>
      {/if}
      <Button variant="ghost" title={tip("Snippets", "snippets")} aria-label="Snippets" onclick={() => (showSnippets = true)}>
        <Icon name="terminal" size={16} /><span class="txt">Snippets</span>
      </Button>
      <Button variant="icon" title="Settings" aria-label="Settings" onclick={() => (showSettings = true)}><Icon name="settings" size={18} /></Button>
    </div>
  </nav>
  <div class="body">
  <div class="panes" bind:clientWidth={panesW}>
    {#each tabs as tab (tab.key)}
      {@const lay = P.layout(tab.layout)}
      {@const solo = lay.panes.length === 1}
      {@const zoomed = solo ? null : tab.zoom}
      <div class="tabview" class:hidden={tab.key !== activeKey} role="tabpanel" id="tabview-{tab.key}" aria-labelledby="tab-{tab.key}">
        <div class="term">
          {#each lay.panes as p (p.id)}
            {@const v = P.zoomView(p.rect, p.id, zoomed)}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="leaf"
              class:focused={!solo && tab.focus === p.id}
              class:dim={!solo && tab.focus !== p.id && settings.value.dim_unfocused_panes}
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
              {#if !solo}
                <!-- A slim handle along the top edge. Under the pointer (or with keyboard focus) it opens
                     into a header: the pane's name to drag it by, and its broadcast, zoom and close buttons. -->
                <div class="phead" role="group" aria-label="Pane {paneSaved(tab, p.id)?.label ?? tab.title}">
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <div class="pdrag" title={zoomed === null ? "Drag onto another pane's edge to move this pane" : undefined} onpointerdown={(e) => dragPane(e, tab, p.id)}>
                    <span class="handle"></span>
                    <span class="ptitle">{paneSaved(tab, p.id)?.label ?? tab.title}</span>
                  </div>
                  <button
                    class:on={broadcast.includes(p.id)}
                    aria-pressed={broadcast.includes(p.id)}
                    aria-label="Broadcast: typing in a broadcast pane goes to this one too"
                    title={tip(broadcast.includes(p.id) ? "Stop sending other panes' typing here" : "Broadcast: send typing to this pane too", "broadcast_pane")}
                    onclick={() => toggleBroadcast([p.id])}><Icon name="radio" size={13} /></button>
                  <button
                    class:on={zoomed === p.id}
                    aria-pressed={zoomed === p.id}
                    aria-label="Zoom this pane to fill the tab"
                    title={tip(zoomed === p.id ? "Restore the other panes" : "Zoom to fill the tab", "zoom_pane")}
                    onclick={() => ((tab.focus = p.id), toggleZoom(tab), focusedPane(tab)?.focusTerminal())}><Icon name="maximize" size={13} /></button>
                  <button aria-label="Close this pane" title={tip("Close pane", "close_tab")} onclick={() => requestClosePane(tab, p.id)}><Icon name="x" size={13} /></button>
                </div>
              {/if}
              {#if zoomed === p.id || muted.includes(p.id)}
                <div class="leafbadges">
                  {#if zoomed === p.id}
                    <button class="zoombadge" title={tip("Zoomed. Click to restore", "zoom_pane")} onclick={() => (tab.zoom = null)}>Zoomed</button>
                  {/if}
                  {#if muted.includes(p.id)}
                    <button class="zoombadge mutebadge" title={tip("Alerts from this pane are off. Click to turn them on", "mute_pane")} onclick={() => toggleMute(p.id)}>Muted</button>
                  {/if}
                </div>
              {/if}
              <TerminalPane
                bind:this={tab.panes[p.id]}
                active={tab.key === activeKey}
                focused={tab.focus === p.id}
                compact={!solo}
                badgeRow={zoomed === p.id || muted.includes(p.id)}
                onfocus={() => (tab.focus = p.id)}
                oninput={(d) => mirror(p.id, d)}
                broadcasting={broadcast.includes(p.id)}
                ssh={paneSsh(tab, p.id)}
                saved={paneSaved(tab, p.id)}
                env={sessionOf(tab, p.id)?.env ?? null}
                color={sessionOf(tab, p.id)?.color ?? null}
                confirmPaste={/^prod/i.test(sessionOf(tab, p.id)?.env ?? "")}
                onstatus={(s) => (tab.status[p.id] = s)}
                onactivity={() => (tab.activity = true)}
                oncommand={(c) => commandDone(tab, p.id, c)}
                onsnippet={addCommandSnippet}
                onexpand={(short) => expandSnippet(tab, p.id, short)}
                onfiles={canDock(tab) ? () => showDock(tab, "files") : undefined}
                onconnected={paneSaved(tab, p.id)?.remember || paneSaved(tab, p.id)?.jumps?.some((j) => j.remember) ? refresh : undefined}
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
        {#if tab.dock}
          {@const fit = dockFit(dockW, panesW)}
          <!-- One panel on the right for Files and Tunnels. It sits beside the terminal, or over it when the window is too small for both. -->
          <aside class="dock" class:over={fit.over} style:width="{fit.width}px" aria-label="Side panel">
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="dockgrip" title="Drag to resize. Double-click to reset." onpointerdown={(e) => dragDock(e, fit.width)} ondblclick={resetDockW}></div>
            <header class="dockhead">
              <div class="docktabs" role="tablist" aria-label="Side panel">
                <button role="tab" aria-selected={tab.dock === "files"} class:on={tab.dock === "files"} onclick={() => showDock(tab, "files")}><Icon name="folder" size={15} />Files</button>
                <button role="tab" aria-selected={tab.dock === "tunnels"} class:on={tab.dock === "tunnels"} onclick={() => showDock(tab, "tunnels")}>
                  <Icon name="swap" size={15} />Tunnels{#if tab.tunnelCount > 0}<span class="count">{tab.tunnelCount}</span>{/if}
                </button>
              </div>
              <Button variant="icon" title="Close panel" aria-label="Close side panel" onclick={() => closeDock(tab)}><Icon name="x" size={16} /></Button>
            </header>
            {#if tab.files}
              <div class="dockbody" class:off={tab.dock !== "files"}>
                <FilePanel ssh={tab.ssh} saved={tab.saved} active={tab.key === activeKey && tab.dock === "files"} terminalId={() => focusedPane(tab)?.sessionId() ?? null} />
              </div>
            {/if}
            {#if tab.tunnels}
              <div class="dockbody" class:off={tab.dock !== "tunnels"}>
                <TunnelPanel
                  terminalId={() => focusedPane(tab)?.sessionId() ?? null}
                  saved={library.sessions.find((x) => x.id === tab.saved?.id)?.tunnels ?? []}
                  active={tab.key === activeKey && tab.dock === "tunnels"}
                />
              </div>
            {/if}
          </aside>
        {/if}
      </div>
    {/each}
  </div>
  </div>
  </div>
  {#if tabMenu}
    {@const mt = tabs.find((t) => t.key === tabMenu!.key)}
    <Menu label="Tab" left={tabMenu.x} top={tabMenu.y} onclose={() => (tabMenu = null)}>
      {#if mt?.ssh && !mt.saved}
        <button role="menuitem" onclick={() => (saveConnection(mt), (tabMenu = null))}>Save this connection…</button>
      {/if}
      {#if mt}
        {@const at = tabs.indexOf(mt)}
        <button role="menuitem" onclick={() => (duplicateTab(mt), (tabMenu = null))}>Duplicate tab</button>
        <button role="menuitem" disabled={!canReconnect(mt)} onclick={() => (reconnectTab(mt), (tabMenu = null))}>{canDock(mt) ? "Reconnect" : "Restart shell"}</button>
        <button role="menuitem" onclick={() => (startRename(mt), (tabMenu = null))}><span class="mttl">Rename tab</span><kbd>F2</kbd></button>
        <hr />
        <button role="menuitem" onclick={() => ((activeKey = mt.key), splitPane(mt, "row"), (tabMenu = null))}><span class="mttl">Split right</span><kbd>{keyOf("split_right")}</kbd></button>
        <button role="menuitem" onclick={() => ((activeKey = mt.key), splitPane(mt, "col"), (tabMenu = null))}><span class="mttl">Split down</span><kbd>{keyOf("split_down")}</kbd></button>
        <hr />
        <button role="menuitem" onclick={() => (requestCloseTab(mt.key), (tabMenu = null))}>Close tab</button>
        <button role="menuitem" disabled={tabs.length < 2} onclick={() => (requestCloseTabs(tabs.filter((t) => t !== mt).map((t) => t.key)), (tabMenu = null))}>Close other tabs</button>
        <button role="menuitem" disabled={at === tabs.length - 1} onclick={() => (requestCloseTabs(tabs.slice(at + 1).map((t) => t.key)), (tabMenu = null))}>Close tabs to the right</button>
      {/if}
    </Menu>
  {/if}
  {#if newMenu}
    <Menu label="New tab" left={newMenu.left} top={newMenu.top} onclose={() => (newMenu = null)}>
      <button role="menuitem" onclick={() => ((newMenu = null), newTab())}><Icon name="terminal" size={14} /><span class="mttl">Local shell</span><kbd>{keyOf("new_tab")}</kbd></button>
      <button role="menuitem" onclick={() => ((newMenu = null), (connecting = true))}><Icon name="server" size={14} /><span class="mttl">SSH connection…</span></button>
      {#if newMenuSessions.length}
        <hr />
        <div class="mhead">Recent sessions</div>
        {#each newMenuSessions as s (s.id)}
          <button role="menuitem" title={`${s.user}@${s.host}`} onclick={() => ((newMenu = null), connectSaved(s))}>
            <span class="mdot" style:background={s.color ?? "var(--accent)"}></span><span class="mttl">{s.name}</span>
          </button>
        {/each}
      {/if}
    </Menu>
  {/if}
  {#if tabList}
    <Menu label="All tabs" right={tabList.right} top={tabList.top} scroll onclose={() => (tabList = null)}>
      {#each tabs as t (t.key)}
        {@const st = stateOf(t)}
        <button role="menuitemradio" aria-checked={t.key === activeKey} onclick={() => ((activeKey = t.key), (tabList = null))}>
          <span class="sshdot {st}" class:none={st === "local"}></span>
          <span class="mttl">{t.title}</span>
          {#if t.activity}<span class="activity"></span>{/if}
        </button>
      {/each}
    </Menu>
  {/if}
  {#if editing_session}
    {#key editing_session}
      <SessionDialog
        session={editing_session.session}
        defaultFolderId={editing_session.folderId}
        draft={editing_session.draft ?? null}
        folders={shown.folders}
        sessions={shown.sessions}
        error={dialogError}
        {vault}
        onunlock={() => (showUnlock = true)}
        onsubmit={submitSession}
        oncancel={() => (editing_session = null)}
      />
    {/key}
  {/if}
  {#if passwordFor}
    <PasswordPrompt
      title={`${passwordFor.session.user}@${passwordFor.session.host}`}
      {vault}
      onunlock={() => (showUnlock = true)}
      oncancel={() => (passwordFor = null)}
      onsubmit={(pw, remember) => {
        const p = passwordFor!;
        passwordFor = null;
        connectSaved(p.session, pw, remember, [], p.into);
      }}
    />
  {/if}
  {#if jumpFor}
    <PasswordPrompt
      title={`${jumpFor.hop.user}@${jumpFor.hop.host} (jump host ${jumpFor.hop.name})`}
      {vault}
      onunlock={() => (showUnlock = true)}
      oncancel={() => (jumpFor = null)}
      onsubmit={(pw, remember) => {
        const j = jumpFor!;
        jumpFor = null;
        connectSaved(j.session, j.secret, j.remember, [...j.jumps, { id: j.hop.id, secret: pw, remember }], j.into);
      }}
    />
  {/if}
  {#if connecting}
    <ConnectDialog {vault} onunlock={() => (showUnlock = true)} onconnect={connectSsh} oncancel={() => (connecting = false)} />
  {/if}
  <Toasts />
  {#if showSnippets}
    <SnippetDialog
      {snippets}
      initial={snippetPick}
      draft={snippetDraft}
      session={activeTab?.saved ? { id: activeTab.saved.id, label: activeTab.saved.label } : null}
      tabPanes={activeTab ? P.leaves(activeTab.layout).length : 0}
      broadcast={broadcast.length}
      onsend={sendSnippet}
      onsave={saveSnippet}
      onremember={rememberSnippet}
      ondelete={removeSnippet}
      onclose={closeSnippets}
    />
  {/if}
  {#if showPalette}
    <Palette items={paletteItems} {recent} history={historySource} initial={paletteInitial} onrun={runPalette} onclose={() => ((showPalette = false), requestAnimationFrame(() => focusedPane(activeTab)?.focusTerminal()))} />
  {/if}
  {#if closeAsk}
    {#key closeAsk}
      <Confirm title={closeAsk.title} body={closeAsk.body} confirmLabel={closeAsk.label} dontAsk onconfirm={(never) => answerClose(true, never)} oncancel={() => answerClose(false)} />
    {/key}
  {/if}
  {#if showUnlock && vault}
    <UnlockDialog isNew={vault.is_new} ondone={() => ((showUnlock = false), loadVault(), refresh())} oncancel={() => (showUnlock = false)} />
  {/if}
  {#if showSettings}
    <SettingsDialog
      initialTab={settingsTab}
      onclose={() => ((showSettings = false), (settingsTab = undefined))}
      {vault}
      onunlock={() => ((showSettings = false), (showUnlock = true))}
      onimported={() => (refresh(), snippetsLoad().then((x) => (snippets = x)))}
      onsshimport={doImport}
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
     title bar, positioned in tauri.conf.json); `--lights` is the room they take. */
  main {
    --lights: 0px;
    position: relative;
    display: flex;
    height: 100%;
  }
  main.mac {
    --lights: 78px;
  }
  .col {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }
  /* The sidebar's right edge: drag to resize. Below the top band, which belongs to the window. */
  .sbgrip {
    position: absolute;
    z-index: 6;
    top: var(--chrome-h);
    bottom: 0;
    left: calc(var(--sidebar-w) - 4px);
    width: 7px;
    cursor: col-resize;
    touch-action: none;
  }
  .sbgrip:hover,
  main.resizing .sbgrip {
    background: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  /* While a width is dragged it follows the pointer, with no animation and no text selection. */
  main.resizing {
    user-select: none;
    cursor: col-resize;
  }
  main.resizing :global(.sb) {
    transition: none;
  }
  nav {
    display: flex;
    align-items: flex-end;
    gap: var(--space-1);
    flex: none;
    box-sizing: border-box;
    height: var(--chrome-h);
    padding: 0 var(--space-3);
    background: var(--bg-alt);
    border-bottom: 1px solid var(--border);
    font-size: var(--text-m);
    transition: padding-left 160ms ease-out;
  }
  /* The collapsed rail is narrower than the traffic lights, so the tabs start after them. */
  main.rail nav {
    padding-left: max(var(--space-3), calc(var(--lights) + var(--space-2) - var(--rail-w)));
  }
  @media (prefers-reduced-motion: reduce) {
    nav {
      transition: none;
    }
  }
  /* One pixel taller than the strip's content, so the active tab can cover the strip's bottom border. */
  .strip {
    display: flex;
    align-items: flex-end;
    gap: var(--space-1);
    /* As wide as its tabs, so the new-tab button sits right after the last one. */
    flex: 0 1 auto;
    min-width: 0;
    height: calc(100% + 1px);
    margin-bottom: -1px;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }
  .strip::-webkit-scrollbar {
    display: none;
  }
  .tab {
    display: flex;
    align-items: center;
    flex: 0 1 auto;
    /* Room for the state dot, a few letters and the close button. Below this the strip scrolls. */
    min-width: 96px;
    max-width: 220px;
    height: 34px;
    margin-bottom: 1px;
    border-radius: var(--radius-l) var(--radius-l) 0 0;
    color: var(--dim);
  }
  .tab:hover {
    background: var(--hover);
  }
  /* Covers the strip's bottom border so the active tab and the terminal read as one surface. */
  .tab.active {
    height: 35px;
    margin-bottom: 0;
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
    padding: 0 var(--space-1);
    border-radius: var(--radius-s);
    background: var(--c);
    color: var(--on);
    font-size: var(--text-xs);
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .envpill.prod {
    background: var(--red);
    color: var(--on-red);
  }
  .tunneldot {
    display: flex;
    flex: none;
    color: var(--accent-text);
  }
  .sub {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--dim);
    font-size: var(--text-xs);
  }
  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  /* Vertical tab layout: the same tabs, listed in the sidebar above the sessions. */
  .vtabs {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: var(--text-m);
  }
  .vtabs .tab,
  .vtabs .tab.active {
    flex: none;
    width: 100%;
    min-width: 0;
    max-width: none;
    height: 32px;
    margin: 0;
    border-radius: var(--radius-m);
    box-shadow: inset 2px 0 0 var(--tc, transparent);
  }
  .vtabs .label {
    flex: 1;
    padding-left: var(--space-3);
  }
  .vtabs .close {
    flex: none;
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
    gap: var(--space-2);
    height: 100%;
    min-width: 0;
    padding: 0 var(--space-1) 0 var(--space-4);
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
    margin: 0 var(--space-1) 0 var(--space-3);
    padding: 2px var(--space-1);
    border-radius: var(--radius-s);
    background: var(--input);
    color: var(--fg);
  }
  .close {
    display: grid;
    place-items: center;
    flex: none;
    width: 24px;
    height: 24px;
    margin-right: var(--space-2);
    padding: 0;
    border-radius: var(--radius-m);
    color: var(--dim);
    cursor: pointer;
  }
  .close:hover {
    background: var(--hover);
    color: var(--fg);
  }
  /* Centred in the strip, on the same line as the traffic lights and the sidebar toggle. */
  .right {
    display: flex;
    align-items: center;
    align-self: center;
    flex: none;
    gap: var(--space-2);
    margin-left: var(--space-1);
    padding-right: 2px;
  }
  nav > :global(.btn) {
    align-self: center;
  }
  .newtab {
    display: flex;
    align-self: center;
    flex: none;
  }
  /* The small arrow beside the plus: a narrow second half of the same control. */
  .newtab :global(.btn.more) {
    min-width: 16px;
    padding: var(--space-1) 0;
  }
  .mdot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .fill {
    flex: 1;
    align-self: stretch;
    min-width: var(--space-2);
  }
  /* A narrow strip keeps the icons and drops the words; each button still has its tooltip and label. */
  nav.compact .txt {
    display: none;
  }
  nav.compact .right {
    gap: 2px;
  }
  .mttl {
    flex: 1;
    white-space: nowrap;
  }
  .sshdot.none {
    visibility: hidden;
  }
  /* The dock: one panel on the right for Files and Tunnels. */
  .dock {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: none;
    box-sizing: border-box;
    min-height: 0;
    background: var(--bg-alt);
    border-left: 1px solid var(--border);
  }
  .dock.over {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    z-index: 9;
    box-shadow: -12px 0 32px rgba(0, 0, 0, 0.35);
  }
  .dockgrip {
    position: absolute;
    z-index: 2;
    top: 0;
    bottom: 0;
    left: -4px;
    width: 7px;
    cursor: col-resize;
    touch-action: none;
  }
  .dockgrip:hover {
    background: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .dockhead {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex: none;
    padding: var(--space-1) var(--space-2) 0 var(--space-2);
    border-bottom: 1px solid var(--border);
    font-size: var(--text-m);
  }
  .docktabs {
    display: flex;
    gap: 2px;
  }
  .docktabs button {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3) var(--space-2);
    margin-bottom: -1px;
    border-bottom: 2px solid transparent;
    color: var(--dim);
    cursor: pointer;
  }
  .docktabs button:hover {
    color: var(--fg);
  }
  .docktabs .on {
    color: var(--fg);
    border-bottom-color: var(--accent);
    font-weight: 600;
  }
  .docktabs .count {
    padding: 0 var(--space-1);
    border-radius: var(--radius-l);
    background: var(--hover);
    font-size: var(--text-xs);
    font-weight: 600;
  }
  .dockbody {
    display: flex;
    flex: 1;
    min-width: 0;
    min-height: 0;
  }
  /* The panel behind stays mounted, so a folder listing or a running upload is still there when you come back. */
  .dockbody.off {
    display: none;
  }
  /* Broadcast is on: the button that stops it is red. */
  nav :global(.btn.warn) {
    color: var(--red-text);
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
    border: 1px solid var(--focus);
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
  /* The pane's top edge. At rest it is a slim strip with a short handle (shown when the pointer is
     over the pane). Under the pointer, or with keyboard focus inside, it opens into the header. */
  .phead {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    z-index: 7;
    display: flex;
    align-items: center;
    gap: 2px;
    box-sizing: border-box;
    height: 12px;
    padding: 0 var(--space-1);
    overflow: hidden;
    font-size: var(--text-s);
    color: var(--dim);
  }
  .phead:hover,
  .phead:focus-within,
  .leaf.dragging .phead {
    z-index: 10;
    height: 26px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  .pdrag {
    position: relative;
    display: flex;
    align-items: center;
    flex: 1;
    align-self: stretch;
    min-width: 0;
    cursor: grab;
    touch-action: none;
  }
  .leaf.dragging .pdrag {
    cursor: grabbing;
  }
  .handle {
    position: absolute;
    top: 4px;
    left: calc(50% - 18px);
    width: 36px;
    height: 4px;
    border-radius: var(--radius-s);
    background: var(--border-strong);
    opacity: 0;
    transition: opacity 0.12s;
  }
  .leaf:hover .handle {
    opacity: 1;
  }
  .ptitle {
    min-width: 0;
    padding-left: var(--space-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ptitle,
  .phead button {
    visibility: hidden;
  }
  .phead:hover .ptitle,
  .phead:hover button,
  .phead:focus-within .ptitle,
  .phead:focus-within button,
  .leaf.dragging .ptitle {
    visibility: visible;
  }
  .phead:hover .handle,
  .phead:focus-within .handle {
    opacity: 0;
  }
  /* Kept focusable while hidden, so Tab can reach the header: focus then opens it. */
  .phead:not(:hover):not(:focus-within) button {
    visibility: visible;
    opacity: 0;
  }
  .phead button {
    display: grid;
    place-items: center;
    flex: none;
    width: 24px;
    height: 24px;
    padding: 0;
    border-radius: var(--radius-m);
    color: var(--dim);
    cursor: pointer;
  }
  .phead button:hover {
    background: var(--hover);
    color: var(--fg);
  }
  .phead button.on {
    color: var(--accent-text);
  }
  /* Panes without focus sit a little back, so the one that takes the keys stands out (a setting). */
  .leaf.dim::before {
    content: "";
    position: absolute;
    inset: 0;
    z-index: 6;
    background: var(--bg);
    opacity: 0.32;
    pointer-events: none;
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
    gap: var(--space-2);
  }
  .mutebadge {
    background: var(--dim);
  }
  .zoombadge {
    all: unset;
    padding: 1px var(--space-2);
    border-radius: var(--radius-s);
    background: var(--accent);
    color: var(--on-accent);
    font-size: var(--text-xs);
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
