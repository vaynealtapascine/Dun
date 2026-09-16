import { invoke } from "@tauri-apps/api/core";

/** How often the phone checks in while someone is looking at it. */
export const SYNC_EVERY_MS = 30_000;

/**
 * Keeps the phone in step while it is on screen.
 *
 * Alarms already sync before every alert, but between them the phone would
 * show a stale list — an item added on the PC, or one already done there,
 * wouldn't appear until something rang. So sync when the app comes back to the
 * foreground and every half minute while it stays there, and stop as soon as
 * it goes away: there is no reason to keep waking the radio in the background,
 * where the alarm path takes over.
 *
 * Failures are ignored on purpose. Being out of reach of the PC is ordinary,
 * and the next tick tries again.
 */
export function startForegroundSync({
  sync = () => invoke("sync_now"),
  visible = () => document.visibilityState === "visible",
  watch = (onChange) => {
    document.addEventListener("visibilitychange", onChange);
    return () => document.removeEventListener("visibilitychange", onChange);
  },
}: {
  sync?: () => Promise<unknown>;
  visible?: () => boolean;
  /** Calls back when the app may have come or gone; returns an unsubscribe. */
  watch?: (onChange: () => void) => () => void;
} = {}): () => void {
  let timer: ReturnType<typeof setInterval> | undefined;
  let running = false;

  const tick = async () => {
    // One at a time: a slow check-in must not stack up behind itself.
    if (running || !visible()) return;
    running = true;
    try {
      await sync();
    } catch {
      // Offline, or the PC is off. The next tick will try again.
    } finally {
      running = false;
    }
  };

  const resume = () => {
    if (timer !== undefined) return;
    timer = setInterval(tick, SYNC_EVERY_MS);
    void tick();
  };

  const pause = () => {
    clearInterval(timer);
    timer = undefined;
  };

  const onVisibility = () => (visible() ? resume() : pause());
  const unwatch = watch(onVisibility);
  onVisibility();

  return () => {
    unwatch();
    pause();
  };
}
