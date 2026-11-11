<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { homeDir, join } from "@tauri-apps/api/path";
  import Button from "./Button.svelte";

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
    <input class="mono" bind:value placeholder="~/.ssh/id_ed25519" />
    <Button onclick={browse}>Browse…</Button>
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
</style>
