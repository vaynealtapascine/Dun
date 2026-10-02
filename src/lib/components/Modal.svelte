<script lang="ts">
  import type { Snippet } from "svelte";
  import { ms } from "../motion";
  import { Velocity } from "../swipe";
  import Icon from "./Icon.svelte";

  let {
    open = $bindable(false),
    title,
    children,
    footer,
  }: { open?: boolean; title: string; children: Snippet; footer?: Snippet } = $props();

  let dialog: HTMLDialogElement | undefined = $state();
  let sheet: HTMLDivElement | undefined = $state();
  /** The content stays until the closing animation has played. */
  let shown = $state(false);
  const closing = $derived(shown && !open);
  let closeTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    if (!dialog) return;
    clearTimeout(closeTimer);
    if (open) {
      shown = true;
      if (!dialog.open) {
        drag = 0;
        dialog.showModal();
      }
    } else if (dialog.open) {
      closeTimer = setTimeout(() => {
        dialog?.close();
        shown = false;
        drag = 0;
      }, ms(180));
    } else {
      shown = false;
    }
  });

  /*
   * On a phone the dialog is a sheet along the bottom edge, and pulling its
   * header down puts it away, the way sheets behave elsewhere on Android.
   */
  let drag = $state(0);
  let dragging = $state(false);
  let startY = 0;
  const velocity = new Velocity();

  function sheetMode() {
    return matchMedia("(max-width: 540px)").matches;
  }

  function onpointerdown(e: PointerEvent) {
    if (!e.isPrimary || e.button !== 0 || !sheetMode()) return;
    if ((e.target as HTMLElement).closest("button")) return;
    startY = e.clientY;
    velocity.reset();
    velocity.add(-e.clientY, e.timeStamp);
    dragging = true;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onpointermove(e: PointerEvent) {
    if (!dragging) return;
    const dy = e.clientY - startY;
    // Downwards follows the finger; upwards only gives a little.
    drag = dy > 0 ? dy : -Math.sqrt(-dy) * 2;
    velocity.add(-e.clientY, e.timeStamp);
  }

  function onpointerup() {
    if (!dragging) return;
    dragging = false;
    const height = sheet?.offsetHeight ?? 400;
    if (drag > height * 0.3 || (drag > 24 && velocity.value > 0.5)) open = false;
    else drag = 0;
  }
</script>

<dialog
  bind:this={dialog}
  class:closing
  aria-label={title}
  onclose={() => {
    open = false;
    shown = false;
  }}
  onkeydown={(e) => {
    // Chromium only fires `cancel` for Escape after certain user activation,
    // so don't rely on it.
    if (e.key === "Escape" && !e.defaultPrevented) {
      e.preventDefault();
      open = false;
    }
  }}
  onclick={(e) => {
    // Click on the backdrop closes.
    if (e.target === dialog) open = false;
  }}
>
  {#if shown}
    <div class="sheet" class:dragging bind:this={sheet} style:--drag="{Math.max(drag, -12)}px">
      <!-- Dragging the header down is a touch shortcut; Close and Escape do the same. -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <header {onpointerdown} {onpointermove} {onpointerup} onpointercancel={() => { dragging = false; drag = 0; }}>
        <span class="handle" aria-hidden="true"></span>
        <h2>{title}</h2>
        <button class="icon-btn" onclick={() => (open = false)} aria-label="Close">
          <Icon name="close" />
        </button>
      </header>
      <div class="body">{@render children()}</div>
      {#if footer}
        <footer>{@render footer()}</footer>
      {/if}
    </div>
  {/if}
</dialog>

<style>
  dialog {
    padding: 0;
    border: none;
    background: transparent;
    width: min(34rem, calc(100vw - 1.5rem));
    max-height: calc(100dvh - 1.5rem);
    color: var(--fg);
    overflow: visible;
  }
  dialog::backdrop {
    background: rgba(0, 0, 0, 0.35);
    animation: backdrop-in var(--dur) ease-out;
  }
  dialog.closing::backdrop {
    opacity: 0;
    transition: opacity 180ms ease-in;
  }
  .sheet {
    display: flex;
    flex-direction: column;
    max-height: calc(100dvh - 1.5rem);
    background: var(--bg);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    border: 1px solid var(--border);
    animation: sheet-in var(--dur) var(--ease-out);
  }
  .closing .sheet {
    animation: sheet-out 180ms ease-in forwards;
  }
  header {
    flex-shrink: 0;
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem 0.75rem 0.25rem 1.1rem;
  }
  .handle { display: none; }
  h2 {
    margin: 0;
    font-size: 1.1rem;
  }
  .body {
    padding: 0.5rem 1.1rem 1rem;
    overflow-y: auto;
    overscroll-behavior: contain;
    display: grid;
    gap: 0.9rem;
  }
  footer {
    flex-shrink: 0;
    flex-wrap: wrap;
    display: flex;
    gap: 0.5rem;
    justify-content: flex-end;
    padding: 0.75rem 1.1rem;
    border-top: 1px solid var(--border);
  }
  @keyframes backdrop-in { from { opacity: 0; } }
  @keyframes sheet-in { from { opacity: 0; transform: translateY(8px) scale(0.97); } }
  @keyframes sheet-out { to { opacity: 0; transform: translateY(6px) scale(0.98); } }

  /* Phones: a sheet along the bottom edge, within reach of a thumb. */
  @media (max-width: 540px) {
    dialog {
      width: 100vw;
      max-width: 100vw;
      max-height: calc(100dvh - 2.5rem);
      margin: auto 0 0;
    }
    .sheet {
      max-height: calc(100dvh - 2.5rem);
      padding-bottom: env(safe-area-inset-bottom);
      border-width: 1px 0 0;
      border-radius: 14px 14px 0 0;
      transform: translateY(var(--drag));
      transition: transform var(--dur-slow) var(--ease-out);
      animation-name: sheet-up;
      animation-duration: var(--dur-slow);
    }
    .sheet.dragging { transition: none; }
    .closing .sheet {
      animation: none;
      transform: translateY(100%);
      transition: transform 200ms var(--ease-in-out);
    }
    header {
      padding-top: 1.1rem;
      touch-action: none;
      cursor: grab;
    }
    .dragging header { cursor: grabbing; }
    .handle {
      display: block;
      position: absolute;
      top: 0.45rem;
      left: 50%;
      width: 2.25rem;
      height: 0.25rem;
      margin-left: -1.125rem;
      border-radius: 999px;
      background: var(--fg-faint);
      opacity: 0.6;
    }
    .body { padding-inline: 0.85rem; }
    footer { padding: 0.65rem 0.85rem; }
  }
  @keyframes sheet-up { from { transform: translateY(100%); } }
</style>
