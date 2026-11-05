import { mount } from "svelte";
import "@fontsource/dm-sans/400.css";
import "@fontsource/dm-sans/500.css";
import "@fontsource/dm-sans/700.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/600.css";
import "./setup.css";
import App from "./App.svelte";

export default mount(App, { target: document.getElementById("app")! });
