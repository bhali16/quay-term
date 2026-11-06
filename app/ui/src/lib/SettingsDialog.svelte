<script lang="ts">
  import { onMount } from "svelte";
  import Icon, { type Name } from "./Icon.svelte";
  import { settings } from "./settings.svelte";
  import { updates } from "./updates.svelte";
  import { themes } from "./themes";
  import { actionLabels, defaultBindings, describe, type Action } from "./keys";
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { getDiagnostics, vaultExport, vaultImport } from "./ipc";
  import type { BackupReport } from "./bindings/BackupReport";
  import type { VaultStatus } from "./bindings/VaultStatus";
  import type { Diagnostics } from "./bindings/Diagnostics";

  let {
    onclose,
    onimported,
    vault = null,
    onunlock,
  }: { onclose: () => void; onimported?: () => void; vault?: VaultStatus | null; onunlock?: () => void } = $props();

  let error = $state<string | null>(null);
  let recording = $state<Action | null>(null);
  let diag = $state<Diagnostics | null>(null);
  let copied = $state(false);
  type Tab = "appearance" | "terminal" | "shortcuts" | "backup" | "about";
  const tabs: { id: Tab; label: string; icon: Name }[] = [
    { id: "appearance", icon: "palette", label: "Appearance" },
    { id: "terminal", icon: "terminal", label: "Terminal" },
    { id: "shortcuts", icon: "keyboard", label: "Shortcuts" },
    { id: "backup", icon: "lock", label: "Backup" },
    { id: "about", icon: "info", label: "About" },
  ];
  let tab = $state<Tab>("appearance");

  const actions = Object.keys(actionLabels) as Action[];

  async function patch(p: Partial<typeof settings.value>) {
    error = await settings.update({ ...settings.value, ...p });
  }
  function setBinding(a: Action, combo: string | null) {
    const kb = { ...settings.value.keybindings };
    if (combo === null || combo === defaultBindings[a]) delete kb[a];
    else kb[a] = combo;
    patch({ keybindings: kb });
  }
  // The window listener runs in the capture phase so a recorded shortcut never fires its action.
  function onkey(e: KeyboardEvent) {
    if (recording) {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") recording = null;
      else {
        const d = describe(e);
        if (d) {
          const clash = actions.find((a) => a !== recording && settings.bindings[a] === d);
          if (clash) error = `${d} is already used by "${actionLabels[clash]}".`;
          else setBinding(recording, d);
          recording = null;
        }
      }
    } else if (e.key === "Escape") onclose();
  }

  onMount(() => {
    getDiagnostics().then((d) => (diag = d));
    window.addEventListener("keydown", onkey, true);
    return () => window.removeEventListener("keydown", onkey, true);
  });

  // ---- encrypted backup ----
  let pw = $state("");
  let pw2 = $state("");
  let withSecrets = $state(true);
  let busy = $state(false);
  let result = $state<string | null>(null);

  const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? "" : "s"}`;
  async function run(job: () => Promise<string | null>) {
    error = result = null;
    busy = true;
    try {
      result = await job();
    } catch (e) {
      error = `${e}`;
    } finally {
      busy = false;
    }
  }
  const doExport = () =>
    run(async () => {
      if (pw !== pw2) throw "The two passwords differ.";
      const file = await saveDialog({ title: "Save vault file", defaultPath: "quay-term.vault" });
      if (!file) return null;
      const r = await vaultExport(file, pw, withSecrets);
      return `Saved ${plural(r.sessions, "session")}${withSecrets ? `, ${plural(r.secrets, "secret")}` : ""} and ${plural(r.snippets, "snippet")}.`;
    });
  const doImport = () =>
    run(async () => {
      const file = await openDialog({ multiple: false, directory: false, title: "Open vault file" });
      if (typeof file !== "string") return null;
      const r: BackupReport = await vaultImport(file, pw);
      onimported?.();
      const bits = [`${plural(r.sessions, "session")} added`, `${plural(r.snippets, "snippet")} added`];
      if (r.secrets) bits.push(`${plural(r.secrets, "secret")} stored`);
      if (r.skipped) bits.push(`${r.skipped} already here`);
      if (r.secrets_failed) bits.push(`${r.secrets_failed} secret(s) could not be stored in the keychain`);
      return bits.join(", ") + ".";
    });

  async function copyDiag() {
    if (!diag) return;
    const text = [`quay-term ${diag.version}`, `${diag.os} ${diag.arch}`, `settings: ${diag.settings_file}`, `logs: ${diag.log_dir}`].join("\n");
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch {
      error = "Could not access the clipboard.";
    }
  }
</script>

<div class="dlg-backdrop" role="presentation" onmousedown={(e) => e.target === e.currentTarget && onclose()}>
  <div class="dlg wide" role="dialog" aria-label="Settings">
    <h2>Settings</h2>
    <div class="layout">
      <div class="tabs" role="tablist">
        {#each tabs as t (t.id)}
          <button role="tab" aria-selected={tab === t.id} class:on={tab === t.id} onclick={() => (tab = t.id)}><Icon name={t.icon} size={15} />{t.label}</button>
        {/each}
      </div>
      <div class="pane">
        {#if tab === "appearance"}
          <h3>Theme</h3>
          <label>
            Color theme
            <select value={settings.value.theme} onchange={(e) => patch({ theme: e.currentTarget.value })}>
              {#each themes as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
            </select>
          </label>
          <label>
            Tab bar
            <select value={settings.value.tab_layout} onchange={(e) => patch({ tab_layout: e.currentTarget.value === "vertical" ? "vertical" : "horizontal" })}>
              <option value="horizontal">Horizontal (strip above the terminal)</option>
              <option value="vertical">Vertical (list on the left)</option>
            </select>
          </label>
          <div class="swatches">
            {#each themes as t (t.id)}
              <button class="sw" class:on={settings.value.theme === t.id} title={t.name} aria-label={t.name} onclick={() => patch({ theme: t.id })} style:background={t.term.background}>
                <span style:color={t.term.foreground}>Aa</span>
                <i style:background={t.ui.accent}></i>
              </button>
            {/each}
          </div>
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
            <input type="checkbox" checked={settings.value.check_for_updates} onchange={(e) => patch({ check_for_updates: e.currentTarget.checked })} />
            Check for updates when quay-term starts
          </label>
          <label class="check">
            <input type="checkbox" checked={settings.value.focus_follows_mouse} onchange={(e) => patch({ focus_follows_mouse: e.currentTarget.checked })} />
            Focus the pane under the mouse pointer without clicking
          </label>
          <label class="check">
            <input type="checkbox" checked={settings.value.show_hints} onchange={(e) => patch({ show_hints: e.currentTarget.checked })} />
            Show a reconnect bar when a session ends
          </label>
          <label class="check">
            <input type="checkbox" checked={settings.value.shell_integration} onchange={(e) => patch({ shell_integration: e.currentTarget.checked })} />
            Mark prompts and commands in new local zsh and bash shells (jump between prompts, copy output)
          </label>
          <label class="check">
            <input type="checkbox" checked={settings.value.block_gutter} onchange={(e) => patch({ block_gutter: e.currentTarget.checked })} />
            Show a green or red bar beside finished commands
          </label>
          <label class="check">
            <input type="checkbox" checked={settings.value.notifications} onchange={(e) => patch({ notifications: e.currentTarget.checked })} />
            Notify me when a long command finishes or a session drops while quay-term is in the background
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
          {/if}
        {:else if tab === "shortcuts"}
          <div class="keys">
            {#each actions as a (a)}
              <span>{actionLabels[a]}</span>
              <button class="chip" class:rec={recording === a} onclick={() => ((error = null), (recording = recording === a ? null : a))}>
                {recording === a ? "Press keys…" : settings.bindings[a]}
              </button>
              <button class="reset" title="Reset to default" disabled={!settings.value.keybindings[a]} onclick={() => setBinding(a, null)}>↺</button>
            {/each}
          </div>
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
              <div class="brow"><button class="copy" onclick={onunlock}>{vault.is_new ? "Set a master password…" : "Unlock…"}</button></div>
            {/if}
          {/if}
          <h3>Move to another machine</h3>
          <p class="blurb">Saves your sessions, folders, jump hosts, tunnels and snippets in one file, locked with a master password (Argon2id and ChaCha20-Poly1305). Importing adds what is missing and never changes what you have.</p>
          <label>
            Master password
            <input type="password" autocomplete="new-password" bind:value={pw} />
          </label>
          <label>
            Repeat it (to export)
            <input type="password" autocomplete="new-password" bind:value={pw2} />
          </label>
          <label class="check"><input type="checkbox" bind:checked={withSecrets} /> Include saved passwords and key passphrases</label>
          <div class="brow">
            <button class="copy" disabled={busy || pw.length < 8} onclick={doExport}>Export…</button>
            <button class="copy" disabled={busy || !pw} onclick={doImport}>Import…</button>
          </div>
          {#if pw && pw.length < 8}<p class="blurb">A master password needs at least 8 characters to export.</p>{/if}
          {#if busy}<p class="blurb">Working… this takes a moment on purpose.</p>{/if}
          {#if result}<p class="ok" role="status">{result}</p>{/if}
        {:else if diag}
          <h3>quay-term</h3>
          <div class="about">
            <span>Version {diag.version} · {diag.os} {diag.arch}</span>
            <span class="path" title={diag.log_dir}>Logs: {diag.log_dir}</span>
            <span class="path" title={diag.settings_file}>Settings: {diag.settings_file}</span>
            <button class="copy" onclick={copyDiag}>{copied ? "Copied" : "Copy diagnostics"}</button>
          </div>
          <h3>Updates</h3>
          <div class="about" aria-live="polite">
            {#if updates.state.kind === "available"}
              <span>Version {updates.state.version} is available.</span>
              {#if updates.state.notes}<span class="path" title={updates.state.notes}>{updates.state.notes}</span>{/if}
              <button class="copy" onclick={() => updates.install()}>Install and restart</button>
            {:else if updates.state.kind === "downloading"}
              <span>Downloading {updates.state.version}{updates.state.percent === null ? "…" : ` – ${updates.state.percent}%`}</span>
            {:else if updates.state.kind === "checking"}
              <span>Checking…</span>
            {:else}
              {#if updates.state.kind === "current"}<span>You are on the latest version.</span>{/if}
              {#if updates.state.kind === "error"}<span class="err">Could not check: {updates.state.message}</span>{/if}
              <button class="copy" onclick={() => updates.check()}>Check for updates</button>
            {/if}
          </div>
        {/if}
      </div>
    </div>
    {#if error}<p class="err">{error}</p>{/if}
    <div class="actions"><button class="primary" onclick={onclose}>Done</button></div>
  </div>
</div>

<style>
  .wide {
    width: 600px;
    max-width: 94vw;
  }
  .layout {
    display: flex;
    gap: 16px;
    height: 360px;
  }
  .tabs {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 130px;
    flex: none;
    padding-right: 12px;
    border-right: 1px solid var(--border);
  }
  .tabs button {
    font: inherit;
    color: inherit;
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: left;
    padding: 7px 10px;
    border: 0;
    border-radius: 5px;
    background: none;
    cursor: pointer;
    opacity: 0.75;
  }
  .tabs button:hover {
    background: var(--hover);
  }
  .tabs button.on {
    opacity: 1;
    background: var(--accent);
    color: var(--on-accent);
  }
  .pane {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    overflow: auto;
    padding-right: 2px;
  }
  /* One row of five: they share the pane's width instead of wrapping 4 + 1. */
  .swatches {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 8px;
  }
  .sw {
    position: relative;
    min-width: 0;
    height: 52px;
    border: 2px solid var(--border);
    border-radius: 8px;
    cursor: pointer;
    font: 600 15px var(--font-mono);
  }
  .sw.on {
    border-color: var(--accent);
  }
  .sw i {
    position: absolute;
    right: 6px;
    bottom: 6px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
  h3 {
    margin: 4px 0 0;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    opacity: 0.6;
  }
  .keys {
    display: grid;
    grid-template-columns: 1fr auto auto;
    gap: 6px 8px;
    align-items: center;
  }
  .chip,
  .reset,
  .copy {
    font: inherit;
    color: inherit;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg-alt);
    padding: 3px 10px;
    cursor: pointer;
  }
  .chip {
    min-width: 110px;
    font-family: var(--font-mono);
    font-size: 12px;
  }
  .chip.rec {
    border-color: var(--accent);
  }
  .reset {
    padding: 3px 7px;
  }
  .reset:disabled {
    opacity: 0.3;
    cursor: default;
  }
  .blurb {
    margin: 0;
    font-size: 12.5px;
    opacity: 0.75;
  }
  .brow {
    display: flex;
    gap: 8px;
  }
  .copy:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .ok {
    margin: 0;
    color: var(--green);
    font-size: 13px;
  }
  .about {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    opacity: 0.7;
  }
  .copy {
    align-self: flex-start;
    margin-top: 4px;
  }
  .err {
    margin: 0;
    color: var(--red);
  }
</style>
