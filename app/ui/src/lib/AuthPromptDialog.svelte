<script lang="ts">
  import { focus } from "./focus";
  import type { AuthPrompt } from "./bindings/AuthPrompt";

  let { prompt, onanswer }: { prompt: AuthPrompt; onanswer: (answers: string[] | null) => void } = $props();

  // The parent re-creates this dialog for every round, so the initial value is enough.
  // svelte-ignore state_referenced_locally
  let answers = $state<string[]>(prompt.prompts.map(() => ""));
  const ready = $derived(answers.every((a) => a.length > 0));
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onanswer(null)} />

<div class="dlg-backdrop top" role="presentation">
  <form
    class="dlg"
    aria-label="Sign-in"
    onsubmit={(e) => (e.preventDefault(), ready && onanswer(answers))}
  >
    <h2>Sign in to {prompt.host}:{prompt.port}</h2>
    {#if prompt.instructions}
      <p class="hint">{prompt.instructions}</p>
    {/if}
    {#each prompt.prompts as field, i}
      <label>
        {field.text}
        {#if i === 0}
          <input
            use:focus
            type={field.echo ? "text" : "password"}
            bind:value={answers[i]}
            autocomplete="off"
          />
        {:else}
          <input type={field.echo ? "text" : "password"} bind:value={answers[i]} autocomplete="off" />
        {/if}
      </label>
    {/each}
    <div class="actions">
      <button type="button" onclick={() => onanswer(null)}>Cancel</button>
      <button type="submit" class="primary" disabled={!ready}>Continue</button>
    </div>
  </form>
</div>

<style>
  /* Same layer as the host-key prompt: it must never hide behind another dialog. */
  .top {
    z-index: 20;
  }
  .hint {
    margin: 0;
    line-height: 1.5;
    white-space: pre-line;
  }
</style>
