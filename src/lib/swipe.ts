/**
 * The arithmetic behind swipe-to-archive, kept apart from the component so the
 * awkward parts — deciding a gesture is horizontal, not fighting the page's
 * own scrolling, where a half-swipe or a flick lands — can be tested without a
 * browser.
 */

/** How far the row slides to uncover the button behind it. */
export const REVEAL_PX = 88;

/** Movement before a gesture is taken to mean anything. */
const SLOP_PX = 8;

/** Past this much of the reveal, letting go opens it rather than snapping back. */
const SETTLE_AT = 0.4;

/** A flick this fast (px per ms) decides the outcome on its own. */
const FLICK = 0.45;

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
 * How far a row must travel before letting go archives it outright, without
 * the second tap on the button. Long enough that it never happens by accident
 * on a narrow phone, short enough to reach with a thumb on a wide one.
 */
export function commitAt(width: number): number {
  return Math.max(REVEAL_PX * 2, width * 0.55);
}

/**
 * Where the row should sit for a drag of `dx` that began with it `from` pixels
 * open. Only leftward swiping does anything. Within a row's width the row
 * follows the finger exactly; beyond that it resists, since nothing is there.
 */
export function offsetFor(dx: number, from = 0, width = Infinity): number {
  const wanted = from - dx;
  if (wanted <= 0) return 0;
  if (wanted <= width) return wanted;
  // Rubber band: the extra travel arrives at a third of the speed.
  return width + (wanted - width) / 3;
}

/**
 * Where the row lands when the finger lifts: shut, open on the button, or
 * archived. `velocity` is in px per ms, positive while the row is opening.
 */
export function settle(offset: number, velocity = 0, width = Infinity): number | "archive" {
  if (offset >= commitAt(width)) return "archive";
  if (velocity >= FLICK) return REVEAL_PX;
  if (velocity <= -FLICK) return 0;
  return offset >= REVEAL_PX * SETTLE_AT ? REVEAL_PX : 0;
}

/**
 * Recent pointer positions, enough to tell a flick from a slow drag. Only the
 * last ~80ms count: a finger that stopped before lifting meant to stop.
 */
export class Velocity {
  #samples: { x: number; t: number }[] = [];

  add(x: number, t: number) {
    this.#samples.push({ x, t });
    while (this.#samples.length > 2 && t - this.#samples[0]!.t > 80) this.#samples.shift();
  }

  /** px per ms; positive when moving left (opening a row). */
  get value(): number {
    const first = this.#samples[0];
    const last = this.#samples.at(-1);
    if (!first || !last || last.t - first.t < 8) return 0;
    return (first.x - last.x) / (last.t - first.t);
  }

  reset() {
    this.#samples = [];
  }
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
