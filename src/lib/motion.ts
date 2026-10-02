/**
 * Dun's motion vocabulary, shared by Svelte transitions and gesture code.
 *
 * Motion here only ever explains a change the person caused or needs to see:
 * where a row went, which tab is now showing, that a sheet can be dragged.
 * Nothing loops or decorates, and everything collapses to an instant change
 * when the system asks for reduced motion.
 */
import { cubicOut } from "svelte/easing";
import { flip } from "svelte/animate";
import { fly, slide, type TransitionConfig } from "svelte/transition";

/** Matches `--ease-out` in app.css: quick to respond, soft to land. */
export const easeOut = (t: number) => 1 - Math.pow(1 - t, 4);

export const DURATION = { fast: 120, base: 200, slow: 280 } as const;

const query = typeof matchMedia === "function" ? matchMedia("(prefers-reduced-motion: reduce)") : null;

export function reducedMotion(): boolean {
  return query?.matches ?? false;
}

/** A duration that becomes zero under reduced motion. */
export function ms(duration: number): number {
  return reducedMotion() ? 0 : duration;
}

/** A row joining a list: it rises a few pixels into place. */
export function rowIn(node: Element): TransitionConfig {
  return fly(node, { y: 8, duration: ms(DURATION.base), easing: easeOut, opacity: 0 });
}

/**
 * A row leaving a list: it fades while the space it took closes, so the rows
 * below slide up instead of jumping.
 */
export function rowOut(node: Element): TransitionConfig {
  const collapse = slide(node, { duration: ms(DURATION.slow), easing: cubicOut });
  const css = collapse.css;
  // The list's gap closes too, or it would snap shut once the row is gone.
  const parent = node.parentElement;
  const gap = parent ? parseFloat(getComputedStyle(parent).rowGap) || 0 : 0;
  const last = node.nextElementSibling === null;
  return {
    ...collapse,
    css: (t, u) =>
      `${css?.(t, u) ?? ""}opacity: ${Math.min(1, t * 1.6)};` +
      (gap ? `margin-${last ? "top" : "bottom"}: ${-gap * u}px;` : ""),
  };
}

/** Rows that change place (a timer finishing jumps to the top) glide there. */
export function reorder(node: Element, positions: { from: DOMRect; to: DOMRect }) {
  return flip(node, positions, { duration: (d) => ms(Math.min(360, 160 + Math.sqrt(d) * 8)), easing: easeOut });
}

/** Content arriving from one side, as when the next tab slides in. */
export function enterFrom(node: Element, { x = 0, y = 0 }: { x?: number; y?: number }): TransitionConfig {
  return fly(node, { x, y, duration: ms(DURATION.base), easing: easeOut, opacity: 0 });
}

/** Content revealed beneath a control, such as the search tools. */
export function reveal(node: Element): TransitionConfig {
  return slide(node, { duration: ms(DURATION.base), easing: easeOut });
}

/** A short physical tick, where the device can give one (Android). */
export function haptic(pattern: number | number[] = 8) {
  try {
    navigator.vibrate?.(pattern);
  } catch {
    // Some webviews throw without a user gesture; a missing tick is harmless.
  }
}
