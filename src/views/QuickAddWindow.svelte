<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { api } from "../lib/api/commands";
  import { app } from "../lib/stores/app.svelte";
  import QuickAdd from "../lib/components/QuickAdd.svelte";

  // The always-on-top bar opened by the global shortcut. Rust hides it on blur.
  let bar: QuickAdd | undefined = $state();
  let panel: HTMLDivElement | undefined = $state();

  const hide = () => {
    app.error = null;
    api.quickaddHide();
  };

  $effect(() => {
    app.start();
    const shown = listen("quickadd-shown", () => {
      bar?.clear();
      // Focus after the window has actually become visible.
      requestAnimationFrame(() => bar?.focus());
    });
    return () => {
      app.stop();
      shown.then((f) => f());
    };
  });

  // Keep the native window exactly as tall as the content (preview lines come and go).
  $effect(() => {
    if (!panel) return;
    const observer = new ResizeObserver(() => {
      if (panel) api.quickaddFit(Math.ceil(panel.getBoundingClientRect().height));
    });
    observer.observe(panel);
    return () => observer.disconnect();
  });
</script>

<div class="panel" bind:this={panel}>
  <QuickAdd
    bind:this={bar}
    autofocus
    placeholder="Laundry in 45m · Pay rent tomorrow 9am · Stretch every weekday at 3pm"
    onmore={(draft) => api.quickaddOpenForm(draft)}
    onadded={hide}
    onescape={hide}
  />
  {#if app.error}<p class="error" role="alert">{app.error}</p>{/if}
</div>

<style>
  :global(html),
  :global(body) {
    overflow: hidden;
  }
  .panel {
    padding: 0.55rem;
    background: var(--bg);
  }
  .error {
    margin: 0.3rem 0.5rem 0;
    font-size: 0.83rem;
    color: var(--ringing);
  }
</style>
