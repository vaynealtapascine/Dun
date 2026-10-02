/**
 * Small pointer conveniences shared by several controls.
 */

/**
 * Lets an ordinary mouse wheel scroll a horizontal strip (presets, tags).
 * Trackpads already scroll sideways, and a strip that can't scroll any
 * further hands the wheel back to the page.
 */
export function wheelScrollsX(node: HTMLElement) {
  function onwheel(e: WheelEvent) {
    if (e.ctrlKey || Math.abs(e.deltaX) >= Math.abs(e.deltaY)) return;
    const max = node.scrollWidth - node.clientWidth;
    if (max <= 0) return;
    const atStart = node.scrollLeft <= 0 && e.deltaY < 0;
    const atEnd = node.scrollLeft >= max - 1 && e.deltaY > 0;
    if (atStart || atEnd) return;
    e.preventDefault();
    node.scrollLeft += e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;
  }
  node.addEventListener("wheel", onwheel, { passive: false });
  return { destroy: () => node.removeEventListener("wheel", onwheel) };
}

/**
 * Steps a two- or three-digit field by `delta`, wrapping within 0…max, and
 * returns it padded to two digits as the fields show it.
 */
export function stepField(value: string, delta: number, max: number): string {
  const n = /^\d+$/.test(value.trim()) ? Number(value.trim()) : 0;
  const span = max + 1;
  return String((((n + delta) % span) + span) % span).padStart(2, "0");
}
