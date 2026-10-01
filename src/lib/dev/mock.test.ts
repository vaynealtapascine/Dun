import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import { clearMocks } from "@tauri-apps/api/mocks";
import { api } from "../api/commands";
import type { ItemDraft, ItemView, LocalSettings, Snapshot } from "../api/types";
import { installMockBackend } from "./mock";

const start = Date.parse("2026-10-01T12:00:00Z");
let stop: () => void;

const draft = (durationMs = 5000, extra: Partial<ItemDraft> = {}): ItemDraft => ({
  title: "Tea", notes: "", tag: null, schedule: { kind: "timer", durationMs },
  nag: null, chime: null, quietExempt: null, startTimer: true, ...extra,
});
const item = async (id: string): Promise<ItemView> => {
  const found = (await api.snapshot()).items.find((value) => value.id === id);
  if (!found) throw new Error(`Missing ${id}`);
  return found;
};

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(start);
  vi.stubGlobal("window", {
    crypto: globalThis.crypto,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  });
  stop = installMockBackend();
});

afterEach(() => {
  stop();
  clearMocks();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("browser timer preview", () => {
  it("publishes created timers and naturally rings and nags until Done", async () => {
    const snapshots: Snapshot[] = [];
    const unlisten = await listen<Snapshot>("state-changed", (event) => snapshots.push(event.payload));
    await api.setSetting("nagDefault", { mode: "repeat", intervalMin: 2 });
    const id = await api.createItem(draft());
    expect(snapshots.at(-1)?.items.find((value) => value.id === id)?.status).toEqual({ kind: "upcoming", at: start + 5000 });
    expect((await item(id)).nag).toEqual({ mode: "repeat", intervalMin: 2 });

    await vi.advanceTimersByTimeAsync(5000);
    expect(snapshots.at(-1)?.items.find((value) => value.id === id)?.status.kind).toBe("due");
    expect((await item(id)).ring).toEqual({ alerts: 1, nextAlertAt: start + 125_000, held: null });
    await vi.advanceTimersByTimeAsync(120_000);
    expect((await item(id)).ring?.alerts).toBe(2);
    await api.done(id, start + 5000);
    expect(await item(id)).toMatchObject({ timer: { state: "idle" }, status: { kind: "idle" }, ring: null });
    expect(await api.history(id)).toMatchObject([{ kind: "done", occurrence: start + 5000, title: "Tea" }]);
    await unlisten();
  });

  it("pauses, resumes the exact remaining time, and resets to idle", async () => {
    const id = await api.createItem(draft(10_000));
    await vi.advanceTimersByTimeAsync(3000);
    await api.timer(id, "pause");
    expect((await item(id)).timer).toEqual({ state: "paused", remainingMs: 7000 });
    await vi.advanceTimersByTimeAsync(20_000);
    expect((await item(id)).status).toEqual({ kind: "idle" });
    await api.timer(id, "resume");
    expect((await item(id)).timer).toEqual({ state: "running", endAt: start + 30_000 });
    await vi.advanceTimersByTimeAsync(7000);
    await api.timer(id, "pause");
    expect((await item(id)).status.kind).toBe("due");
    await api.timer(id, "reset");
    expect(await item(id)).toMatchObject({ timer: { state: "idle" }, status: { kind: "idle" }, ring: null });
    await api.timer(id, "start");
    expect((await item(id)).timer).toEqual({ state: "running", endAt: start + 40_000 });
  });

  it("snoozes the same occurrence and rings again when the snooze expires", async () => {
    const id = await api.createItem(draft(1000));
    await vi.advanceTimersByTimeAsync(1000);
    await api.snooze(id, 1, start + 1000);
    expect(await item(id)).toMatchObject({
      status: { kind: "due", occurrence: start + 1000, ringsAt: start + 61_000, snoozed: true },
      snooze: { count: 1 }, ring: null,
    });
    await api.snooze(id, 1, start);
    expect((await item(id)).snooze?.count).toBe(1);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(await item(id)).toMatchObject({ status: { kind: "due", snoozed: false }, ring: { alerts: 1, held: null } });
    await api.done(id, start + 1000);
    expect((await api.history(id))[0]?.snoozeCount).toBe(1);
  });

  it("starts independent timers from preset settings", async () => {
    const presetId = await api.savePreset(null, {
      name: "Quick", durationMs: 2000, nag: { mode: "once" }, chime: { kind: "bundled", id: "soft" }, tag: "home", order: 4,
    });
    const first = await api.startPreset(presetId);
    const second = await api.startPreset(presetId);
    expect(first).not.toBe(second);
    expect(await item(first)).toMatchObject({ title: "Quick", nag: { mode: "once" }, tag: "home", chime: { id: "soft" } });
    await vi.advanceTimersByTimeAsync(2000);
    expect((await item(first)).ring).toEqual({ alerts: 1, nextAlertAt: null, held: null });
    await vi.advanceTimersByTimeAsync(120_000);
    expect((await item(first)).ring?.alerts).toBe(1);
    await api.done(first);
    expect((await item(second)).status.kind).toBe("due");
  });

  it("holds muted timers and releases them when mute ends", async () => {
    await api.muteFor(1);
    const id = await api.createItem(draft(1000));
    await vi.advanceTimersByTimeAsync(1000);
    expect((await item(id)).ring).toEqual({ alerts: 0, nextAlertAt: start + 60_000, held: "mute" });
    await vi.advanceTimersByTimeAsync(59_000);
    expect((await item(id)).ring).toEqual({ alerts: 1, nextAlertAt: start + 120_000, held: null });
  });

  it("keeps timers exempt from quiet hours unless explicitly overridden", async () => {
    await api.setSetting("quietHours", { enabled: true, start: "11:00:00", end: "13:00:00" });
    const exempt = await api.createItem(draft(1000));
    const held = await api.createItem(draft(1000, { quietExempt: false }));
    await vi.advanceTimersByTimeAsync(1000);
    expect((await item(exempt)).ring?.held).toBeNull();
    expect((await item(held)).ring?.held).toBe("quiet");
    await api.setSetting("quietHours", { enabled: false, start: "11:00:00", end: "13:00:00" });
    expect((await item(held)).ring?.alerts).toBe(1);
  });

  it("keeps completed reminders in history and archive restores timers", async () => {
    const id = await api.createItem(draft(1000, { schedule: { kind: "oneOff", due: start }, startTimer: false }));
    await api.done(id);
    expect(await item(id)).toMatchObject({ kind: "reminder", status: { kind: "idle" } });
    expect((await api.history(id))[0]?.itemId).toBe(id);
    const timerId = await api.createItem(draft(1000));
    await api.deleteItem(timerId);
    await vi.advanceTimersByTimeAsync(2000);
    const archived = (await api.snapshot()).archived?.find((value) => value.id === timerId);
    expect(archived).toMatchObject({ deleted: true, ring: null });
    await api.deleteItem(timerId, false);
    expect(await item(timerId)).toMatchObject({ deleted: false, status: { kind: "due" } });
  });

  it("returns isolated snapshots and rejects invalid timers", async () => {
    const id = await api.createItem(draft());
    const snapshot = await api.snapshot();
    snapshot.items.find((value) => value.id === id)!.title = "Changed outside IPC";
    expect((await item(id)).title).toBe("Tea");
    await expect(api.createItem(draft(0))).rejects.toThrow("valid timer duration");
    await expect(api.createItem(draft(1000, { title: " " }))).rejects.toThrow("title");
    stop();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("serializes nested reactive proxies at the IPC boundary", async () => {
    const localEvents: LocalSettings[] = [];
    const unlisten = await listen<LocalSettings>("local-settings-changed", (event) => localEvents.push(event.payload));
    const settings = await api.localSettings();
    settings.theme = "light";
    settings.chime = new Proxy(settings.chime, {});
    await api.setLocalSettings(new Proxy(settings, {}));
    expect(localEvents.at(-1)?.theme).toBe("light");
    settings.theme = "dark";
    expect((await api.localSettings()).theme).toBe("light");

    const timer = draft(2000);
    timer.schedule = new Proxy(timer.schedule, {});
    timer.nag = new Proxy({ mode: "repeat" as const, intervalMin: 3 }, {});
    const id = await api.createItem(new Proxy(timer, {}));
    expect(await item(id)).toMatchObject({ schedule: { durationMs: 2000 }, nag: { intervalMin: 3 } });
    const presetId = await api.savePreset(null, new Proxy({
      name: "Proxy preset", durationMs: 3000, nag: new Proxy({ mode: "once" as const }, {}),
      chime: new Proxy({ kind: "bundled" as const, id: "soft" }, {}), tag: null, order: 5,
    }, {}));
    const fromPreset = await api.startPreset(presetId);
    expect(await item(fromPreset)).toMatchObject({ title: "Proxy preset", nag: { mode: "once" }, chime: { id: "soft" } });
    await unlisten();
  });
});
