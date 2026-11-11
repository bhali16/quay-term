<script lang="ts">
  import Button from "./Button.svelte";
  import Dialog from "./Dialog.svelte";
  import { focus } from "./focus";

  /** Asks for the name of a new top-level folder (Home has no sidebar row to type it into). */
  let { oncreate, oncancel }: { oncreate: (name: string) => void; oncancel: () => void } = $props();
  let name = $state("");
</script>

<Dialog title="New folder" onclose={oncancel} onsubmit={(e) => (e.preventDefault(), name.trim() && oncreate(name.trim()))}>
  <label>
    Name
    <input type="text" use:focus bind:value={name} autocomplete="off" placeholder="Production" />
  </label>
  {#snippet actions()}
    <Button onclick={oncancel}>Cancel</Button>
    <Button type="submit" variant="primary" disabled={!name.trim()}>Create folder</Button>
  {/snippet}
</Dialog>
