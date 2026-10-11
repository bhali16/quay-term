<script lang="ts">
  import { onMount } from "svelte";
  import { settings } from "./settings.svelte";
  import { themes } from "./themes";
  import { actionLabels, defaultBindings, describe, type Action } from "./keys";
  import { getDiagnostics } from "./ipc";
  import type { Diagnostics } from "./bindings/Diagnostics";

  let { onclose }: { onclose: () => void } = $props();

  let error = $state<string | null>(null);
  let recording = $state<Action | null>(null);
  let diag = $state<Diagnostics | null>(null);
  let copied = $state(false);
  type Tab = "appearance" | "terminal" | "shortcuts" | "about";
  const tabs: { id: Tab; label: string }[] = [
    { id: "appearance", label: "Appearance" },
    { id: "terminal", label: "Terminal" },
    { id: "shortcuts", label: "Shortcuts" },
    { id: "about", label: "About" },
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
          <button role="tab" aria-selected={tab === t.id} class:on={tab === t.id} onclick={() => (tab = t.id)}>{t.label}</button>
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
            <input type="checkbox" checked={settings.value.cursor_blink} onchange={(e) => patch({ cursor_blink: e.currentTarget.checked })} />
            Blinking cursor
          </label>
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
        {:else if diag}
          <h3>quay-term</h3>
          <div class="about">
            <span>Version {diag.version} · {diag.os} {diag.arch}</span>
            <span class="path" title={diag.log_dir}>Logs: {diag.log_dir}</span>
            <span class="path" title={diag.settings_file}>Settings: {diag.settings_file}</span>
            <button class="copy" onclick={copyDiag}>{copied ? "Copied" : "Copy diagnostics"}</button>
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
    color: #fff;
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
  .swatches {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .sw {
    position: relative;
    width: 76px;
    height: 52px;
    border: 2px solid var(--border);
    border-radius: 8px;
    cursor: pointer;
    font: 600 15px ui-monospace, Menlo, monospace;
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
    font-family: ui-monospace, Menlo, Consolas, monospace;
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
    color: #ff8a80;
  }
</style>
