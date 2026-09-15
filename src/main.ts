import { mount } from "svelte";
import App from "./App.svelte";
import QuickAddWindow from "./views/QuickAddWindow.svelte";
import "./app.css";

const target = document.getElementById("app");
if (!target) throw new Error("#app missing from index.html");

// Decide before the dev mock runs: it installs its own __TAURI_INTERNALS__.
const inTauri = "__TAURI_INTERNALS__" in window;

/** Every window loads the same page; its label picks the root view. */
async function windowLabel(): Promise<string> {
  if (inTauri) {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    return getCurrentWindow().label;
  }
  // Browser preview: http://localhost:1420/?window=quickadd
  return new URLSearchParams(location.search).get("window") ?? "main";
}

async function start() {
  // Plain-browser preview during development: fake the Rust side.
  if (import.meta.env.DEV && !inTauri) {
    const { installMockBackend } = await import("./lib/dev/mock");
    installMockBackend();
  }
  const label = await windowLabel();
  mount(label === "quickadd" ? QuickAddWindow : App, { target: target! });
}

start();
