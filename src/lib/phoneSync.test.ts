import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SYNC_EVERY_MS, startForegroundSync } from "./phoneSync";

describe("foreground sync", () => {
  /** Stands in for the tab going away and coming back. */
  let visible = true;
  let listeners: (() => void)[] = [];
  const watch = (onChange: () => void) => {
    listeners.push(onChange);
    return () => (listeners = listeners.filter((l) => l !== onChange));
  };
  const show = (on: boolean) => {
    visible = on;
    listeners.forEach((l) => l());
  };
  const start = (sync: () => Promise<unknown>) => startForegroundSync({ sync, visible: () => visible, watch });

  beforeEach(() => {
    visible = true;
    listeners = [];
    vi.useFakeTimers();
  });
  afterEach(() => vi.useRealTimers());

  it("syncs on start and then every half minute", async () => {
    const sync = vi.fn().mockResolvedValue(undefined);
    const stop = start(sync);

    expect(sync).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(SYNC_EVERY_MS * 2);
    expect(sync).toHaveBeenCalledTimes(3);

    stop();
    await vi.advanceTimersByTimeAsync(SYNC_EVERY_MS * 2);
    expect(sync).toHaveBeenCalledTimes(3);
  });

  it("stops while the app is away and catches up when it returns", async () => {
    const sync = vi.fn().mockResolvedValue(undefined);
    const stop = start(sync);
    expect(sync).toHaveBeenCalledTimes(1);

    show(false);
    await vi.advanceTimersByTimeAsync(SYNC_EVERY_MS * 3);
    expect(sync).toHaveBeenCalledTimes(1);

    // Back on screen: sync at once rather than waiting out the interval.
    show(true);
    expect(sync).toHaveBeenCalledTimes(2);
    stop();
  });

  it("doesn't stack up when a check-in outlasts the interval", async () => {
    let finish = () => {};
    const sync = vi.fn(() => new Promise<void>((resolve) => (finish = resolve)));
    const stop = start(sync);

    await vi.advanceTimersByTimeAsync(SYNC_EVERY_MS * 3);
    expect(sync).toHaveBeenCalledTimes(1);

    finish();
    await vi.advanceTimersByTimeAsync(SYNC_EVERY_MS);
    expect(sync).toHaveBeenCalledTimes(2);
    stop();
  });

  it("keeps going after a failure", async () => {
    const sync = vi.fn().mockRejectedValue(new Error("couldn't reach the PC"));
    const stop = start(sync);

    await vi.advanceTimersByTimeAsync(SYNC_EVERY_MS);
    expect(sync).toHaveBeenCalledTimes(2);
    stop();
  });
});
