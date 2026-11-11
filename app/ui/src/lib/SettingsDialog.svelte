<script lang="ts">
  import { onMount } from "svelte";
  import Button from "./Button.svelte";
  import Dialog from "./Dialog.svelte";
  import { roving } from "./roving";
  import Icon, { type Name } from "./Icon.svelte";
  import { settings } from "./settings.svelte";
  import { updates } from "./updates.svelte";
  import { themes, type Theme } from "./themes";
  import { actionGroups, actionLabels, defaultBindings, describe, formatShortcut, UNBOUND, type Action } from "./keys";
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { sendNotification } from "@tauri-apps/plugin-notification";
  import { ensurePermission, type Permission } from "./notify";
  import { getDiagnostics, historyClear, historyCount, vaultExport, vaultImport } from "./ipc";
  import { compileDeny, DEFAULT_DENY } from "./history";
  import type { BackupReport } from "./bindings/BackupReport";
  import type { VaultStatus } from "./bindings/VaultStatus";
  import type { Diagnostics } from "./bindings/Diagnostics";

  let {
    onclose,
    onimported,
    onsshimport,
    vault = null,
    onunlock,
    initialTab,
  }: { onclose: () => void; onimported?: () => void; onsshimport?: () => void; vault?: VaultStatus | null; onunlock?: () => void; initialTab?: Tab } = $props();

  let error = $state<string | null>(null);
  // Result of the last permission request, so a refusal is explained here instead of failing silently.
  let permission = $state<Permission | null>(null);
  async function setNotifications(on: boolean) {
    patch({ notifications: on });
    permission = on ? await ensurePermission() : null;
  }
  async function testNotification() {
    permission = await ensurePermission();
    if (permission === "granted") sendNotification({ title: "Quay Term", body: "Notifications are working." });
  }
  let recording = $state<Action | null>(null);
  let diag = $state<Diagnostics | null>(null);
  let copied = $state(false);
  type Tab = "general" | "appearance" | "terminal" | "history" | "shortcuts" | "backup" | "about";
  const tabs: { id: Tab; label: string; icon: Name }[] = [
    { id: "general", icon: "settings", label: "General" },
    { id: "appearance", icon: "palette", label: "Appearance" },
    { id: "terminal", icon: "terminal", label: "Terminal" },
    { id: "history", icon: "search", label: "History" },
    { id: "shortcuts", icon: "keyboard", label: "Shortcuts" },
    { id: "backup", icon: "lock", label: "Backup" },
    { id: "about", icon: "info", label: "About" },
  ];
  // svelte-ignore state_referenced_locally
  let tab = $state<Tab>(initialTab ?? "general");

  // ---- command history ----
  let histCount = $state<number | null>(null);
  let confirmForget = $state(false);
  let histNote = $state<string | null>(null);
  const loadCount = () => historyCount().then((n) => (histCount = n)).catch(() => (histCount = null));
  $effect(() => {
    if (tab === "history" || tab === "backup") loadCount();
  });
  const denyText = $derived(settings.value.history_deny_patterns.join("\n"));
  const denyInvalid = $derived(compileDeny(settings.value.history_deny_patterns).invalid);
  const setDeny = (text: string) => patch({ history_deny_patterns: text.split("\n").map((l) => l.trim()).filter(Boolean) });
  async function forgetAll() {
    confirmForget = false;
    try {
      await historyClear();
      histNote = "History deleted from this computer.";
    } catch (e) {
      error = `${e}`;
    }
    loadCount();
  }

  const actionIds = Object.keys(actionLabels) as Action[];

  async function patch(p: Partial<typeof settings.value>) {
    error = await settings.update({ ...settings.value, ...p });
  }

  // ---- theme ----
  /** Following the system starts from the theme in use, so turning it on changes nothing at first. */
  function setMatchSystem(on: boolean) {
    const now = settings.theme;
    patch(on ? { theme_match_system: true, ...(now.dark ? { theme_dark: now.id } : { theme_light: now.id }) } : { theme_match_system: false, theme: now.id });
  }

  // ---- shortcuts ----
  let keyFilter = $state("");
  /** The groups with the rows that match the filter (by name or by shortcut). */
  const keyGroups = $derived.by(() => {
    const q = keyFilter.trim().toLowerCase();
    const hit = (a: Action) => !q || actionLabels[a].toLowerCase().includes(q) || formatShortcut(settings.bindings[a]).toLowerCase().includes(q) || settings.bindings[a].toLowerCase().includes(q);
    return actionGroups.map((g) => ({ name: g.name, rows: g.actions.filter(hit) })).filter((g) => g.rows.length);
  });
  /** A shortcut the user chose for `action` that `other` already has. Shown under the row, with Replace. */
  let clash = $state<{ action: Action; combo: string; other: Action } | null>(null);
  /** The stored overrides with `a` set to `combo` (the default is stored as no override). */
  function withBinding(kb: Record<string, string>, a: Action, combo: string) {
    const next = { ...kb };
    if (combo === defaultBindings[a]) delete next[a];
    else next[a] = combo;
    return next;
  }
  /** Give `a` the shortcut `combo`, unless another action has it: then ask, right at the row. */
  function tryBind(a: Action, combo: string) {
    const other = actionIds.find((x) => x !== a && settings.bindings[x] === combo);
    if (other) clash = { action: a, combo, other };
    else {
      clash = null;
      patch({ keybindings: withBinding(settings.value.keybindings, a, combo) });
    }
  }
  /** Take the shortcut from the action that had it; that one is left without a shortcut. */
  function replaceClash() {
    if (!clash) return;
    const kb = withBinding({ ...settings.value.keybindings, [clash.other]: UNBOUND }, clash.action, clash.combo);
    clash = null;
    patch({ keybindings: kb });
  }
  // The window listener runs in the capture phase so a recorded shortcut never fires its action.
  // Escape (cancel the recording, or close) comes through the shared dialog stack instead.
  function onkey(e: KeyboardEvent) {
    if (!recording || e.key === "Escape") return;
    e.preventDefault();
    e.stopPropagation();
    const d = describe(e);
    if (d) {
      tryBind(recording, d);
      recording = null;
    }
  }

  onMount(() => {
    getDiagnostics().then((d) => (diag = d));
    window.addEventListener("keydown", onkey, true);
    return () => window.removeEventListener("keydown", onkey, true);
  });

  // ---- encrypted backup: one card to export, one to import ----
  const MIN_PW = 8;
  let pw = $state("");
  let pw2 = $state("");
  let withSecrets = $state(true);
  // Off unless ticked each time: history can hold hostnames and paths the user may not want to move.
  let withHistory = $state(false);
  /** The vault file chosen to import, and the password it was exported with. */
  let importFile = $state<string | null>(null);
  let importPw = $state("");
  let busy = $state<"export" | "import" | null>(null);
  let result = $state<{ from: "export" | "import"; text: string } | null>(null);
  const baseName = (path: string) => path.split(/[\\/]/).pop() || path;

  const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? "" : "s"}`;
  async function run(from: "export" | "import", job: () => Promise<string | null>) {
    error = result = null;
    busy = from;
    try {
      const text = await job();
      if (text) result = { from, text };
    } catch (e) {
      error = `${e}`;
    } finally {
      busy = null;
    }
  }
  const doExport = () =>
    run("export", async () => {
      if (pw !== pw2) throw "The two passwords differ.";
      const file = await saveDialog({ title: "Save vault file", defaultPath: "quay-term.vault" });
      if (!file) return null;
      const r = await vaultExport(file, pw, withSecrets, withHistory);
      const parts = [plural(r.sessions, "session")];
      if (withSecrets) parts.push(plural(r.secrets, "secret"));
      parts.push(plural(r.snippets, "snippet"));
      if (withHistory) parts.push(`${r.history} history ${r.history === 1 ? "entry" : "entries"}`);
      pw = pw2 = "";
      return `Saved ${parts.slice(0, -1).join(", ")} and ${parts[parts.length - 1]}.`;
    });
  async function chooseImport() {
    try {
      const file = await openDialog({ multiple: false, directory: false, title: "Open vault file" });
      if (typeof file !== "string") return;
      importFile = file;
      importPw = "";
      error = result = null;
    } catch (e) {
      error = `${e}`;
    }
  }
  const doImport = () =>
    run("import", async () => {
      if (!importFile) return null;
      const r: BackupReport = await vaultImport(importFile, importPw);
      onimported?.();
      const bits = [`${plural(r.sessions, "session")} added`, `${plural(r.snippets, "snippet")} added`];
      if (r.secrets) bits.push(`${plural(r.secrets, "secret")} stored`);
      if (r.skipped) bits.push(`${r.skipped} already here`);
      if (r.secrets_failed) bits.push(`${r.secrets_failed} secret(s) could not be stored in the keychain`);
      if (r.history) bits.push(`${r.history} history ${r.history === 1 ? "entry" : "entries"} added`);
      if (r.history_skipped)
        bits.push(
          `${r.history_skipped} history ${r.history_skipped === 1 ? "entry" : "entries"} not added (${
            !settings.value.record_history ? "turn on history in Settings > History first" : vault?.mode === "locked" ? "unlock the vault first" : "already here"
          })`,
        );
      loadCount();
      importFile = null;
      importPw = "";
      return bits.join(", ") + ".";
    });

  async function copyDiag() {
    if (!diag) return;
    const text = [`Quay Term ${diag.version}`, `${diag.os} ${diag.arch}`, `settings: ${diag.settings_file}`, `logs: ${diag.log_dir}`].join("\n");
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch {
      error = "Could not access the clipboard.";
    }
  }
</script>

{#snippet swatches(label: string, list: Theme[], current: string, pick: (id: string) => void)}
  <div class="swatches" role="radiogroup" aria-label={label}>
    {#each list as t (t.id)}
      <button class="sw" role="radio" aria-checked={current === t.id} class:on={current === t.id} onclick={() => pick(t.id)}>
        <span class="chipc" style:background={t.term.background} style:color={t.term.foreground}>Aa<i style:background={t.ui.accent}></i></span>
        <span class="swname">{t.name}</span>
      </button>
    {/each}
  </div>
{/snippet}

<!-- Escape cancels a shortcut being recorded first, then closes. -->
<Dialog title="Settings" class="wide" backdropClose onclose={() => (recording ? (recording = null) : onclose())}>
    <div class="layout">
      <div class="tabs" role="tablist" aria-orientation="vertical" aria-label="Settings sections" use:roving={{ selector: '[role="tab"]', vertical: true, activate: true }}>
        {#each tabs as t (t.id)}
          <button role="tab" id="settings-tab-{t.id}" aria-controls="settings-panel" tabindex={tab === t.id ? 0 : -1} aria-selected={tab === t.id} class:on={tab === t.id} onclick={() => (tab = t.id)}><Icon name={t.icon} size={15} />{t.label}</button>
        {/each}
      </div>
      <div class="pane" role="tabpanel" id="settings-panel" aria-labelledby="settings-tab-{tab}">
        {#if tab === "general"}
          <h3>Tabs</h3>
          <label>
            Tab bar
            <select value={settings.value.tab_layout} onchange={(e) => patch({ tab_layout: e.currentTarget.value === "vertical" ? "vertical" : "horizontal" })}>
              <option value="horizontal">Horizontal (strip above the terminal)</option>
              <option value="vertical">Vertical (in the sidebar)</option>
            </select>
          </label>
          <label class="check">
            <input type="checkbox" checked={settings.value.confirm_close} onchange={(e) => patch({ confirm_close: e.currentTarget.checked })} />
            Ask before closing a running command or a tab with several connected panes
          </label>
          <h3>Updates</h3>
          <label class="check">
            <input type="checkbox" checked={settings.value.check_for_updates} onchange={(e) => patch({ check_for_updates: e.currentTarget.checked })} />
            Check for updates when Quay Term starts
          </label>
          <p class="blurb">Nothing is installed without a click. Settings &gt; About shows the version you have and installs a new one.</p>
          <h3>Notifications</h3>
          <label class="check">
            <input type="checkbox" checked={settings.value.notifications} onchange={(e) => setNotifications(e.currentTarget.checked)} />
            Notify me when a long command finishes or a session drops while Quay Term or that tab is in the background
          </label>
          {#if settings.value.notifications}
            <label class="check">
              <input
                type="number"
                min="1"
                max="3600"
                style="width: 4.5rem"
                value={settings.value.notify_after_secs}
                onchange={(e) => patch({ notify_after_secs: Math.max(1, Math.min(3600, Math.round(+e.currentTarget.value) || 15)) })}
              />
              seconds before a command counts as long
            </label>
            <label class="check">
              <input type="checkbox" checked={settings.value.notify_show_command} onchange={(e) => patch({ notify_show_command: e.currentTarget.checked })} />
              Show the first words of the command in the notification (output is never included)
            </label>
            <p class="hint">
              Mute one pane with "{actionLabels.mute_pane}"{settings.bindings.mute_pane ? ` (${formatShortcut(settings.bindings.mute_pane)})` : ""}. Clicking a notification brings that tab forward.
              Command alerts need command marks (Settings &gt; Terminal &gt; Command marks).
            </p>
            <button class="action" onclick={testNotification}>Send a test notification</button>
            {#if permission === "denied"}
              <p class="hint warn" role="status">
                The system has notifications blocked for Quay Term, so none will appear. Allow them in your system notification settings, then send a test.
              </p>
            {:else if permission === "unknown"}
              <p class="hint warn" role="status">Could not check notification permission on this system.</p>
            {/if}
          {/if}
        {:else if tab === "appearance"}
          <h3>Theme</h3>
          <label class="check">
            <input type="checkbox" checked={settings.value.theme_match_system} onchange={(e) => setMatchSystem(e.currentTarget.checked)} />
            Match the system's light or dark appearance
          </label>
          {#if settings.value.theme_match_system}
            <p class="blurb">When the system is light</p>
            {@render swatches("Theme when the system is light", themes.filter((t) => !t.dark), settings.value.theme_light, (id) => patch({ theme_light: id }))}
            <p class="blurb">When the system is dark</p>
            {@render swatches("Theme when the system is dark", themes.filter((t) => t.dark), settings.value.theme_dark, (id) => patch({ theme_dark: id }))}
            <p class="blurb">The system is in {settings.systemDark ? "dark" : "light"} mode now, so {settings.theme.name} is showing.</p>
          {:else}
            {@render swatches("Theme", themes, settings.value.theme, (id) => patch({ theme: id }))}
          {/if}
        {:else if tab === "terminal"}
          <h3>Font</h3>
          <label>
            Font family
            <input value={settings.value.font_family} onchange={(e) => patch({ font_family: e.currentTarget.value })} />
          </label>
          <label>
            Font size
            <input type="number" min="8" max="36" value={settings.value.font_size} onchange={(e) => patch({ font_size: Number(e.currentTarget.value) || 14 })} />
          </label>
          <h3>Behavior</h3>
          <label>
            Scrollback lines
            <input type="number" min="0" max="100000" step="1000" value={settings.value.scrollback} onchange={(e) => patch({ scrollback: Number(e.currentTarget.value) || 0 })} />
          </label>
          <label class="check">
            <input type="checkbox" checked={settings.value.copy_on_select} onchange={(e) => patch({ copy_on_select: e.currentTarget.checked })} />
            Copy text to the clipboard when selected
          </label>
          <label class="check">
            <input type="checkbox" checked={settings.value.cursor_blink} onchange={(e) => patch({ cursor_blink: e.currentTarget.checked })} />
            Blinking cursor
          </label>
          <label class="check">
            <input type="checkbox" checked={settings.value.show_hints} onchange={(e) => patch({ show_hints: e.currentTarget.checked })} />
            Show a reconnect bar when a session ends
          </label>
          <h3>Split panes</h3>
          <label class="check">
            <input type="checkbox" checked={settings.value.focus_follows_mouse} onchange={(e) => patch({ focus_follows_mouse: e.currentTarget.checked })} />
            Focus the pane under the mouse pointer without clicking
          </label>
          <label class="check">
            <input type="checkbox" checked={settings.value.dim_unfocused_panes} onchange={(e) => patch({ dim_unfocused_panes: e.currentTarget.checked })} />
            Dim the panes that do not have focus
          </label>
          <h3>Command marks</h3>
          <label class="check">
            <input type="checkbox" checked={settings.value.shell_integration} onchange={(e) => patch({ shell_integration: e.currentTarget.checked })} />
            Mark prompts and commands in new local zsh and bash shells (jump between prompts, copy output)
          </label>
          <label class="check">
            <input type="checkbox" checked={settings.value.block_gutter} onchange={(e) => patch({ block_gutter: e.currentTarget.checked })} />
            Show a green or red bar beside finished commands
          </label>
          <label class="check">
            <input type="checkbox" checked={settings.value.snippet_expand} onchange={(e) => patch({ snippet_expand: e.currentTarget.checked })} />
            Experimental: type <code>;name</code> then Tab at a marked prompt to open that snippet
          </label>
          <h3>Accessibility</h3>
          <label class="check">
            <input type="checkbox" checked={settings.value.screen_reader} onchange={(e) => patch({ screen_reader: e.currentTarget.checked })} />
            Make terminal text readable by a screen reader (slower on fast output)
          </label>
        {:else if tab === "shortcuts"}
          <div class="filter"><Icon name="search" size={14} /><input type="search" placeholder="Filter by name or shortcut" aria-label="Filter shortcuts" bind:value={keyFilter} /></div>
          {#each keyGroups as g (g.name)}
            <h3>{g.name}</h3>
            <div class="keys">
              {#each g.rows as a (a)}
                <span>{actionLabels[a]}</span>
                <button class="chip" class:rec={recording === a} class:unset={!settings.bindings[a] && recording !== a} onclick={() => ((error = null), (clash = null), (recording = recording === a ? null : a))}>
                  {recording === a ? "Press keys…" : formatShortcut(settings.bindings[a]) || "Not set"}
                </button>
                <Button size="sm" title="Reset to default" aria-label={`Reset ${actionLabels[a]} to default`} disabled={!settings.value.keybindings[a]} onclick={() => tryBind(a, defaultBindings[a])}>↺</Button>
                {#if clash?.action === a}
                  <p class="clash" role="alert">
                    <span>{formatShortcut(clash.combo)} is used by "{actionLabels[clash.other]}".</span>
                    <Button size="sm" variant="primary" initialFocus onclick={replaceClash}>Replace</Button>
                    <Button size="sm" onclick={() => (clash = null)}>Cancel</Button>
                  </p>
                {/if}
              {/each}
            </div>
          {:else}
            <p class="blurb">No shortcut matches "{keyFilter}".</p>
          {/each}
        {:else if tab === "history"}
          <h3>Command history</h3>
          <label class="check">
            <input type="checkbox" checked={settings.value.record_history} onchange={(e) => patch({ record_history: e.currentTarget.checked })} />
            Save the commands I run, so I can search them across sessions
          </label>
          <p class="blurb">
            Commands are saved when they finish in a shell with command marks (Settings &gt; Terminal), with the host, folder and exit code. Search them
            with "{actionLabels.history_search}"{settings.bindings.history_search ? ` (${formatShortcut(settings.bindings.history_search)})` : ""} or type <code>$</code> in the command palette. Output is never saved.
          </p>
          <p class="blurb">
            Stored on this computer only, in <code>history.db</code> next to your settings, readable only by your user account.
            {#if !vault || vault.mode === "keychain"}
              It is not encrypted (your passwords are in the system keychain, and history is not).
            {:else}
              Each command, host and folder is encrypted with your master password.
              {vault.mode === "locked" ? "The vault is locked, so nothing is saved and history cannot be searched until you unlock it." : ""}
            {/if}
            It goes into a backup only if you tick "Include command history" when exporting. Nothing is written while this is off.
          </p>
          <h3>Never save</h3>
          <p class="blurb">
            Commands typed with a leading space are never saved. Neither are commands matching any of these patterns (regular expressions, one per line, case
            ignored).
          </p>
          <textarea
            class="deny"
            rows="6"
            spellcheck="false"
            autocapitalize="off"
            aria-label="Patterns of commands never to save"
            value={denyText}
            onchange={(e) => setDeny(e.currentTarget.value)}
          ></textarea>
          {#if denyInvalid.length}
            <p class="hint warn" role="status">Not a valid pattern, ignored: {denyInvalid.join(", ")}</p>
          {/if}
          <div class="brow">
            <Button size="sm" onclick={() => patch({ history_deny_patterns: [...DEFAULT_DENY] })}>Restore default patterns</Button>
          </div>
          <h3>Saved history</h3>
          <p class="blurb">
            {histCount === null ? "Could not read the history." : histCount === 0 ? "Nothing saved." : `${histCount} ${histCount === 1 ? "entry" : "entries"} saved.`} The newest 20,000 are kept, for up to 180 days. Remove one from the
            history search with Shift+Delete.
          </p>
          <div class="brow">
            {#if confirmForget}
              <Button variant="danger" size="sm" onclick={forgetAll}>Delete all history</Button>
              <Button size="sm" onclick={() => (confirmForget = false)}>Cancel</Button>
            {:else}
              <Button size="sm" disabled={!histCount} onclick={() => ((histNote = null), (confirmForget = true))}>Forget all history…</Button>
            {/if}
          </div>
          {#if histNote}<p class="ok" role="status">{histNote}</p>{/if}
        {:else if tab === "backup"}
          <h3>Saved passwords</h3>
          {#if vault?.mode === "keychain"}
            <p class="blurb">Kept in the system keychain.</p>
          {:else if vault}
            <p class="blurb">
              This computer has no system keychain, so they are kept in a file locked with a master password.
              {vault.mode === "unlocked" ? "It is unlocked for this launch." : "It is locked."}
            </p>
            {#if vault.mode === "locked"}
              <div class="brow"><Button size="sm" onclick={onunlock}>{vault.is_new ? "Set a master password…" : "Unlock…"}</Button></div>
            {/if}
          {/if}
          <h3>Import from SSH config</h3>
          <p class="blurb">Adds the hosts in ~/.ssh/config as saved sessions. Hosts you already have are skipped.</p>
          <div class="brow"><Button size="sm" onclick={onsshimport}><Icon name="import" size={13} />Import hosts</Button></div>
          <h3>Move to another machine</h3>
          <p class="blurb">A vault file holds your sessions, folders, jump hosts, tunnels and snippets, locked with a master password (Argon2id and ChaCha20-Poly1305).</p>
          <section class="card" aria-labelledby="backup-export">
            <h4 id="backup-export">Export</h4>
            <label>
              Master password for the file
              <input type="password" autocomplete="new-password" bind:value={pw} aria-describedby="backup-rule" />
              <span class="rule" id="backup-rule" class:bad={pw.length > 0 && pw.length < MIN_PW}>At least {MIN_PW} characters. It cannot be recovered, so keep it somewhere safe.</span>
            </label>
            <label>
              Repeat it
              <input type="password" autocomplete="new-password" bind:value={pw2} />
              {#if pw2 && pw !== pw2}<span class="rule bad" role="status">The two passwords differ.</span>{/if}
            </label>
            <label class="check"><input type="checkbox" bind:checked={withSecrets} /> Include saved passwords and key passphrases</label>
            <label class="check"><input type="checkbox" bind:checked={withHistory} disabled={!histCount} /> Include command history{histCount ? ` (${histCount} ${histCount === 1 ? "entry" : "entries"})` : ""}</label>
            <div class="brow">
              <Button size="sm" disabled={!!busy || pw.length < MIN_PW || pw !== pw2} onclick={doExport}>Export…</Button>
              {#if busy === "export"}<span class="blurb">Working… this takes a moment on purpose.</span>{/if}
            </div>
            {#if result?.from === "export"}<p class="ok" role="status">{result.text}</p>{/if}
          </section>
          <section class="card" aria-labelledby="backup-import">
            <h4 id="backup-import">Import</h4>
            <p class="blurb">Adds what is missing and never changes what you have.</p>
            <div class="brow">
              <Button size="sm" disabled={!!busy} onclick={chooseImport}>{importFile ? "Choose another file…" : "Choose a vault file…"}</Button>
              {#if importFile}<span class="path" title={importFile}>{baseName(importFile)}</span>{/if}
            </div>
            {#if importFile}
              <label>
                The password this file was exported with
                <input type="password" autocomplete="off" bind:value={importPw} onkeydown={(e) => e.key === "Enter" && importPw && !busy && doImport()} />
              </label>
              <div class="brow">
                <Button size="sm" disabled={!!busy || !importPw} onclick={doImport}>Import</Button>
                {#if busy === "import"}<span class="blurb">Working… this takes a moment on purpose.</span>{/if}
              </div>
            {/if}
            {#if result?.from === "import"}<p class="ok" role="status">{result.text}</p>{/if}
          </section>
        {:else if diag}
          <h3>Quay Term</h3>
          <div class="about">
            <span>Version {diag.version} · {diag.os} {diag.arch}</span>
            <span class="path" title={diag.log_dir}>Logs: {diag.log_dir}</span>
            <span class="path" title={diag.settings_file}>Settings: {diag.settings_file}</span>
            <Button size="sm" onclick={copyDiag}>{copied ? "Copied" : "Copy diagnostics"}</Button>
          </div>
          <h3>Updates</h3>
          <div class="about" aria-live="polite">
            {#if updates.state.kind === "available"}
              <span>Version {updates.state.version} is available.</span>
              {#if updates.state.notes}<span class="path" title={updates.state.notes}>{updates.state.notes}</span>{/if}
              <Button size="sm" onclick={() => updates.install()}>Install and restart</Button>
            {:else if updates.state.kind === "downloading"}
              <span>Downloading {updates.state.version}{updates.state.percent === null ? "…" : ` – ${updates.state.percent}%`}</span>
            {:else if updates.state.kind === "checking"}
              <span>Checking…</span>
            {:else}
              {#if updates.state.kind === "current"}<span>You are on the latest version.</span>{/if}
              {#if updates.state.kind === "error"}<span class="err">Could not check: {updates.state.message}</span>{/if}
              <Button size="sm" onclick={() => updates.check()}>Check for updates</Button>
            {/if}
          </div>
        {/if}
      </div>
    </div>
    {#if error}<p class="err">{error}</p>{/if}
  {#snippet actions()}
    <Button variant="primary" onclick={onclose}>Done</Button>
  {/snippet}
</Dialog>

<style>
  :global(.dlg.wide) {
    width: 760px;
    max-width: 94vw;
  }
  .layout {
    display: flex;
    gap: var(--space-4);
    height: min(540px, 68vh);
  }
  .tabs {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 130px;
    flex: none;
    padding-right: var(--space-3);
    border-right: 1px solid var(--border);
  }
  .tabs button {
    font: inherit;
    color: inherit;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    text-align: left;
    padding: var(--space-2) var(--space-3);
    border: 0;
    border-radius: var(--radius-m);
    background: none;
    cursor: pointer;
    color: var(--dim);
  }
  .tabs button:hover {
    background: var(--hover);
    color: var(--fg);
  }
  .tabs button.on {
    background: var(--accent);
    color: var(--on-accent);
  }
  .pane {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    overflow: auto;
    padding-right: 2px;
  }
  /* Rows keep their natural height; the pane scrolls instead of squeezing them. */
  .pane > :global(*) {
    flex-shrink: 0;
  }
  .pane > .action {
    align-self: flex-start;
    padding: var(--space-1) var(--space-4);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-m);
    background: var(--input);
    color: var(--fg);
    font: inherit;
    cursor: pointer;
  }
  .pane > .action:hover {
    background: var(--hover);
  }
  /* One row of five: they share the pane's width instead of wrapping 4 + 1. Each shows the
     theme's own colours and its name, so no separate list of names is needed. */
  .swatches {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: var(--space-2);
  }
  .sw {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: var(--dim);
    font: inherit;
    font-size: var(--text-xs);
    cursor: pointer;
  }
  .chipc {
    position: relative;
    display: grid;
    place-items: center;
    height: 48px;
    border: 2px solid var(--border-strong);
    border-radius: var(--radius-l);
    font: 600 var(--text-l) var(--font-mono);
  }
  .sw.on .chipc {
    border-color: var(--focus);
    box-shadow: 0 0 0 1px var(--focus);
  }
  .sw.on {
    color: var(--fg);
    font-weight: 600;
  }
  .chipc i {
    position: absolute;
    right: 6px;
    bottom: 6px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
  .swname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: center;
  }
  h4 {
    margin: 0;
    font-size: var(--text-m);
    font-weight: 600;
  }
  /* One job per card: export on its own, import on its own. */
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-l);
  }
  .rule {
    font-size: var(--text-xs);
    color: var(--dim);
  }
  .rule.bad {
    color: var(--red-text);
  }
  .filter {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-m);
    background: var(--input);
    color: var(--dim);
  }
  .filter:focus-within {
    border-color: var(--focus);
  }
  .filter input {
    flex: 1;
    min-width: 0;
    padding: var(--space-1) 0;
    border: 0;
    outline: 0;
    background: none;
    color: var(--fg);
    font: inherit;
  }
  .chip.unset {
    color: var(--dim);
    letter-spacing: 0;
  }
  /* Under the row it is about, across all three columns. */
  .clash {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    padding: var(--space-1) var(--space-3);
    border: 1px solid var(--yellow);
    border-radius: var(--radius-m);
    color: var(--fg);
    font-size: var(--text-s);
  }
  .clash span {
    flex: 1;
    min-width: 160px;
  }
  h3 {
    margin: var(--space-1) 0 0;
    font-size: var(--text-s);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--dim);
  }
  .keys {
    display: grid;
    grid-template-columns: 1fr auto auto;
    gap: var(--space-2) var(--space-2);
    align-items: center;
  }
  .chip {
    font: inherit;
    color: inherit;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-m);
    background: var(--bg-alt);
    padding: 2px var(--space-3);
    cursor: pointer;
  }
  /* The UI font: the modifier symbols (⌥⇧⌘) are too small to read in the mono one. */
  .chip {
    min-width: 110px;
    font-family: var(--font-ui);
    font-size: var(--text-m);
    letter-spacing: 0.06em;
  }
  .chip.rec {
    border-color: var(--focus);
  }
  .deny {
    font: var(--text-s) var(--font-mono);
    color: var(--fg);
    background: var(--input);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-m);
    padding: var(--space-1) var(--space-2);
    resize: vertical;
  }
  .blurb {
    margin: 0;
    font-size: var(--text-s);
    color: var(--dim);
  }
  .brow {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .ok {
    margin: 0;
    color: var(--green-text);
    font-size: var(--text-m);
  }
  .about {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-size: var(--text-s);
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--dim);
  }
  .err {
    margin: 0;
    color: var(--red-text);
  }
</style>
