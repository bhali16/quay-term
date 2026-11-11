/**
 * What a dialog or menu needs to be usable without a mouse: focus goes in, stays in, and
 * comes back to where it was. Used as Svelte actions on the dialog and menu elements.
 */
import { step, typeAhead } from "./roving";

/** The last few elements that had focus, newest last, to return to when an overlay closes. */
const recent: HTMLElement[] = [];
let fallback: (() => void) | null = null;

if (typeof document !== "undefined") {
  document.addEventListener(
    "focusin",
    (e) => {
      if (!(e.target instanceof HTMLElement)) return;
      recent.push(e.target);
      if (recent.length > 16) recent.shift();
    },
    true,
  );
}

/** Where focus goes when the element that had it is gone (the app passes "the terminal"). */
export function setFocusFallback(fn: () => void) {
  fallback = fn;
}

/** Give focus back after `node` closed, unless something else has already taken it. */
function returnFocus(node: HTMLElement, before: HTMLElement[]) {
  // A frame later: close handlers that move focus themselves (rename a tab, back to the terminal) go first.
  requestAnimationFrame(() => {
    const now = document.activeElement;
    if (now instanceof HTMLElement && now !== document.body && now.isConnected && !node.contains(now)) return;
    const target = [...before].reverse().find((el) => el.isConnected && !el.closest("[inert]") && el.getClientRects().length > 0);
    if (target) target.focus();
    else fallback?.();
  });
}

const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

function focusables(root: HTMLElement) {
  return [...root.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.getClientRects().length > 0);
}

// Several dialogs can be open at once (a host-key prompt over the session form), so count holds.
const held = new Map<Element, number>();
function hold(el: Element) {
  held.set(el, (held.get(el) ?? 0) + 1);
  (el as HTMLElement).inert = true;
}
function release(el: Element) {
  const n = (held.get(el) ?? 1) - 1;
  if (n > 0) return void held.set(el, n);
  held.delete(el);
  (el as HTMLElement).inert = false;
}

/**
 * Svelte action for a dialog: marks it modal, makes the rest of the window inert (so Tab and
 * screen readers stay inside), wraps Tab at both ends and returns focus on close.
 * Elements marked `data-modal-keep` (the toasts) stay usable.
 */
export function modal(node: HTMLElement) {
  const before = recent.filter((el) => !node.contains(el));
  if (!node.hasAttribute("role")) node.setAttribute("role", "dialog");
  node.setAttribute("aria-modal", "true");
  if (!node.hasAttribute("tabindex")) node.tabIndex = -1;

  const outside: Element[] = [];
  for (let el: Element = node.closest(".dlg-backdrop") ?? node; el.parentElement && el !== document.body; el = el.parentElement) {
    for (const sib of el.parentElement.children) {
      if (sib === el || sib.hasAttribute("data-modal-keep") || sib.tagName === "SCRIPT" || sib.tagName === "STYLE") continue;
      hold(sib);
      outside.push(sib);
    }
  }
  if (!node.contains(document.activeElement)) (focusables(node)[0] ?? node).focus();

  function onKey(e: KeyboardEvent) {
    if (e.key !== "Tab") return;
    const items = focusables(node);
    if (items.length === 0) return e.preventDefault();
    const first = items[0];
    const last = items[items.length - 1];
    const at = document.activeElement;
    if (e.shiftKey ? at === first || at === node : at === last) {
      e.preventDefault();
      (e.shiftKey ? last : first).focus();
    }
  }
  node.addEventListener("keydown", onKey);

  return {
    destroy() {
      node.removeEventListener("keydown", onKey);
      outside.forEach(release);
      returnFocus(node, before);
    },
  };
}

/**
 * Svelte action for a `role="menu"` element: focus starts on the checked or first item, arrow
 * keys, Home, End and typing a letter move it, Tab stays inside, the menu is nudged back
 * inside the window, and focus returns on close. Closing (Escape, click away) is the caller's.
 */
export function menu(node: HTMLElement) {
  const before = recent.filter((el) => !node.contains(el));
  const items = () => [...node.querySelectorAll<HTMLElement>('[role^="menuitem"]:not([disabled])')];

  const pad = 6;
  const r = node.getBoundingClientRect();
  const w = document.documentElement.clientWidth;
  const h = document.documentElement.clientHeight;
  const dx = Math.max(pad - r.left, Math.min(0, w - pad - r.right));
  const dy = Math.max(pad - r.top, Math.min(0, h - pad - r.bottom));
  if (dx || dy) node.style.translate = `${dx}px ${dy}px`;

  const start = items();
  (start.find((el) => el.getAttribute("aria-checked") === "true") ?? start[0] ?? node).focus();

  function onKey(e: KeyboardEvent) {
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    const list = items();
    const i = list.indexOf(document.activeElement as HTMLElement);
    let to = step(e.key, i < 0 && e.key === "ArrowUp" ? 0 : i, list.length, true);
    if (to === null && e.key.length === 1 && e.key !== " ") {
      const hit = typeAhead(list.map((el) => el.textContent ?? ""), i, e.key);
      if (hit >= 0) to = hit;
    }
    if (to === null && e.key !== "Tab") return;
    e.preventDefault();
    if (to !== null) list[to].focus();
  }
  node.addEventListener("keydown", onKey);

  return {
    destroy() {
      node.removeEventListener("keydown", onKey);
      returnFocus(node, before);
    },
  };
}
