// Dev-only: a fake backend so the UI can be previewed in a plain browser
// (`npm run dev`, then open http://localhost:1420). Never bundled into the app:
// main.ts only imports this when running under Vite dev outside Tauri.

import { mockIPC } from "@tauri-apps/api/mocks";
import type { HistoryRow, ItemView, LocalSettings, Snapshot, StatusView } from "../api/types";

const MIN = 60_000;

export function installMockBackend() {
  // Lets the app tell a browser preview apart from a real (e.g. Android) webview.
  (window as unknown as Record<string, unknown>).__DUN_PREVIEW__ = true;
  const now = Date.now();
  const base = (id: string, title: string, status: StatusView, extra: Partial<ItemView> = {}): ItemView => ({
    id,
    created: { at: now - 86_400_000, kind: "reminder", by: "preview" },
    title,
    notes: "",
    tag: null,
    schedule: { kind: "oneOff", due: now },
    timer: { state: "idle" },
    nag: { mode: "repeat", intervalMin: 1 },
    chime: null,
    quietExemptOverride: null,
    completion: { through: null, doneAt: null, gen: 0 },
    snooze: null,
    deleted: false,
    kind: "reminder",
    quietExempt: false,
    status,
    ring: null,
    ...extra,
  });
  const due = (at: number, snoozed = false, missed = 1): StatusView => ({
    kind: "due",
    occurrence: at,
    firstMissed: missed > 1 ? at - (missed - 1) * 86_400_000 : null,
    missedCount: missed,
    ringsAt: at,
    snoozed,
  });

  const snapshot: Snapshot = {
    now,
    tz: "Local",
    deviceId: "preview",
    tags: [
      { id: "home", name: "Home", color: "#3cb371", order: 0, deleted: false },
      { id: "work", name: "Work", color: "#3a86ff", order: 1, deleted: false },
    ],
    presets: [
      { id: "tea", name: "Tea", durationMs: 4 * MIN, nag: { mode: "repeat", intervalMin: 1 }, chime: null, tag: null, order: 0, deleted: false },
      { id: "pomo", name: "Pomodoro", durationMs: 25 * MIN, nag: { mode: "repeat", intervalMin: 1 }, chime: null, tag: "work", order: 1, deleted: false },
      { id: "laundry", name: "Laundry", durationMs: 45 * MIN, nag: { mode: "repeat", intervalMin: 5 }, chime: null, tag: "home", order: 2, deleted: false },
    ],
    settings: {
      quietHours: { enabled: true, start: "23:00:00", end: "07:00:00" },
      muteUntil: null,
      nagDefault: { mode: "repeat", intervalMin: 1 },
      dateOnlyTime: "09:00:00",
      missedSummaryThreshold: 3,
      handoff: { enabled: true, failLoudAfterS: 120, phoneGraceS: 120 },
    },
    summary: [],
    items: [
      base("rent", "Pay rent", due(now - 3 * MIN), { tag: "home", ring: { alerts: 3, nextAlertAt: now + MIN, held: null } }),
      base(
        "meds",
        "Evening meds",
        due(now - 26 * 60 * MIN, false, 2),
        {
          kind: "recurring",
          schedule: {
            kind: "recurring",
            recurrence: { rule: { kind: "daily", every: 1, times: ["21:00:00"] }, start: "2026-09-01T00:00:00", tz: null },
            mode: "fromSchedule",
            effectiveFrom: 0,
          },
          ring: { alerts: 1, nextAlertAt: now + MIN, held: null },
        },
      ),
      base("call", "Call the dentist", due(now + 12 * MIN, true), { tag: "work", snooze: { occurrence: now - 5 * MIN, until: now + 12 * MIN, count: 2 } }),
      base("stretch", "Stretch", { kind: "upcoming", at: now + 95 * MIN }, {
        kind: "recurring",
        schedule: {
          kind: "recurring",
          recurrence: { rule: { kind: "weekly", every: 1, weekdays: [1, 2, 3, 4, 5], times: ["15:00:00"] }, start: "2026-09-01T00:00:00", tz: null },
          mode: "fromSchedule",
          effectiveFrom: 0,
        },
      }),
      base("bins", "Take the bins out", { kind: "upcoming", at: now + 26 * 60 * MIN }, { tag: "home", notes: "Recycling week" }),
      base("passport", "Renew passport", { kind: "upcoming", at: now + 9 * 86_400_000 }),
      base("pomo-1", "Pomodoro", { kind: "upcoming", at: now + 17 * MIN }, {
        kind: "timer",
        quietExempt: true,
        tag: "work",
        schedule: { kind: "timer", durationMs: 25 * MIN },
        timer: { state: "running", endAt: now + 17 * MIN },
      }),
      base("eggs", "Eggs", due(now - 20_000), {
        kind: "timer",
        quietExempt: true,
        schedule: { kind: "timer", durationMs: 7 * MIN },
        timer: { state: "running", endAt: now - 20_000 },
      }),
      base("bread", "Bread proof", { kind: "idle" }, {
        kind: "timer",
        quietExempt: true,
        schedule: { kind: "timer", durationMs: 90 * MIN },
        timer: { state: "paused", remainingMs: 38 * MIN },
      }),
    ],
  };

  const local: LocalSettings = {
    chime: { kind: "bundled", id: "bell" },
    volume: 0.8,
    hotkey: "CommandOrControl+Alt+N",
    autostart: true,
    idleThresholdS: 300,
    theme: "system",
  };

  const history: HistoryRow[] = [
    { id: "h1", itemId: "stretch", kind: "done", occurrence: now - 20 * 60 * MIN, at: now - 19 * 60 * MIN, snoozeCount: 1, title: "Stretch", refId: null, prevCompletion: null, device: "phone" },
    { id: "h2", itemId: "rent", kind: "done", occurrence: now - 2 * 86_400_000, at: now - 2 * 86_400_000 + 7 * MIN, snoozeCount: 0, title: "Water plants", refId: null, prevCompletion: null, device: "preview" },
  ];

  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    switch (cmd) {
      case "get_snapshot":
        return { ...snapshot, now: Date.now() };
      case "get_local_settings":
        return local;
      case "list_chimes":
        return ["bell", "rise", "pulse", "soft"].map((id) => ({ chime: { kind: "bundled", id }, label: id[0]!.toUpperCase() + id.slice(1) }));
      case "get_history":
        return history;
      case "app_version":
        return "v0.1.0 (preview)";
      case "mark_done":
        snapshot.items = snapshot.items.filter((i) => i.id !== a.id);
        return null;
      default:
        return null;
    }
  });
}
