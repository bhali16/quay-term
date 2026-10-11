import { mount } from "svelte";
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

const app = mount(App, { target: document.getElementById("app")! });

export default app;
