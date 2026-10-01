<script lang="ts">
  import type { ItemView } from "../api/types";
  import { api } from "../api/commands";
  import { app } from "../stores/app.svelte";
  import Icon from "./Icon.svelte";

  let { item, onedit }: { item: ItemView; onedit: (item: ItemView) => void } = $props();
  let dialog: HTMLDialogElement;
  let x = $state(0);
  let y = $state(0);
  const due = $derived(item.status.kind === "due");
  const occurrence = $derived(item.status.kind === "due" ? item.status.occurrence : null);

  export function openAt(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    x = Math.max(8, Math.min(e.clientX || rect.left, window.innerWidth - 232));
    y = Math.max(8, Math.min(e.clientY || rect.bottom, window.innerHeight - 340));
    dialog.showModal();
  }

  function act(fn: () => void) {
    dialog.close();
    fn();
  }

  function navigate(e: KeyboardEvent) {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) return;
    e.preventDefault();
    const buttons = [...dialog.querySelectorAll<HTMLButtonElement>("button")];
    const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const next = e.key === "Home" ? 0 : e.key === "End" ? buttons.length - 1 :
      (at + (e.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length;
    buttons[next]?.focus();
  }
</script>

<button class="icon-btn" aria-label="Actions for {item.title}" title="More actions" aria-haspopup="dialog" onclick={openAt}>
  <Icon name="more" />
</button>
<dialog bind:this={dialog} aria-label="Actions for {item.title}" style:left="{x}px" style:top="{y}px"
  onclick={(e) => { if (e.target === dialog) dialog.close(); }} onkeydown={navigate}>
  <div class="menu">
    <button onclick={() => act(() => onedit(item))}>Edit</button>
    {#if item.kind === "timer"}
      {#if item.timer.state === "running" && !due}
        <button onclick={() => act(() => { void app.run(() => api.timer(item.id, "pause")); })}>Pause</button>
      {:else}
        <button onclick={() => act(() => { void app.run(() => api.timer(item.id, item.timer.state === "paused" ? "resume" : "start")); })}>
          {item.timer.state === "paused" ? "Resume" : due ? "Restart" : "Start"}
        </button>
      {/if}
      {#if item.timer.state !== "idle"}
        <button onclick={() => act(() => { void app.run(() => api.timer(item.id, "reset")); })}>Reset</button>
      {/if}
    {/if}
    {#if due || item.kind !== "timer"}
      <button onclick={() => act(() => { void app.run(() => api.done(item.id, occurrence)); })}>Mark done</button>
    {/if}
    {#if due}
      <button onclick={() => act(() => { void app.run(() => api.snooze(item.id, 5, occurrence)); })}>Snooze 5 minutes</button>
      <button onclick={() => act(() => { void app.run(() => api.snooze(item.id, 15, occurrence)); })}>Snooze 15 minutes</button>
    {/if}
    <button onclick={() => act(() => { void app.archive(item); })}>Archive</button>
  </div>
</dialog>

<style>
  dialog { position: fixed; margin: 0; padding: 0; width: 224px; max-height: calc(100dvh - 16px); overflow-y: auto;
    border: 1px solid var(--border); border-radius: var(--radius); background: var(--bg-raised); color: var(--fg); box-shadow: var(--shadow); }
  dialog::backdrop { background: transparent; }
  .menu { display: grid; padding: 0.3rem; }
  .menu button { text-align: left; min-height: 44px; padding: 0.5rem 0.75rem; border: 0; border-radius: var(--radius-sm); background: transparent; cursor: pointer; }
  .menu button:hover, .menu button:focus-visible { background: var(--bg-sunken); }
</style>
