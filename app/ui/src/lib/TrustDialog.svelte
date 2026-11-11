<script lang="ts">
  import { onMount } from "svelte";
  import { focus } from "./focus";
  import { onEscape } from "./escape";
  import { modal } from "./modal";
  import type { HostKeyPrompt } from "./bindings/HostKeyPrompt";

  let { prompt, onanswer }: { prompt: HostKeyPrompt; onanswer: (trust: boolean) => void } = $props();

  // Level 1: drawn above every other dialog, so it owns Escape whichever opened first.
  onMount(() => onEscape(() => onanswer(false), 1));
</script>

<div class="dlg-backdrop top" role="presentation">
  <div use:modal class="dlg" role="alertdialog" aria-label="Unknown host key" tabindex="-1">
    <h2>Trust this host?</h2>
    <p>
      The authenticity of <strong>{prompt.host}:{prompt.port}</strong> cannot be verified. This is the first time you
      connect to it.
    </p>
    <dl>
      <dt>Key type</dt>
      <dd>{prompt.key_type}</dd>
      <dt>Fingerprint</dt>
      <dd class="fp">{prompt.fingerprint}</dd>
    </dl>
    <p class="hint">Compare the fingerprint with one from the server's owner. Trusting saves it to known_hosts.</p>
    <div class="actions">
      <button use:focus onclick={() => onanswer(false)}>Cancel</button>
      <button class="primary" onclick={() => onanswer(true)}>Trust and connect</button>
    </div>
  </div>
</div>

<style>
  /* Above the other dialogs (z-index 10): a host-key prompt must never be hidden behind one. */
  .top {
    z-index: 20;
  }
  p {
    margin: 0;
    line-height: 1.5;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px 14px;
    margin: 0;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-deep);
  }
  dt {
    color: var(--dim);
  }
  dd {
    margin: 0;
  }
  .fp {
    font-family: var(--font-mono);
    font-size: 12px;
    word-break: break-all;
  }
</style>
