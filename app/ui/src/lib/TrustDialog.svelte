<script lang="ts">
  import { focus } from "./focus";
  import type { HostKeyPrompt } from "./bindings/HostKeyPrompt";

  let { prompt, onanswer }: { prompt: HostKeyPrompt; onanswer: (trust: boolean) => void } = $props();
</script>

<div class="backdrop" role="presentation" onkeydown={(e) => e.key === "Escape" && onanswer(false)}>
  <div class="dialog" role="alertdialog" aria-label="Unknown host key" tabindex="-1">
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
  .backdrop {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.5);
    z-index: 20;
  }
  .dialog {
    width: 420px;
    padding: 18px;
    border-radius: 10px;
    background: #2b2d36;
    font-size: 13px;
  }
  h2 {
    margin: 0 0 8px;
    font-size: 15px;
  }
  p {
    margin: 6px 0;
  }
  dl {
    margin: 10px 0;
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 12px;
  }
  dt {
    opacity: 0.6;
  }
  dd {
    margin: 0;
  }
  .fp {
    font-family: ui-monospace, Menlo, Consolas, monospace;
    word-break: break-all;
  }
  .hint {
    opacity: 0.6;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 12px;
  }
  button {
    font: inherit;
    color: inherit;
    padding: 6px 14px;
    border: 1px solid #44464f;
    border-radius: 5px;
    background: none;
    cursor: pointer;
  }
  .primary {
    background: #3b6fd8;
    border-color: #3b6fd8;
  }
</style>
