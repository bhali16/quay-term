<script lang="ts">
  import Icon, { type Name } from "./Icon.svelte";
  import type { SavedSession } from "./bindings/SavedSession";

  /**
   * The page of a new tab, like a browser's: the ways to start a session, as large buttons,
   * and the sessions used lately. What is opened from here takes the tab's place.
   */
  let {
    active,
    sessions,
    recent,
    total,
    paletteKey = "",
    sidebarKey = "",
    onshell,
    onssh,
    onnew,
    onfolder,
    onimport,
    onconnect,
    onpalette,
    onsessions,
  }: {
    /** This Home is the tab in front: it takes the focus and the letter shortcuts. */
    active: boolean;
    /** Up to a handful of saved sessions to offer, most recent first. */
    sessions: SavedSession[];
    /** `sessions` are ones connected to lately (else just the first few saved). */
    recent: boolean;
    /** How many sessions are saved in all. */
    total: number;
    paletteKey?: string;
    sidebarKey?: string;
    onshell: () => void;
    onssh: () => void;
    onnew: () => void;
    onfolder: () => void;
    onimport: () => void;
    onconnect: (s: SavedSession) => void;
    onpalette: () => void;
    /** Show the sidebar with every saved session (left out while it is already showing). */
    onsessions?: () => void;
  } = $props();

  type Card = { icon: Name; title: string; text: string; key: string; run: () => void };
  // Plain letters, which work only here: a new tab has no terminal to type into yet.
  const cards = $derived<Card[]>([
    { icon: "terminal", title: "Local shell", text: "A terminal on this computer.", key: "L", run: onshell },
    { icon: "server", title: "SSH connection", text: "Connect to user@host once.", key: "S", run: onssh },
    { icon: "plus", title: "New saved session", text: "Save a host to reuse it.", key: "N", run: onnew },
    { icon: "folder-plus", title: "New folder", text: "Group saved sessions.", key: "F", run: onfolder },
    { icon: "import", title: "Import SSH config", text: "Bring in hosts from ~/.ssh/config.", key: "I", run: onimport },
    { icon: "command", title: "Command palette", text: "Search actions, sessions and snippets.", key: paletteKey, run: onpalette },
  ]);

  let first = $state<HTMLButtonElement>();
  $effect(() => {
    if (!active) return;
    // After the click or key that opened the tab has settled its own focus.
    const f = requestAnimationFrame(() => overlay() || first?.focus());
    return () => cancelAnimationFrame(f);
  });
  const overlay = () => !!document.querySelector('[role="dialog"], [role="alertdialog"], [role="menu"]');

  /** The plain-letter shortcuts. Not while typing somewhere, and not under a dialog or menu. */
  function onkeydown(e: KeyboardEvent) {
    if (!active || e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey || e.key.length !== 1) return;
    // The sidebar keeps its own keys; so does anything being typed into.
    if ((e.target as HTMLElement | null)?.closest?.("input, textarea, select, [contenteditable], aside") || overlay()) return;
    const card = cards.find((c) => c.key.length === 1 && c.key.toLowerCase() === e.key.toLowerCase());
    if (!card) return;
    e.preventDefault();
    card.run();
  }
</script>

<svelte:window {onkeydown} />

<section class="home" aria-label="Home">
  <div class="scroll">
    <div class="inner">
      <header>
        <h1>Quay Term</h1>
        <p>Start a session in this tab.</p>
      </header>

      <div class="cards">
        {#each cards as c, i (c.title)}
          {#if i === 0}
            <button class="card" bind:this={first} onclick={c.run}>{@render cardBody(c)}</button>
          {:else}
            <button class="card" onclick={c.run}>{@render cardBody(c)}</button>
          {/if}
        {/each}
      </div>

      {#if sessions.length}
        <div class="sessions">
          <div class="head">
            <h2>{recent ? "Recent sessions" : "Saved sessions"}</h2>
            {#if onsessions}<button class="link" onclick={onsessions}>Show all {total}{#if sidebarKey}<kbd>{sidebarKey}</kbd>{/if}</button>{/if}
          </div>
          <ul>
            {#each sessions as s (s.id)}
              <li>
                <button class="row" title="Connect to {s.name}" onclick={() => onconnect(s)}>
                  <Icon name="server" size={14} />
                  <span class="name">{s.name}</span>
                  <span class="addr">{s.user}@{s.host}{s.port === 22 ? "" : `:${s.port}`}</span>
                </button>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
    </div>
  </div>
</section>

{#snippet cardBody(c: Card)}
  <span class="ico"><Icon name={c.icon} size={22} /></span>
  <span class="ttl">{c.title}</span>
  <span class="txt">{c.text}</span>
  {#if c.key}<kbd>{c.key}</kbd>{/if}
{/snippet}

<style>
  .home {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    background: var(--bg);
    font-size: var(--text-m);
  }
  .scroll {
    display: flex;
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: var(--space-6);
  }
  /* Centred while there is room; scrolls from the top when there is not. */
  .inner {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
    width: 100%;
    max-width: 720px;
    margin: auto;
  }
  header {
    text-align: center;
  }
  h1 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 700;
  }
  header p {
    margin: var(--space-1) 0 0;
    color: var(--dim);
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
    gap: var(--space-3);
  }
  button {
    margin: 0;
    font: inherit;
    color: var(--fg);
    cursor: pointer;
  }
  .card {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1);
    min-height: 116px;
    padding: var(--space-4);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-l);
    background: var(--surface);
    text-align: left;
  }
  .card:hover {
    border-color: var(--focus);
    background: var(--hover);
  }
  .ico {
    display: flex;
    margin-bottom: var(--space-2);
    color: var(--accent-text);
  }
  .ttl {
    font-size: var(--text-l);
    font-weight: 600;
  }
  .txt {
    color: var(--dim);
    font-size: var(--text-s);
    line-height: 1.4;
  }
  kbd {
    padding: 1px var(--space-1);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    color: var(--dim);
    font: inherit;
    font-size: var(--text-xs);
  }
  .card kbd {
    position: absolute;
    top: var(--space-3);
    right: var(--space-3);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    margin-bottom: var(--space-1);
  }
  h2 {
    margin: 0;
    color: var(--dim);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  ul {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0 var(--space-3);
    margin: 0 calc(-1 * var(--space-2));
    padding: 0;
    list-style: none;
  }
  @media (max-width: 520px) {
    ul {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-2);
    border: 0;
    border-radius: var(--radius-m);
    background: none;
    text-align: left;
  }
  .row:hover {
    background: var(--hover);
  }
  .row :global(svg) {
    flex: none;
    color: var(--dim);
  }
  .name {
    flex: none;
    max-width: 60%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .addr {
    min-width: 0;
    overflow: hidden;
    color: var(--dim);
    font-family: var(--font-mono);
    font-size: var(--text-s);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .link {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1) var(--space-2);
    border: 0;
    border-radius: var(--radius-m);
    background: none;
    color: var(--dim);
    font-size: var(--text-s);
  }
  .link:hover {
    background: var(--hover);
    color: var(--fg);
  }
  .link kbd {
    margin-left: var(--space-1);
  }
</style>
