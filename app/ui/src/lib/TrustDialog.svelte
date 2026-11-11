<script lang="ts">
  import Button from "./Button.svelte";
  import Dialog from "./Dialog.svelte";
  import type { HostKeyPrompt } from "./bindings/HostKeyPrompt";

  let { prompt, onanswer }: { prompt: HostKeyPrompt; onanswer: (trust: boolean) => void } = $props();
</script>

<Dialog title="Trust this host?" alert top onclose={() => onanswer(false)}>
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
  {#snippet actions()}
    <!-- Focus starts on Cancel, so a stray Enter never trusts a key. -->
    <Button initialFocus onclick={() => onanswer(false)}>Cancel</Button>
    <Button variant="primary" onclick={() => onanswer(true)}>Trust and connect</Button>
  {/snippet}
</Dialog>

<style>
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
