<script lang="ts">
  import { onDestroy, type Snippet } from "svelte";
  import { axisOf, commitAt, offsetFor, REVEAL_PX, rows, settle, Velocity } from "../swipe";
  import { haptic, ms } from "../motion";
  import Icon from "./Icon.svelte";

  let {
    label,
    onarchive,
    role = "group",
    children,
  }: {
    label: string;
    /** Resolves false if the item could not be archived, so the row returns. */
    onarchive: () => Promise<boolean> | void;
    /** What the row is to a screen reader; a list's rows are "listitem". */
    role?: "group" | "listitem";
    children: Snippet;
  } = $props();

  let offset = $state(0);
  let dragging = $state(false);
  let leaving = $state(false);
  let width = $state(0);

  let startX = 0;
  let startY = 0;
  let from = 0;
  let axis: "x" | "y" | null = null;
  let swiped = false;
  const velocity = new Velocity();

  /** Far enough that letting go archives the row outright. */
  const armed = $derived(width > 0 && offset >= commitAt(width));
  let wasArmed = false;
  $effect(() => {
    // One tick as the row crosses into, or back out of, a full swipe.
    if (armed === wasArmed) return;
    wasArmed = armed;
    haptic(armed ? 12 : 6);
  });

  const close = () => (offset = 0);
  onDestroy(() => {
    rows.closed(close);
    clearTimeout(wheelTimer);
  });

  function land(target: number | "archive") {
    if (target === "archive") {
      archive();
      return;
    }
    offset = target;
    if (offset > 0) rows.opened(close);
    else rows.closed(close);
  }

  function onpointerdown(e: PointerEvent) {
    if (!e.isPrimary || e.button !== 0 || leaving) return;
    swiped = false;
    const target = e.target as HTMLElement;
    // Text fields keep their own drag (selecting text), and the archive
    // button behind the row is that button's business.
    if (target.closest(".delete, input, textarea, select, [contenteditable]")) return;
    startX = e.clientX;
    startY = e.clientY;
    from = offset;
    axis = null;
    velocity.reset();
    velocity.add(e.clientX, e.timeStamp);
    dragging = true;
  }

  function onpointermove(e: PointerEvent) {
    if (!dragging) return;
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    if (axis === null) {
      axis = axisOf(dx, dy);
      if (axis === "y") {
        // The list is scrolling; this gesture was never ours.
        dragging = false;
        return;
      }
      if (axis === null) return;
      // Take the pointer so the row keeps following the finger even once it
      // wanders off the row's own edges.
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      // A mouse drag would otherwise select the text it passes over.
      if (e.pointerType === "mouse") getSelection()?.removeAllRanges();
      rows.opened(close);
      swiped = true;
    }
    velocity.add(e.clientX, e.timeStamp);
    offset = offsetFor(dx, from, width);
  }

  function onpointerup() {
    if (!dragging) return;
    dragging = false;
    if (axis !== "x") return;
    land(settle(offset, velocity.value, width));
  }

  /*
   * Two-finger swipes on a trackpad arrive as horizontal wheel events. They
   * open and archive a row just as a finger does, settling once they stop.
   */
  let wheeling = $state(false);
  let wheelTimer: ReturnType<typeof setTimeout> | undefined;
  let wheelVelocity = 0;

  function onwheel(e: WheelEvent) {
    if (leaving || Math.abs(e.deltaX) <= Math.abs(e.deltaY) * 1.5 || e.deltaX === 0) return;
    // Keep the gesture from also meaning "go back" in the webview.
    e.preventDefault();
    if (!wheeling) rows.opened(close);
    wheeling = true;
    const delta = e.deltaMode === 1 ? e.deltaX * 16 : e.deltaX;
    offset = offsetFor(-delta, offset, width);
    wheelVelocity = delta / 16;
    clearTimeout(wheelTimer);
    wheelTimer = setTimeout(() => {
      wheeling = false;
      land(settle(offset, wheelVelocity, width));
    }, 140);
  }

  function wheel(node: HTMLElement) {
    // Svelte's wheel handlers are passive; this one has to be able to say no.
    node.addEventListener("wheel", onwheel, { passive: false });
    return { destroy: () => node.removeEventListener("wheel", onwheel) };
  }

  /**
   * An open row is a row waiting for an answer, so the first tap anywhere on it
   * is "no" rather than whatever it would normally have done.
   */
  function onclickcapture(e: MouseEvent) {
    if (swiped) {
      swiped = false;
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    if (offset > 0 && !(e.target as HTMLElement).closest(".delete")) {
      e.preventDefault();
      e.stopPropagation();
      close();
      rows.closed(close);
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && offset > 0) {
      e.stopPropagation();
      close();
      rows.closed(close);
    }
  }

  /** The row carries on off the edge, then the list closes the gap. */
  function archive() {
    leaving = true;
    rows.closed(close);
    offset = width || REVEAL_PX;
    setTimeout(async () => {
      if ((await onarchive()) === false) {
        // The item is still here, so the row comes back.
        leaving = false;
        offset = 0;
      }
    }, ms(180));
  }
</script>

<div class="swipe" class:open={offset > 0} class:armed class:leaving style:--offset="{offset}px" style:--reveal="{Math.min(1, offset / REVEAL_PX)}"
  bind:clientWidth={width} use:wheel>
  <div class="behind" inert={offset === 0}>
    <button class="delete" onclick={archive} tabindex={offset === 0 ? -1 : 0} aria-label="Archive “{label}”">
      <span class="delete-label"><Icon name="archive" size={16} /> Archive</span>
    </button>
  </div>
  <div
    class="front"
    class:dragging={dragging || wheeling}
    {role}
    aria-label={label}
    {onpointerdown}
    {onpointermove}
    {onpointerup}
    {onkeydown}
    onpointercancel={() => { dragging = false; swiped = false; close(); rows.closed(close); }}
    onclickcapture={onclickcapture}
  >
    {@render children()}
  </div>
</div>

<style>
  .swipe {
    position: relative;
    border-radius: var(--radius);
    overflow: hidden;
  }
  .behind {
    position: absolute;
    inset: 0;
    display: flex;
    justify-content: flex-end;
    background: var(--accent);
    border-radius: var(--radius);
    opacity: 0;
    transition: background-color var(--dur) var(--ease-out), opacity var(--dur-fast) ease-out;
  }
  .open .behind { opacity: 1; }
  .armed .behind, .leaving .behind { background: color-mix(in srgb, var(--accent) 78%, var(--fg)); }
  .delete {
    display: flex;
    align-items: center;
    /* The button grows with the swipe, so its label rides the row's edge. */
    width: max(88px, var(--offset));
    padding: 0 0.85rem;
    border: none;
    background: none;
    color: var(--accent-fg);
    font-weight: 600;
    font-size: 0.85rem;
    cursor: pointer;
  }
  .delete-label {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    opacity: var(--reveal);
    transform: scale(calc(0.7 + 0.3 * var(--reveal)));
    transform-origin: left center;
    transition: transform var(--dur) var(--ease-out);
  }
  .armed .delete-label { transform: scale(1.12); font-weight: 700; }
  .front {
    position: relative;
    transform: translateX(calc(-1 * var(--offset)));
    /* Vertical scrolling stays the list's; horizontal comes to us. */
    touch-action: pan-y;
  }
  .front.dragging { user-select: none; }
  .front:not(.dragging) {
    transition: transform var(--dur-slow) var(--ease-out);
  }
  .leaving .front { transition-duration: 180ms; transition-timing-function: var(--ease-in-out); }
</style>
