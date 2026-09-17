<script lang="ts">
  import type { Snippet } from "svelte";
  import { axisOf, offsetFor, rows, settle } from "../swipe";
  import Icon from "./Icon.svelte";

  let {
    label,
    ondelete,
    role = "group",
    children,
  }: {
    label: string;
    ondelete: () => void;
    /** What the row is to a screen reader; a list's rows are "listitem". */
    role?: "group" | "listitem";
    children: Snippet;
  } = $props();

  let offset = $state(0);
  let dragging = $state(false);

  let startX = 0;
  let startY = 0;
  let from = 0;
  let axis: "x" | "y" | null = null;

  const close = () => (offset = 0);

  function onpointerdown(e: PointerEvent) {
    // A stylus or mouse press on the delete button is that button's business.
    if ((e.target as HTMLElement).closest(".delete")) return;
    startX = e.clientX;
    startY = e.clientY;
    from = offset;
    axis = null;
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
      rows.opened(close);
    }
    offset = offsetFor(dx, from);
  }

  function onpointerup() {
    if (!dragging) return;
    dragging = false;
    offset = settle(offset);
    if (offset > 0) rows.opened(close);
    else rows.closed(close);
  }

  /**
   * An open row is a row waiting for an answer, so the first tap anywhere on it
   * is "no" rather than whatever it would normally have done.
   */
  function onclickcapture(e: MouseEvent) {
    if (offset > 0 && !(e.target as HTMLElement).closest(".delete")) {
      e.preventDefault();
      e.stopPropagation();
      close();
      rows.closed(close);
    }
  }

  function remove() {
    close();
    rows.closed(close);
    ondelete();
  }
</script>

<div class="swipe" style:--offset="{offset}px">
  <div class="behind">
    <button class="delete" onclick={remove} tabindex={offset === 0 ? -1 : 0} aria-label="Delete “{label}”">
      <Icon name="trash" size={16} /> Delete
    </button>
  </div>
  <div
    class="front"
    class:dragging
    {role}
    aria-label={label}
    {onpointerdown}
    {onpointermove}
    onpointerup={onpointerup}
    onpointercancel={onpointerup}
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
    background: var(--ringing);
    border-radius: var(--radius);
  }
  .delete {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0 0.85rem;
    border: none;
    background: none;
    color: #fff;
    font-weight: 600;
    font-size: 0.85rem;
    cursor: pointer;
  }
  .front {
    position: relative;
    transform: translateX(calc(-1 * var(--offset)));
    /* Vertical scrolling stays the list's; horizontal comes to us. */
    touch-action: pan-y;
  }
  .front:not(.dragging) {
    transition: transform 0.18s ease-out;
  }
</style>
