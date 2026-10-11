<script lang="ts">
  import { focus } from "./focus";
  let {
    title,
    onsubmit,
    oncancel,
  }: { title: string; onsubmit: (password: string, remember: boolean) => void; oncancel: () => void } = $props();

  let password = $state("");
  let remember = $state(true);
</script>

<div class="dlg-backdrop" role="presentation" onkeydown={(e) => e.key === "Escape" && oncancel()}>
  <form class="dlg" onsubmit={(e) => (e.preventDefault(), password && onsubmit(password, remember))} aria-label="Password">
    <h2>Password for {title}</h2>
    
    <input type="password" use:focus bind:value={password} autocomplete="off" />
    <label class="check"><input type="checkbox" bind:checked={remember} /> Save in the system keychain</label>
    <div class="actions">
      <button type="button" onclick={oncancel}>Cancel</button>
      <button type="submit" class="primary" disabled={!password}>Connect</button>
    </div>
  </form>
</div>
