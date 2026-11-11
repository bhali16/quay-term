<script lang="ts">
  import Icon from "./Icon.svelte";
  import { toasts } from "./toastStore.svelte";

  /** Toasts whose "Details" are open. */
  let open = $state<number[]>([]);
  const toggle = (id: number) => (open = open.includes(id) ? open.filter((x) => x !== id) : [...open, id]);
</script>

<!-- Timers stop while the pointer or the keyboard is on the stack, so nothing leaves mid-read. -->
<div
  class="toasts"
  role="region"
  aria-label="Notifications"
  data-modal-keep
  onmouseenter={() => toasts.pause()}
  onmouseleave={() => toasts.resume()}
  onfocusin={() => toasts.pause()}
  onfocusout={() => toasts.resume()}
>
  {#each toasts.list as t (t.id)}
    <div class="toast {t.kind}" role={t.kind === "error" ? "alert" : "status"}>
      <span class="ticon"><Icon name={t.kind === "error" ? "alert" : t.kind === "success" ? "check" : "info"} size={16} /></span>
      <div class="body">
        <span class="msg">{t.text}</span>
        {#if t.detail}
          <button class="more" aria-expanded={open.includes(t.id)} onclick={() => toggle(t.id)}>{open.includes(t.id) ? "Hide details" : "Details"}</button>
          {#if open.includes(t.id)}<pre class="detail">{t.detail}</pre>{/if}
        {/if}
      </div>
      {#if t.action}<button class="tact" onclick={() => toasts.act(t.id)}>{t.action.label}</button>{/if}
      <button class="x" aria-label="Dismiss" title="Dismiss" onclick={() => toasts.close(t.id)}><Icon name="x" size={14} /></button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    z-index: 40;
    display: flex;
    flex-direction: column;
    gap: 8px;
    pointer-events: none;
  }
  .toast {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    width: max-content;
    max-width: min(360px, calc(100vw - 32px));
    margin-left: auto;
    box-sizing: border-box;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-left: 3px solid var(--accent);
    border-radius: 8px;
    background: var(--surface);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
    font-size: 13px;
    pointer-events: auto;
    animation: toast-in 0.15s ease-out;
  }
  .toast.success {
    border-left-color: var(--green);
  }
  .toast.error {
    border-left-color: var(--red);
  }
  .ticon {
    display: flex;
    flex: none;
    margin-top: 3px;
    color: var(--accent-text);
  }
  .toast.success .ticon {
    color: var(--green-text);
  }
  .toast.error .ticon {
    color: var(--red-text);
  }
  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding-top: 3px;
  }
  .msg {
    overflow-wrap: anywhere;
    line-height: 1.4;
  }
  button {
    all: unset;
    box-sizing: border-box;
    flex: none;
    border-radius: 5px;
    cursor: pointer;
  }
  .x {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    color: var(--dim);
  }
  .x:hover {
    color: var(--fg);
    background: var(--hover);
  }
  .tact {
    min-height: 24px;
    padding: 0 9px;
    border: 1px solid var(--border-strong);
    font-weight: 500;
    line-height: 22px;
    color: var(--fg);
  }
  .tact:hover {
    background: var(--hover);
  }
  .more {
    font-size: 12px;
    color: var(--dim);
    text-decoration: underline;
  }
  .more:hover {
    color: var(--fg);
  }
  .detail {
    margin: 0;
    max-width: 100%;
    max-height: 120px;
    overflow: auto;
    padding: 6px 8px;
    box-sizing: border-box;
    border-radius: 5px;
    background: var(--bg-deep);
    color: var(--dim);
    font-family: var(--font-mono);
    font-size: 11px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
  }
  button:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }
  @media (prefers-reduced-motion: reduce) {
    .toast {
      animation: none;
    }
  }
  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }
</style>
