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
  it("buckets by state", () => {
    const t = (id: string, status: StatusView, timer: ItemView["timer"]) =>
      item(id, status, { kind: "timer", timer, schedule: { kind: "timer", durationMs: 60_000 } });
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
