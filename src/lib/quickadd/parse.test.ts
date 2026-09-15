import { describe, expect, it } from "vitest";
import type { Schedule, Tag } from "../api/types";
import { parseClock, parseQuickAdd, timerAsReminder } from "./parse";

// TZ=UTC (vitest.config). "Now" is Wednesday 2026-09-16 10:00.
const now = Date.parse("2026-09-16T10:00:00Z");
const tags: Tag[] = [{ id: "t-home", name: "Home", color: "#3cb371", order: 0, deleted: false }];
const opts = { now, tags, dateOnlyTime: "09:00:00" };
const p = (s: string) => parseQuickAdd(s, opts);
const iso = (ms: number) => new Date(ms).toISOString().slice(0, 16);

function rule(s: Schedule | null) {
  if (s?.kind !== "recurring") throw new Error(`expected recurring, got ${JSON.stringify(s)}`);
  return s.recurrence.rule;
}

describe("parseClock", () => {
  it.each([
    ["9", "09:00:00"],
    ["9am", "09:00:00"],
    ["9:30 pm", "21:30:00"],
    ["12am", "00:00:00"],
    ["12pm", "12:00:00"],
    ["21:05", "21:05:00"],
    ["noon", "12:00:00"],
    ["25", null],
  ])("%s -> %s", (input, expected) => {
    expect(parseClock(input)).toBe(expected);
  });
});

describe("timers", () => {
  it.each([
    ["Laundry in 45m", "Laundry", 45 * 60_000],
    ["Laundry in 45 minutes", "Laundry", 45 * 60_000],
    ["Bread in 1h 30m", "Bread", 90 * 60_000],
    ["Tea for 4 min", "Tea", 4 * 60_000],
    ["Check oven in an hour", "Check oven", 3_600_000],
    ["Nap in half an hour", "Nap", 30 * 60_000],
    ["Eggs 7 min timer", "Eggs", 7 * 60_000],
    ["Pasta in 90 seconds", "Pasta", 90_000],
  ])("%s", (input, title, ms) => {
    const r = p(input);
    expect(r.kind).toBe("timer");
    expect(r.title).toBe(title);
    expect(r.schedule).toEqual({ kind: "timer", durationMs: ms });
  });
});

describe("one-off reminders", () => {
  it("date and time", () => {
    const r = p("Pay rent tomorrow 9am");
    expect(r).toMatchObject({ kind: "once", title: "Pay rent" });
    expect(r.schedule?.kind === "oneOff" && iso(r.schedule.due)).toBe("2026-09-17T09:00");
  });

  it("a date alone uses the default time", () => {
    const r = p("Call mom friday");
    expect(r.title).toBe("Call mom");
    expect(r.schedule?.kind === "oneOff" && iso(r.schedule.due)).toBe("2026-09-18T09:00");
  });

  it("a time already passed today rolls forward", () => {
    const r = p("Standup at 9:30");
    expect(r.schedule?.kind === "oneOff" && iso(r.schedule.due)).toBe("2026-09-17T09:30");
  });

  it("strips 'remind me to'", () => {
    expect(p("remind me to water plants at 6pm").title).toBe("water plants");
  });

  it("no time found leaves the schedule empty", () => {
    const r = p("Buy milk");
    expect(r).toMatchObject({ kind: "once", title: "Buy milk", schedule: null });
  });
});

describe("repeats", () => {
  it("every weekday at 3pm", () => {
    const r = p("Stretch every weekday at 3pm");
    expect(r.kind).toBe("repeat");
    expect(r.title).toBe("Stretch");
    expect(rule(r.schedule)).toEqual({ kind: "weekly", every: 1, weekdays: [1, 2, 3, 4, 5], times: ["15:00:00"] });
  });

  it("every day at two times", () => {
    const r = p("Meds every day at 9am and 9pm");
    expect(r.title).toBe("Meds");
    expect(rule(r.schedule)).toEqual({ kind: "daily", every: 1, times: ["09:00:00", "21:00:00"] });
  });

  it("daily without a time uses the default", () => {
    expect(rule(p("Journal daily").schedule)).toEqual({ kind: "daily", every: 1, times: ["09:00:00"] });
  });

  it("every other day / every 3 days", () => {
    expect(rule(p("Water cactus every other day at 8").schedule)).toMatchObject({ kind: "daily", every: 2, times: ["08:00:00"] });
    expect(rule(p("Plants every 3 days").schedule)).toMatchObject({ kind: "daily", every: 3 });
  });

  it("named days", () => {
    const r = p("Gym every mon, wed and fri at 7am");
    expect(r.title).toBe("Gym");
    expect(rule(r.schedule)).toEqual({ kind: "weekly", every: 1, weekdays: [1, 3, 5], times: ["07:00:00"] });
  });

  it("weekends", () => {
    expect(rule(p("Long run every weekend at 8am").schedule)).toMatchObject({ weekdays: [6, 7] });
  });

  it("every 2 weeks on thursday", () => {
    const r = p("Payday every 2 weeks on thursday");
    expect(rule(r.schedule)).toEqual({ kind: "weekly", every: 2, weekdays: [4], times: ["09:00:00"] });
    expect(r.title).toBe("Payday");
  });

  it("monthly on the 1st", () => {
    const r = p("Rent monthly on the 1st at 10am");
    expect(r.title).toBe("Rent");
    expect(rule(r.schedule)).toEqual({ kind: "monthly", every: 1, day: 1, times: ["10:00:00"] });
  });

  it("every year on Mar 3", () => {
    const r = p("Mum's birthday every year on Mar 3");
    expect(r.title).toBe("Mum's birthday");
    expect(rule(r.schedule)).toEqual({ kind: "yearly", month: 3, day: 3, times: ["09:00:00"] });
  });

  it("intervals start on the next minute", () => {
    const r = p("Stand up every 45 minutes");
    expect(r.title).toBe("Stand up");
    expect(rule(r.schedule)).toEqual({ kind: "interval", every: 45, unit: "minutes" });
    expect(r.schedule?.kind === "recurring" && r.schedule.recurrence.start).toBe("2026-09-16T10:01:00");
    expect(rule(p("Drink water every hour").schedule)).toEqual({ kind: "interval", every: 1, unit: "hours" });
  });

  it("after I finish", () => {
    const r = p("Take pill every 8 hours after I finish");
    expect(r.title).toBe("Take pill");
    expect(r.schedule?.kind === "recurring" && r.schedule.mode).toBe("afterCompletion");
  });
});

describe("modifiers", () => {
  it("tags", () => {
    expect(p("Bins #home tomorrow").tagId).toBe("t-home");
    expect(p("Bins #home tomorrow").title).toBe("Bins");
    expect(p("Bins #garden tomorrow")).toMatchObject({ tagId: null, unknownTag: "garden" });
  });

  it("nagging", () => {
    expect(p("Laundry in 45m nag 5m").nag).toEqual({ mode: "repeat", intervalMin: 5 });
    expect(p("Laundry in 45m nag every 10 minutes").nag).toEqual({ mode: "repeat", intervalMin: 10 });
    const once = p("FYI meeting tomorrow at 2pm ring once");
    expect(once.nag).toEqual({ mode: "once" });
    expect(once.title).toBe("FYI meeting");
  });
});

describe("timerAsReminder", () => {
  it("turns a timer into a reminder that long from now", () => {
    const r = timerAsReminder(p("Laundry in 45m #home"), now);
    expect(r).toMatchObject({ kind: "once", title: "Laundry", tagId: "t-home" });
    expect(r.schedule).toEqual({ kind: "oneOff", due: now + 45 * 60_000 });
  });

  it("leaves other kinds alone", () => {
    const once = p("Pay rent tomorrow 9am");
    expect(timerAsReminder(once, now)).toBe(once);
  });
});
