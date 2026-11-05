<script lang="ts">
  import { onMount } from "svelte";
  import Mark from "./Mark.svelte";
  import { api, type Info, type Stage } from "./api";

  type Phase = "welcome" | "installing" | "done" | "error";

  let phase = $state<Phase>("welcome");
  let info = $state<Info | null>(null);
  let dir = $state("");
  let existing = $state(false);
  let progress = $state(0);
  let stage = $state<Stage>("extract");
  let message = $state("");
  let exe = $state("");
  let launchAfter = $state(true);
  let customize = $state(false);

  const features = [
    ["SSH, done properly", "Jump hosts, port forwards, saved sessions"],
    ["Terminal and SFTP", "Split panes, snippets, a file panel"],
    ["Yours alone", "Encrypted vault, signed updates"],
  ];

  const stageLabel = $derived(
    stage === "extract" ? "Unpacking the installer" : progress < 82 ? "Installing Quay Term" : "Finishing up",
  );

  onMount(() => {
    // The preview in a plain browser can jump straight to a screen: ?phase=done
    const forced = new URLSearchParams(location.search).get("phase");
    api.info().then((i) => {
      info = i;
      dir = i.defaultDir;
      existing = i.existing;
      if (import.meta.env.DEV && (forced === "done" || forced === "error")) {
        phase = forced;
        exe = dir;
        message = "The installer stopped with code 2. If Quay Term is open, close it and try again.";
      }
    });
    const un = api.onStage((s) => (stage = s));
    return () => void un.then((f) => f());
  });

  async function change() {
    const picked = await api.pickFolder(dir);
    if (picked) {
      dir = picked;
      existing = await api.isInstalled(picked);
    }
  }

  async function start() {
    phase = "installing";
    progress = 3;
    stage = "extract";
    // The installer reports no progress, so ease towards the end and land on 100 when
    // it actually returns.
    const timer = setInterval(() => (progress += (92 - progress) * 0.025), 120);
    try {
      exe = await api.install(dir);
      progress = 100;
      await new Promise((r) => setTimeout(r, 650));
      phase = "done";
    } catch (e) {
      message = String(e);
      phase = "error";
    } finally {
      clearInterval(timer);
    }
  }

  async function finish() {
    if (launchAfter && exe) await api.launch(exe);
    else await api.close();
  }
</script>

<div class="win">
  <header data-tauri-drag-region>
    <div class="title" data-tauri-drag-region>
      <Mark size={16} id="t" />
      <span data-tauri-drag-region>Quay Term Setup</span>
    </div>
    <div class="ctl">
      <button aria-label="Minimize" onclick={() => api.minimize()}>
        <svg width="10" height="10" viewBox="0 0 10 10"><path d="M1 5h8" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" /></svg>
      </button>
      <button class="x" aria-label="Close" onclick={() => api.close()} disabled={phase === "installing"}>
        <svg width="10" height="10" viewBox="0 0 10 10"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" /></svg>
      </button>
    </div>
  </header>

  <div class="body">
    <aside>
      <div class="hero">
        <div class="glow"></div>
        <Mark size={84} id="h" />
      </div>
      <h2>Quay Term</h2>
      <p class="ver">{info ? `Version ${info.version}` : ""}</p>
      <ul>
        {#each features as [t, d]}
          <li><strong>{t}</strong><span>{d}</span></li>
        {/each}
      </ul>
      <div class="prompt" aria-hidden="true"><span class="chev">❯</span> quay<span class="cursor"></span></div>
    </aside>

    <main>
      {#if phase === "welcome"}
        <div class="page">
          <h1>{existing ? "Update Quay Term" : "Install Quay Term"}</h1>
          <p class="lede">A fast SSH client, terminal and SFTP manager. Setup takes about a minute.</p>

          <div class="card">
            <div class="row">
              <div class="k">Install for</div>
              <div class="v">Just you <span class="dim">· no administrator rights needed</span></div>
            </div>
            <div class="row">
              <div class="k">Location</div>
              <div class="v path" title={dir}>{dir}</div>
              <button class="link" onclick={() => (customize = !customize)}>{customize ? "Done" : "Change"}</button>
            </div>
            {#if customize}
              <div class="row sub">
                <button class="ghost" onclick={change}>Browse…</button>
                <span class="dim">Your settings and sessions are kept separately and are never touched.</span>
              </div>
            {/if}
          </div>

          {#if info && !info.hasPayload}
            <p class="warn">This build has no installer payload, so it cannot install.</p>
          {/if}

          <div class="foot">
            <span class="legal">Released under the GPL-3.0 license.</span>
            <button class="primary" onclick={start} disabled={!info?.hasPayload}>{existing ? "Update" : "Install"}</button>
          </div>
        </div>
      {:else if phase === "installing"}
        <div class="page center">
          <h1>{existing ? "Updating…" : "Installing…"}</h1>
          <p class="lede">{stageLabel}</p>
          <div class="bar" role="progressbar" aria-valuenow={Math.round(progress)} aria-valuemin="0" aria-valuemax="100">
            <div class="fill" style="width:{progress}%"></div>
          </div>
          <p class="pct">{Math.round(progress)}%</p>
        </div>
      {:else if phase === "done"}
        <div class="page center">
          <div class="badge ok">
            <svg width="30" height="30" viewBox="0 0 24 24" fill="none"><path d="M5 12.5l4.5 4.5L19 7.5" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" /></svg>
          </div>
          <h1>You're all set</h1>
          <p class="lede">Quay Term is ready. You'll find it in the Start menu.</p>
          <label class="check">
            <input type="checkbox" bind:checked={launchAfter} />
            <span class="box"></span>
            Open Quay Term now
          </label>
          <div class="foot end"><button class="primary" onclick={finish}>Finish</button></div>
        </div>
      {:else}
        <div class="page center">
          <div class="badge bad">
            <svg width="28" height="28" viewBox="0 0 24 24" fill="none"><path d="M12 7v6.5M12 17.2v.1" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" /></svg>
          </div>
          <h1>Setup could not finish</h1>
          <p class="lede err">{message}</p>
          <div class="foot end">
            <button class="ghost" onclick={() => api.close()}>Close</button>
            <button class="primary" onclick={() => (phase = "welcome")}>Try again</button>
          </div>
        </div>
      {/if}
    </main>
  </div>
</div>

<style>
  .win {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg);
    border: 1px solid var(--border);
    box-sizing: border-box;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 42px;
    flex: none;
    background: var(--bg-deep);
    border-bottom: 1px solid #00000040;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 9px;
    padding-left: 14px;
    font-size: 12px;
    color: var(--dim);
  }
  .ctl {
    display: flex;
    height: 100%;
  }
  .ctl button {
    width: 46px;
    border: 0;
    background: none;
    color: var(--dim);
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  .ctl button:hover:not(:disabled) {
    background: var(--hover);
    color: var(--fg);
  }
  .ctl .x:hover:not(:disabled) {
    background: #e5484d;
    color: #fff;
  }
  .ctl button:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }

  aside {
    width: 270px;
    flex: none;
    box-sizing: border-box;
    padding: 34px 26px 22px;
    background: linear-gradient(180deg, #232a4d 0%, var(--bg-deep) 70%);
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    border-right: 1px solid #00000040;
  }
  .hero {
    position: relative;
    margin-bottom: 18px;
  }
  .glow {
    position: absolute;
    inset: -30px;
    background: radial-gradient(closest-side, #38bdf855, transparent);
    filter: blur(8px);
  }
  .hero :global(svg) {
    position: relative;
    filter: drop-shadow(0 10px 22px #00000070);
  }
  aside h2 {
    margin: 0;
    font-size: 21px;
    font-weight: 700;
  }
  .ver {
    margin: 3px 0 22px;
    font-size: 12px;
    color: var(--dim);
    min-height: 16px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 13px;
  }
  li {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-left: 12px;
    border-left: 2px solid #38bdf866;
    font-size: 12px;
  }
  li strong {
    font-weight: 500;
    font-size: 13px;
  }
  li span {
    color: var(--dim);
  }
  .prompt {
    margin-top: auto;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--dim);
  }
  .chev {
    color: #5eead4;
  }
  .cursor {
    display: inline-block;
    width: 7px;
    height: 13px;
    margin-left: 3px;
    vertical-align: -2px;
    background: #38bdf8;
    border-radius: 2px;
    animation: blink 1.1s steps(1) infinite;
  }
  @keyframes blink {
    50% {
      opacity: 0;
    }
  }

  main {
    flex: 1;
    min-width: 0;
    display: flex;
  }
  .page {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 40px 40px 26px;
    animation: rise 0.28s ease-out;
  }
  .page.center {
    justify-content: center;
    align-items: center;
    text-align: center;
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }
  h1 {
    margin: 0 0 8px;
    font-size: 26px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .lede {
    margin: 0 0 24px;
    font-size: 14px;
    line-height: 1.5;
    color: var(--dim);
    max-width: 380px;
  }
  .lede.err {
    color: var(--red);
    font-size: 13px;
    user-select: text;
    -webkit-user-select: text;
  }

  .card {
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    font-size: 13px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
  }
  .row + .row {
    border-top: 1px solid #ffffff10;
  }
  .row.sub {
    font-size: 12px;
    background: #00000018;
    border-radius: 0 0 10px 10px;
  }
  .k {
    width: 68px;
    flex: none;
    color: var(--dim);
  }
  .v {
    flex: 1;
    min-width: 0;
  }
  .v.path {
    font-family: var(--font-mono);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }
  .dim {
    color: var(--dim);
  }
  .link {
    background: none;
    border: 0;
    color: #7aa2ff;
    cursor: pointer;
    font-size: 13px;
    padding: 2px 4px;
  }
  .link:hover {
    text-decoration: underline;
  }
  .warn {
    margin: 14px 0 0;
    font-size: 12px;
    color: #e5c07b;
  }

  .foot {
    margin-top: auto;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    width: 100%;
  }
  .foot.end {
    justify-content: flex-end;
    margin-top: 30px;
  }
  .legal {
    font-size: 11px;
    color: var(--dim);
  }
  .primary,
  .ghost {
    height: 36px;
    padding: 0 22px;
    border-radius: 8px;
    border: 1px solid transparent;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.12s, border-color 0.12s;
  }
  .primary {
    background: var(--accent);
    color: #fff;
  }
  .primary:hover:not(:disabled) {
    background: #4a7ee6;
  }
  .primary:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .ghost {
    background: transparent;
    border-color: var(--border);
  }
  .ghost:hover {
    background: var(--hover);
  }
  .row .ghost {
    height: 28px;
    padding: 0 14px;
    font-size: 12px;
  }

  .bar {
    width: 320px;
    height: 6px;
    border-radius: 3px;
    background: var(--surface);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    border-radius: 3px;
    background: linear-gradient(90deg, #5eead4, #38bdf8);
    transition: width 0.18s linear;
  }
  .pct {
    margin: 12px 0 0;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--dim);
  }

  .badge {
    width: 60px;
    height: 60px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    margin-bottom: 18px;
  }
  .badge.ok {
    background: #4ec9a022;
    color: var(--green);
    box-shadow: 0 0 0 1px #4ec9a055;
  }
  .badge.bad {
    background: #f06b6b22;
    color: var(--red);
    box-shadow: 0 0 0 1px #f06b6b55;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    cursor: pointer;
  }
  .check input {
    position: absolute;
    opacity: 0;
  }
  .box {
    width: 18px;
    height: 18px;
    border-radius: 5px;
    border: 1px solid var(--border);
    background: var(--input);
    display: grid;
    place-items: center;
  }
  .check input:checked + .box {
    background: var(--accent);
    border-color: var(--accent);
  }
  .check input:checked + .box::after {
    content: "";
    width: 9px;
    height: 5px;
    border-left: 2px solid #fff;
    border-bottom: 2px solid #fff;
    transform: translateY(-1px) rotate(-45deg);
  }
  .check input:focus-visible + .box {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
</style>
