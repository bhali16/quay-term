import { mount } from "svelte";
import "@fontsource/dm-sans/400.css";
import "@fontsource/dm-sans/500.css";
import "@fontsource/dm-sans/700.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/600.css";
import "./app.css";
import App from "./App.svelte";

// Hostnames, usernames, paths and tags are case-sensitive and never prose: stop
// the webview from capitalizing, "correcting" or spell-checking any text field.
document.addEventListener("focusin", (e) => {
  const el = e.target;
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
    el.setAttribute("autocapitalize", "off");
    el.setAttribute("autocorrect", "off");
    el.spellcheck = false;
  }
});

// xterm measures glyphs when it opens, so have the bundled fonts ready first.
// They are local files, so this is instant; never block startup on a failure.
await Promise.all([document.fonts.load("14px 'JetBrains Mono'"), document.fonts.load("14px 'DM Sans'")]).catch(() => {});

const app = mount(App, { target: document.getElementById("app")! });

export default app;
