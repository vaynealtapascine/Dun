/**
 * The arithmetic behind swipe-to-delete, kept apart from the component so the
 * awkward parts — deciding a gesture is horizontal, not fighting the page's
 * own scrolling, where a half-swipe lands — can be tested without a browser.
 */

/** How far the row slides to uncover the button behind it. */
export const REVEAL_PX = 88;

/** Movement before a gesture is taken to mean anything. */
const SLOP_PX = 8;

/** Past this much of the reveal, letting go opens it rather than snapping back. */
const SETTLE_AT = 0.4;

export type Axis = "x" | "y";

/**
 * Which way a gesture is going, or `null` while it is too small to tell.
 *
 * Rows sit in a scrolling list, so a finger that is mostly travelling down the
 * screen belongs to the list, not to us — guessing early is what makes a list
 * feel like it is grabbing at you.
 */
export function axisOf(dx: number, dy: number): Axis | null {
  if (Math.abs(dx) < SLOP_PX && Math.abs(dy) < SLOP_PX) return null;
  return Math.abs(dx) > Math.abs(dy) ? "x" : "y";
}

/**
 * Where the row should sit for a drag of `dx` that began with it `from` pixels
 * open. Only leftward swiping does anything, and pulling past the button
 * resists rather than sliding the row off the screen.
 */
export function offsetFor(dx: number, from = 0): number {
  const wanted = from - dx;
  if (wanted <= 0) return 0;
  if (wanted <= REVEAL_PX) return wanted;
  // Rubber band: the extra travel arrives at a third of the speed.
  return REVEAL_PX + (wanted - REVEAL_PX) / 3;
}

/** Where the row lands when the finger lifts. */
export function settle(offset: number): number {
  return offset >= REVEAL_PX * SETTLE_AT ? REVEAL_PX : 0;
}

/**
 * Which row is currently open.
 *
 * Two open rows at once look like a list coming apart, and the second swipe
 * usually means "not that one, this one" — so opening a row shuts the last.
 */
const openRows = new Set<() => void>();

export const rows = {
  /** Call when a row settles open, passing its own close function. */
  opened(close: () => void) {
    for (const other of openRows) if (other !== close) other();
    openRows.clear();
    openRows.add(close);
  },
  /** Call when a row closes, however it closed. */
  closed(close: () => void) {
    openRows.delete(close);
  },
};
