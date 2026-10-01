import { describe, expect, it } from "vitest";
import type { ItemView, StatusView, Tag } from "./api/types";
import { filterItems, groupReminders, groupTimers } from "./grouping";

const at = (iso: string) => Date.parse(`${iso}Z`);
const now = at("2026-09-16T10:00:00");

function item(id: string, status: StatusView, extra: Partial<ItemView> = {}): ItemView {
  return {
    id,
    created: null,
    title: id,
    notes: "",
    tag: null,
    schedule: { kind: "oneOff", due: 0 },
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
  };
}

const due = (ringsAt: number, snoozed = false): StatusView => ({
  kind: "due",
  occurrence: ringsAt,
  firstMissed: null,
  missedCount: 1,
  ringsAt,
  snoozed,
});
const upcoming = (iso: string): StatusView => ({ kind: "upcoming", at: at(iso) });

describe("groupReminders", () => {
  it("orders groups and items and leaves out timers and finished items", () => {
    const groups = groupReminders(
      [
        item("later", upcoming("2026-09-20T09:00:00")),
        item("tomorrow", upcoming("2026-09-17T09:00:00")),
        item("today-late", upcoming("2026-09-16T21:00:00")),
        item("today-soon", upcoming("2026-09-16T11:00:00")),
        item("snoozed", due(now + 300_000, true)),
        item("held", due(now - 60_000), { ring: { alerts: 0, nextAlertAt: null, held: "quiet" } }),
        item("ringing-old", due(now - 600_000)),
        item("ringing-new", due(now - 60_000)),
        item("done", { kind: "idle" }),
        item("timer", due(now), { kind: "timer" }),
      ],
      now,
    );
    expect(groups.map((g) => [g.key, g.items.map((i) => i.id)])).toEqual([
      ["ringing", ["ringing-old", "ringing-new"]],
      ["overdue", ["held", "snoozed"]],
      ["today", ["today-soon", "today-late"]],
      ["tomorrow", ["tomorrow"]],
      ["later", ["later"]],
    ]);
  });

  it("drops empty groups", () => {
    expect(groupReminders([item("x", upcoming("2026-09-16T12:00:00"))], now).map((g) => g.key)).toEqual(["today"]);
  });
});

describe("groupTimers", () => {
  const t = (id: string, status: StatusView, timer: ItemView["timer"], createdAt: number | null = null) =>
    item(id, status, {
      kind: "timer",
      timer,
      schedule: { kind: "timer", durationMs: 60_000 },
      created: createdAt == null ? null : { at: createdAt, kind: "timer", by: "test-device" },
    });
  const ids = (items: ItemView[]) => items.map((item) => item.id);

  it("buckets by state", () => {
    const b = groupTimers([
      t("ringing", due(now - 1000), { state: "running", endAt: now - 1000 }),
      t("run-b", { kind: "upcoming", at: now + 9000 }, { state: "running", endAt: now + 9000 }),
      t("run-a", { kind: "upcoming", at: now + 1000 }, { state: "running", endAt: now + 1000 }),
      t("paused", { kind: "idle" }, { state: "paused", remainingMs: 5000 }),
      t("idle", { kind: "idle" }, { state: "idle" }),
      item("reminder", upcoming("2026-09-16T12:00:00")),
    ]);
    expect(b.ringing.map((i) => i.id)).toEqual(["ringing"]);
    expect(b.running.map((i) => i.id)).toEqual(["run-a", "run-b"]);
    expect(b.paused.map((i) => i.id)).toEqual(["paused"]);
    expect(b.idle.map((i) => i.id)).toEqual(["idle"]);
    expect(ids(b.active)).toEqual(["ringing", "paused", "run-a", "run-b"]);
  });

  it("keeps a timer in its active position when paused and resumed", () => {
    const older = t("older", { kind: "upcoming", at: now + 1000 }, { state: "running", endAt: now + 1000 }, now - 2000);
    const newer = t("newer", { kind: "upcoming", at: now + 9000 }, { state: "running", endAt: now + 9000 }, now - 1000);
    expect(ids(groupTimers([older, newer]).active)).toEqual(["newer", "older"]);

    const paused = { ...newer, status: { kind: "idle" } as const, timer: { state: "paused", remainingMs: 8000 } as const };
    expect(ids(groupTimers([paused, older]).active)).toEqual(["newer", "older"]);

    const resumed = { ...newer, status: { kind: "upcoming", at: now + 120_000 } as const, timer: { state: "running", endAt: now + 120_000 } as const };
    expect(ids(groupTimers([older, resumed]).active)).toEqual(["newer", "older"]);
  });

  it("promotes a started ready timer without changing the order of existing active timers", () => {
    const older = t("older", { kind: "upcoming", at: now + 1000 }, { state: "running", endAt: now + 1000 }, now - 3000);
    const paused = t("paused", { kind: "idle" }, { state: "paused", remainingMs: 5000 }, now - 2000);
    const ready = t("ready", { kind: "idle" }, { state: "idle" }, now - 1000);
    expect(ids(groupTimers([ready, older, paused]).active)).toEqual(["paused", "older"]);
    expect(ids(groupTimers([ready, older, paused]).idle)).toEqual(["ready"]);

    const started = { ...ready, status: { kind: "upcoming", at: now + 60_000 } as const, timer: { state: "running", endAt: now + 60_000 } as const };
    expect(ids(groupTimers([older, started, paused]).active)).toEqual(["ready", "paused", "older"]);
  });

  it("promotes expiry but does not shuffle expired timers when snoozed or alert times change", () => {
    const older = t("older", { kind: "upcoming", at: now + 1000 }, { state: "running", endAt: now + 1000 }, now - 3000);
    const newer = t("newer", due(now - 1000), { state: "running", endAt: now - 1000 }, now - 2000);
    const paused = t("paused", { kind: "idle" }, { state: "paused", remainingMs: 5000 }, now - 1000);
    expect(ids(groupTimers([older, newer, paused]).active)).toEqual(["newer", "paused", "older"]);

    const expired = { ...older, status: due(now - 500) };
    expect(ids(groupTimers([paused, expired, newer]).active)).toEqual(["newer", "older", "paused"]);

    const snoozed = { ...newer, title: "Renamed", status: due(now + 300_000, true) };
    expect(ids(groupTimers([expired, paused, snoozed]).active)).toEqual(["newer", "older", "paused"]);

    const reset = { ...expired, status: { kind: "idle" } as const, timer: { state: "idle" } as const };
    expect(ids(groupTimers([reset, paused, snoozed]).active)).toEqual(["newer", "paused"]);
    expect(ids(groupTimers([reset, paused, snoozed]).idle)).toEqual(["older"]);
  });

  it("uses immutable IDs for equal or missing creation dates, independent of snapshot order and titles", () => {
    const timers = [
      t("legacy-b", { kind: "idle" }, { state: "paused", remainingMs: 5000 }),
      t("new-b", { kind: "upcoming", at: now + 1000 }, { state: "running", endAt: now + 1000 }, now - 1000),
      t("legacy-a", { kind: "upcoming", at: now + 9000 }, { state: "running", endAt: now + 9000 }),
      t("new-a", { kind: "idle" }, { state: "paused", remainingMs: 5000 }, now - 1000),
    ];
    const originalIds = ids(timers);
    expect(ids(groupTimers(timers).active)).toEqual(["new-a", "new-b", "legacy-a", "legacy-b"]);
    expect(ids(timers)).toEqual(originalIds);

    const shuffled = [timers[3]!, timers[2]!, timers[1]!, { ...timers[0]!, title: "A different title" }];
    expect(ids(groupTimers(shuffled).active)).toEqual(["new-a", "new-b", "legacy-a", "legacy-b"]);
  });
});

describe("filterItems", () => {
  const tags: Tag[] = [{ id: "t1", name: "Home", color: "#3a86ff", order: 0, deleted: false }];
  const items = [
    item("Pay rent", upcoming("2026-09-16T12:00:00"), { tag: "t1" }),
    item("Stretch", upcoming("2026-09-16T12:00:00"), { notes: "back and neck" }),
  ];

  it("matches title, notes and tag name", () => {
    expect(filterItems(items, "rent", null, tags).map((i) => i.id)).toEqual(["Pay rent"]);
    expect(filterItems(items, "NECK", null, tags).map((i) => i.id)).toEqual(["Stretch"]);
    expect(filterItems(items, "home", null, tags).map((i) => i.id)).toEqual(["Pay rent"]);
    expect(filterItems(items, "", "t1", tags).map((i) => i.id)).toEqual(["Pay rent"]);
    expect(filterItems(items, "stretch", "t1", tags)).toEqual([]);
  });
});
