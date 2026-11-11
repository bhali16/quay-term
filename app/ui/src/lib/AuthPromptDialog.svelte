<script lang="ts">
  import Button from "./Button.svelte";
  import Dialog from "./Dialog.svelte";
  import { focus } from "./focus";
  import type { AuthPrompt } from "./bindings/AuthPrompt";

  let { prompt, onanswer }: { prompt: AuthPrompt; onanswer: (answers: string[] | null) => void } = $props();

  // The parent re-creates this dialog for every round, so the initial value is enough.
  // svelte-ignore state_referenced_locally
  let answers = $state<string[]>(prompt.prompts.map(() => ""));
  const ready = $derived(answers.every((a) => a.length > 0));
</script>

<Dialog title={`Sign in to ${prompt.host}:${prompt.port}`} top onclose={() => onanswer(null)} onsubmit={(e) => (e.preventDefault(), ready && onanswer(answers))}>
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
  {#snippet actions()}
    <Button onclick={() => onanswer(null)}>Cancel</Button>
    <Button type="submit" variant="primary" disabled={!ready}>Continue</Button>
  {/snippet}
</Dialog>

<style>
  .hint {
    margin: 0;
    line-height: 1.5;
    white-space: pre-line;
  }
</style>
