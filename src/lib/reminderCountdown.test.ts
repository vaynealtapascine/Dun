import { afterEach, describe, expect, it, vi } from "vitest";
import type { StatusView } from "./api/types";
import { selectReminderCountdown, splitReminderCountdown } from "./reminderCountdown";

// Calendar arithmetic follows the local device zone; the normal test zone is UTC.
const at = (iso: string) => Date.parse(`${iso}Z`);

describe("splitReminderCountdown", () => {
  afterEach(() => vi.unstubAllEnvs());

  it("splits calendar years, months, days and a padded clock", () => {
    expect(splitReminderCountdown(at("2024-01-15T10:20:30"), at("2026-04-20T12:24:35"))).toEqual({
      years: 2, months: 3, days: 5, hours: 2, minutes: 4, seconds: 5,
      calendar: "02:03:05", clock: "02:04:05",
      spoken: "2 years, 3 months, 5 days, 2 hours, 4 minutes, 5 seconds",
    });
  });

  it.each([
    ["2025-01-31T10:00:00", "2025-02-28T10:00:00", "00:01:00"],
    ["2024-01-31T10:00:00", "2024-02-29T10:00:00", "00:01:00"],
    ["2024-02-29T10:00:00", "2025-02-28T10:00:00", "01:00:00"],
    ["2025-01-31T10:00:00", "2025-03-30T10:00:00", "00:01:30"],
    ["2025-01-31T10:00:00", "2025-03-31T10:00:00", "00:02:00"],
  ])("clamps calendar anniversaries from %s to %s", (from, to, calendar) => {
    expect(splitReminderCountdown(at(from), at(to))).toMatchObject({ calendar, clock: "00:00:00" });
  });

  it("normalizes the month remainder before a leap-year anniversary", () => {
    expect(splitReminderCountdown(at("2024-02-29T10:00:00"), at("2028-02-28T10:00:00"))).toMatchObject({
      years: 3, months: 11, days: 30, calendar: "03:11:30", clock: "00:00:00",
    });
  });

  it("keeps month anniversaries anchored to the original leap day after a clamped year", () => {
    expect(splitReminderCountdown(at("2024-02-29T10:00:00"), at("2025-03-28T10:00:00"))).toMatchObject({
      years: 1, months: 0, days: 28, calendar: "01:00:28", clock: "00:00:00",
    });
    expect(splitReminderCountdown(at("2024-02-29T10:00:00"), at("2025-03-29T10:00:00"))).toMatchObject({
      years: 1, months: 1, days: 0, calendar: "01:01:00", clock: "00:00:00",
    });
  });

  it("does not count a partial calendar month as a full one", () => {
    expect(splitReminderCountdown(at("2025-01-31T10:00:00"), at("2025-02-28T09:59:59"))).toMatchObject({
      calendar: "00:00:27", clock: "23:59:59",
    });
  });

  it("uses the time of day rather than counting midnight crossings as whole days", () => {
    expect(splitReminderCountdown(at("2026-10-01T23:59:50"), at("2026-10-02T00:00:10"))).toMatchObject({
      calendar: "00:00:00", clock: "00:00:20",
    });
  });

  it("keeps positive fractional seconds visible and carries at calendar boundaries", () => {
    const from = at("2026-10-01T10:00:00");
    expect(splitReminderCountdown(from, from + 1)).toMatchObject({ clock: "00:00:01" });
    expect(splitReminderCountdown(from, from + 59_001)).toMatchObject({ clock: "00:01:00" });
    expect(splitReminderCountdown(from, from + 86_399_750)).toMatchObject({ calendar: "00:00:01", clock: "00:00:00" });
  });

  it("returns the same nonnegative elapsed interval for overdue timestamps", () => {
    const from = at("2024-02-29T14:00:00");
    const to = at("2026-04-02T15:02:03");
    expect(splitReminderCountdown(to, from)).toEqual(splitReminderCountdown(from, to));
  });

  it("only counts completed seconds when a reminder is overdue", () => {
    expect(splitReminderCountdown(1250, 1000)).toMatchObject({ clock: "00:00:00" });
    expect(splitReminderCountdown(2250, 1000)).toMatchObject({ clock: "00:00:01" });
  });

  it("renders exact equality and invalid instants safely", () => {
    expect(splitReminderCountdown(1000, 1000)).toMatchObject({ calendar: "00:00:00", clock: "00:00:00", spoken: "0 seconds" });
    expect(splitReminderCountdown(NaN, 1000)).toMatchObject({ calendar: "00:00:00", clock: "00:00:00" });
  });

  it("allows more than two digits for distant years", () => {
    expect(splitReminderCountdown(at("2026-01-01T00:00:00"), at("2152-01-01T00:00:00"))).toMatchObject({ calendar: "126:00:00" });
  });

  it("uses calendar days across daylight-saving transitions", () => {
    vi.stubEnv("TZ", "America/New_York");
    const springStart = new Date(2026, 2, 7, 12).getTime();
    const springEnd = new Date(2026, 2, 8, 12).getTime();
    expect(springEnd - springStart).toBe(23 * 3_600_000);
    expect(splitReminderCountdown(springStart, springEnd)).toMatchObject({ calendar: "00:00:01", clock: "00:00:00" });
    const fallStart = new Date(2026, 9, 31, 12).getTime();
    const fallEnd = new Date(2026, 10, 1, 12).getTime();
    expect(fallEnd - fallStart).toBe(25 * 3_600_000);
    expect(splitReminderCountdown(fallStart, fallEnd)).toMatchObject({ calendar: "00:00:01", clock: "00:00:00" });
    expect(splitReminderCountdown(fallStart, fallEnd - 3_600_000)).toMatchObject({ calendar: "00:00:00", clock: "24:00:00" });
  });
});

describe("selectReminderCountdown", () => {
  const now = at("2026-10-01T10:00:00");
  const due: Extract<StatusView, { kind: "due" }> = {
    kind: "due", occurrence: now - 60_000, firstMissed: null, missedCount: 1,
    ringsAt: now + 10_000, snoozed: false,
  };

  it("counts an upcoming occurrence and clamps a pending status update to due now", () => {
    expect(selectReminderCountdown({ kind: "upcoming", at: now + 1000 }, now)).toEqual({ at: now + 1000, label: "Due in", overdue: false });
    expect(selectReminderCountdown({ kind: "upcoming", at: now - 1000 }, now)).toEqual({ at: now, label: "Due now", overdue: false });
  });

  it("counts a snooze's resumption time instead of the overdue occurrence", () => {
    expect(selectReminderCountdown({ ...due, snoozed: true }, now)).toEqual({ at: due.ringsAt, label: "Alerts resume in", overdue: false });
    expect(selectReminderCountdown({ ...due, snoozed: true, ringsAt: now - 1000 }, now)).toEqual({ at: now, label: "Resuming alerts", overdue: false });
  });

  it("counts overdue time from the earliest missed occurrence rather than the next nag", () => {
    expect(selectReminderCountdown(due, now)).toEqual({ at: due.occurrence, label: "Overdue by", overdue: true });
    expect(selectReminderCountdown({ ...due, firstMissed: now - 86_400_000 }, now)).toEqual({ at: now - 86_400_000, label: "Overdue by", overdue: true });
  });

  it("never shows negative overdue time if the due status arrives early", () => {
    expect(selectReminderCountdown({ ...due, occurrence: now + 1000 }, now)).toEqual({ at: now, label: "Due now", overdue: false });
  });

  it.each(["idle", "unscheduled"] as const)("omits a countdown for %s reminders", (kind) => {
    expect(selectReminderCountdown({ kind }, now)).toBeNull();
  });
});
