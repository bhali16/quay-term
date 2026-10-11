<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { homeDir, join } from "@tauri-apps/api/path";

  let { value = $bindable("") }: { value: string } = $props();

  async function browse() {
    try {
      const start = await join(await homeDir(), ".ssh").catch(() => undefined);
      const picked = await open({ multiple: false, directory: false, defaultPath: start, title: "Select private key file" });
      if (typeof picked === "string") value = picked;
    } catch {
      /* picker unavailable or cancelled; the path can still be typed */
    }
  }
</script>

<label>
  Key file
  <span class="field">
    <input bind:value placeholder="~/.ssh/id_ed25519" />
    <button type="button" onclick={browse}>Browse…</button>
  </span>
</label>

<style>
  .field {
    display: flex;
    gap: 6px;
  }
  .field input {
    flex: 1;
    min-width: 0;
  }
  .field button {
    flex: none;
    font: inherit;
    color: inherit;
    padding: 6px 12px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: none;
    cursor: pointer;
  }
  .field button:hover {
    background: var(--hover);
  }
</style>
