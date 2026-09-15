import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";

const target = document.getElementById("app");
if (!target) throw new Error("#app missing from index.html");

async function start() {
  // Plain-browser preview during development: fake the Rust side.
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    const { installMockBackend } = await import("./lib/dev/mock");
    installMockBackend();
  }
  mount(App, { target: target! });
}

start();
