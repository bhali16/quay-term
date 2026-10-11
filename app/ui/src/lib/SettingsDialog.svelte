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
    <label>
      Theme
      <select value={settings.value.theme} onchange={(e) => patch({ theme: e.currentTarget.value })}>
        {#each themes as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
      </select>
    </label>
    <label>
      Terminal font
      <input value={settings.value.font_family} onchange={(e) => patch({ font_family: e.currentTarget.value })} />
    </label>
    <div class="row">
      <label class="grow">
        Font size
        <input type="number" min="8" max="36" value={settings.value.font_size} onchange={(e) => patch({ font_size: Number(e.currentTarget.value) || 14 })} />
      </label>
      <label class="grow">
        Scrollback lines
        <input type="number" min="0" max="100000" step="1000" value={settings.value.scrollback} onchange={(e) => patch({ scrollback: Number(e.currentTarget.value) || 0 })} />
      </label>
    </div>
    <label class="check">
      <input type="checkbox" checked={settings.value.cursor_blink} onchange={(e) => patch({ cursor_blink: e.currentTarget.checked })} />
      Blinking cursor
    </label>

    <h3>Keyboard shortcuts</h3>
    <div class="keys">
      {#each actions as a (a)}
        <span>{actionLabels[a]}</span>
        <button class="chip" class:rec={recording === a} onclick={() => ((error = null), (recording = recording === a ? null : a))}>
          {recording === a ? "Press keys…" : settings.bindings[a]}
        </button>
        <button class="reset" title="Reset to default" disabled={!settings.value.keybindings[a]} onclick={() => setBinding(a, null)}>↺</button>
      {/each}
    </div>

    <h3>About</h3>
    {#if diag}
      <div class="about">
        <span>quay-term {diag.version} · {diag.os} {diag.arch}</span>
        <span class="path" title={diag.log_dir}>Logs: {diag.log_dir}</span>
        <span class="path" title={diag.settings_file}>Settings: {diag.settings_file}</span>
        <button class="copy" onclick={copyDiag}>{copied ? "Copied" : "Copy diagnostics"}</button>
      </div>
    {/if}

    {#if error}<p class="err">{error}</p>{/if}
    <div class="actions"><button class="primary" onclick={onclose}>Done</button></div>
  </div>
</div>

<style>
  .wide {
    width: 440px;
  }
  h3 {
    margin: 8px 0 0;
    font-size: 13px;
    opacity: 0.7;
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
