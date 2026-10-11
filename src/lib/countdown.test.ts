import { afterEach, describe, expect, it, vi } from "vitest";
import type { Countdown, CountdownDisplay } from "./api/types";
import { DEFAULT_DISPLAY, displayUnits, readCountdown, readingText, sortCountdowns, splitSpan, visibleParts } from "./countdown";

// The test zone is UTC (see vitest.config.ts), so local and ISO times agree.
const at = (iso: string) => Date.parse(`${iso}Z`);

function countdown(over: Partial<Omit<Countdown, "display">> & { display?: Partial<CountdownDisplay> } = {}): Countdown {
  const { display, ...rest } = over;
  return {
    id: "c", title: "x", notes: "", date: "2026-12-25", time: null, repeat: "none", direction: "auto", pinned: false, deleted: false,
    ...rest,
    display: { ...DEFAULT_DISPLAY, ...display },
  };
}

const text = (c: Countdown, now: string, long = true) => readingText(readCountdown(c, at(now)).parts, long);

describe("splitSpan", () => {
  afterEach(() => vi.unstubAllEnvs());

  it("splits calendar units from years down to seconds", () => {
    const parts = splitSpan(new Date(at("2024-01-15T10:20:30")), new Date(at("2026-04-20T12:24:35")), "years", "seconds");
    expect(parts.map((p) => p.value)).toEqual([2, 3, 0, 5, 2, 4, 5]);
  });

  it("folds missing larger units into the largest shown", () => {
    const from = new Date(at("2024-01-01T00:00:00"));
    const to = new Date(at("2025-01-15T06:00:00"));
    expect(splitSpan(from, to, "days", "days")).toEqual([{ unit: "days", value: 380 }]);
    expect(splitSpan(from, to, "weeks", "days")).toEqual([{ unit: "weeks", value: 54 }, { unit: "days", value: 2 }]);
    expect(splitSpan(from, to, "months", "weeks")).toEqual([{ unit: "months", value: 12 }, { unit: "weeks", value: 2 }]);
    expect(splitSpan(from, to, "hours", "hours")).toEqual([{ unit: "hours", value: 380 * 24 + 6 }]);
    expect(splitSpan(from, to, "years", "years")).toEqual([{ unit: "years", value: 1 }]);
  });

  it("clamps month ends", () => {
    expect(splitSpan(new Date(at("2025-01-31T00:00:00")), new Date(at("2025-02-28T00:00:00")), "months", "days"))
      .toEqual([{ unit: "months", value: 1 }, { unit: "weeks", value: 0 }, { unit: "days", value: 0 }]);
  });

  it("keeps remaining days when weeks are disabled", () => {
    expect(splitSpan(new Date(at("2024-01-15T00:00:00")), new Date(at("2026-04-30T00:00:00")), ["years", "months", "days"]))
      .toEqual([{ unit: "years", value: 2 }, { unit: "months", value: 3 }, { unit: "days", value: 15 }]);
    expect(splitSpan(new Date(at("2025-01-31T00:00:00")), new Date(at("2025-03-30T00:00:00")), ["months", "days"]))
      .toEqual([{ unit: "months", value: 1 }, { unit: "days", value: 30 }]);
  });

  it("folds skipped months and days into the next enabled unit", () => {
    const from = new Date(at("2024-01-01T00:00:00"));
    const to = new Date(at("2026-04-16T06:00:00"));
    expect(splitSpan(from, to, ["years", "days"]))
      .toEqual([{ unit: "years", value: 2 }, { unit: "days", value: 105 }]);
    expect(splitSpan(from, to, ["months", "hours"]))
      .toEqual([{ unit: "months", value: 27 }, { unit: "hours", value: 15 * 24 + 6 }]);
  });

  it("folds skipped clock units into smaller ones in calendar order", () => {
    expect(splitSpan(new Date(at("2026-01-01T00:00:00")), new Date(at("2026-01-02T01:02:05")), ["seconds", "days", "hours", "hours"]))
      .toEqual([{ unit: "days", value: 1 }, { unit: "hours", value: 1 }, { unit: "seconds", value: 125 }]);
  });

  it("uses calendar days across daylight-saving changes and elapsed hours when days are off", () => {
    vi.stubEnv("TZ", "America/New_York");
    const from = new Date("2026-03-07T12:00:00-05:00");
    const to = new Date("2026-03-09T13:00:00-04:00");
    expect(splitSpan(from, to, ["days", "hours"])).toEqual([{ unit: "days", value: 2 }, { unit: "hours", value: 1 }]);
    expect(splitSpan(from, to, ["hours"])).toEqual([{ unit: "hours", value: 48 }]);
  });
});

describe("displayUnits", () => {
  it("preserves legacy ranges and prefers individual selections", () => {
    const display = { ...DEFAULT_DISPLAY, largest: "years", smallest: "days" } as CountdownDisplay;
    expect(displayUnits(display)).toEqual(["years", "months", "weeks", "days"]);
    expect(displayUnits({ ...display, units: ["days", "years", "months", "days"] })).toEqual(["years", "months", "days"]);
  });
});

describe("readCountdown", () => {
  it("counts down to a date ahead and up from one behind", () => {
    expect(text(countdown(), "2026-10-11T15:00:00")).toBe("75 days");
    const r = readCountdown(countdown({ date: "2026-01-01" }), at("2026-10-11T15:00:00"));
    expect(r.mode).toBe("since");
    expect(readingText(r.parts, false)).toBe("283d");
  });

  it("reads today on the day, whatever the hour", () => {
    const r = readCountdown(countdown({ date: "2026-10-11", time: "18:00:00" }), at("2026-10-11T09:00:00"));
    expect(r.isNow).toBe(true);
    expect(readingText(r.parts, true)).toBe("0 days");
  });

  it("can count the first day as day 1", () => {
    expect(text(countdown({ date: "2026-10-11", display: { countFirstDay: true } }), "2026-10-11T09:00:00")).toBe("1 day");
    expect(text(countdown({ date: "2026-10-01", display: { countFirstDay: true } }), "2026-10-11T09:00:00")).toBe("11 days");
  });

  it("counts a yearly date to the next anniversary and numbers it", () => {
    const birthday = countdown({ date: "1961-03-14", repeat: "yearly", display: { largest: "months", smallest: "days" } });
    const r = readCountdown(birthday, at("2026-10-11T09:00:00"));
    expect(r.mode).toBe("until");
    expect(r.index).toBe(66);
    expect(readingText(r.parts, true)).toBe("5 months 3 days");
    expect(r.progress).toBeGreaterThan(0.5);
  });

  it("forces a direction where a repeat allows, and flips where it can't", () => {
    const since = readCountdown(countdown({ date: "2026-01-11", repeat: "monthly", direction: "since" }), at("2026-10-20T00:00:00"));
    expect([since.mode, since.index, since.flipped]).toEqual(["since", 9, false]);
    expect(readingText(since.parts, true)).toBe("9 days");
    const gone = readCountdown(countdown({ date: "2026-01-01", direction: "until" }), at("2026-10-11T00:00:00"));
    expect([gone.mode, gone.flipped]).toEqual(["since", true]);
    const ahead = readCountdown(countdown({ direction: "since" }), at("2026-10-11T00:00:00"));
    expect([ahead.mode, ahead.flipped]).toEqual(["until", true]);
  });

  it("ticks to the second from the set time", () => {
    const c = countdown({ date: "2026-10-12", time: "08:30:00", display: { largest: "hours", smallest: "seconds" } });
    expect(text(c, "2026-10-11T20:00:05", false)).toBe("12h 29m 55s");
  });

  it("uses enabled units for both the reading and whole-day behavior", () => {
    const c = countdown({ date: "2027-02-01", display: { largest: "days", smallest: "seconds", units: ["years", "months", "days"] } });
    expect(text(c, "2026-10-11T15:00:00")).toBe("3 months 21 days");
    expect(readCountdown({ ...c, date: "2026-10-11", time: "18:00:00" }, at("2026-10-11T09:00:00")).isNow).toBe(true);
    expect(text(countdown({ date: "2026-10-12", time: "08:30:00", display: { units: ["hours", "seconds"] } }), "2026-10-11T20:00:05", false))
      .toBe("12h 1795s");
  });

  it("keeps Jan 31 monthly on the last day of shorter months", () => {
    const r = readCountdown(countdown({ date: "2026-01-31", repeat: "monthly" }), at("2026-02-10T00:00:00"));
    expect(r.target.toISOString().slice(0, 10)).toBe("2026-02-28");
    expect(readCountdown(countdown({ date: "2026-01-31", repeat: "monthly" }), at("2026-03-02T00:00:00")).target.toISOString().slice(0, 10))
      .toBe("2026-03-31");
  });
});

describe("anniversary totals", () => {
  it("counts to the next anniversary while keeping the full duration from the original start", () => {
    const c = countdown({ date: "2020-05-01", repeat: "yearly", display: { units: ["years", "months", "days"], showTotal: true } });
    const r = readCountdown(c, at("2026-10-20T15:00:00"));
    expect([r.mode, r.index]).toEqual(["until", 7]);
    expect(readingText(r.parts, true)).toBe("6 months 11 days");
    expect(r.total?.start.toISOString()).toBe("2020-05-01T00:00:00.000Z");
    expect(readingText(r.total!.parts, true)).toBe("6 years 5 months 19 days");
  });

  it("keeps the total growing on and after an anniversary", () => {
    const c = countdown({ date: "2020-05-01", repeat: "yearly", display: { units: ["years", "months", "days"], showTotal: true } });
    const on = readCountdown(c, at("2026-05-01T15:00:00"));
    expect(on.isNow).toBe(true);
    expect(readingText(on.total!.parts, true)).toBe("6 years");
    const after = readCountdown(c, at("2026-05-02T15:00:00"));
    expect(after.index).toBe(7);
    expect(readingText(after.total!.parts, true)).toBe("6 years 1 day");
    const up = readCountdown({ ...c, direction: "since" }, at("2026-05-02T15:00:00"));
    expect(readingText(up.parts, true)).toBe("1 day");
    expect(up.total).toEqual(after.total);
  });

  it("clamps Feb 29 anniversaries without changing the total's start date", () => {
    const c = countdown({ date: "2024-02-29", repeat: "yearly", display: { units: ["years", "months", "days"], showTotal: true } });
    const r = readCountdown(c, at("2025-02-28T12:00:00"));
    expect(r.isNow).toBe(true);
    expect(r.target.toISOString().slice(0, 10)).toBe("2025-02-28");
    expect(readingText(r.total!.parts, true)).toBe("1 year");
    expect(r.total?.start.toISOString().slice(0, 10)).toBe("2024-02-29");
    const leapYear = readCountdown(c, at("2028-02-28T12:00:00"));
    expect(leapYear.target.toISOString().slice(0, 10)).toBe("2028-02-29");
    expect(readingText(leapYear.total!.parts, true)).toBe("3 years 11 months 30 days");
  });

  it("starts at zero, supports day 1, and does not count before the original start time", () => {
    const c = countdown({ date: "2026-10-12", time: "09:00:00", repeat: "yearly", display: { units: ["days", "seconds"], showTotal: true, countFirstDay: true } });
    const before = readCountdown(c, at("2026-10-12T08:59:59"));
    expect(before.total?.started).toBe(false);
    expect(readingText(before.total!.parts, true)).toBe("0 seconds");
    const start = readCountdown(c, at("2026-10-12T09:00:00"));
    expect(start.total?.started).toBe(true);
    expect(readingText(start.total!.parts, true)).toBe("1 day");
    expect(readingText(readCountdown({ ...c, display: { ...c.display, countFirstDay: false } }, at("2026-10-12T09:00:00")).total!.parts, true))
      .toBe("0 seconds");
  });

  it("uses the same total behavior for monthly and weekly repeats and can be turned off", () => {
    for (const repeat of ["monthly", "weekly"] as const) {
      const c = countdown({ date: "2026-01-31", repeat, display: { units: ["days"], showTotal: true } });
      expect(readingText(readCountdown(c, at("2026-03-01T12:00:00")).total!.parts, true)).toBe("29 days");
      expect(readCountdown({ ...c, display: { ...c.display, showTotal: false } }, at("2026-03-01T12:00:00")).total).toBeNull();
      expect(readCountdown({ ...c, repeat: "none" }, at("2026-03-01T12:00:00")).total).toBeNull();
    }
  });
});

describe("visibleParts", () => {
  it("hides empty units but always shows something", () => {
    expect(visibleParts([{ unit: "years", value: 0 }, { unit: "days", value: 4 }])).toEqual([{ unit: "days", value: 4 }]);
    expect(visibleParts([{ unit: "hours", value: 0 }, { unit: "minutes", value: 0 }])).toEqual([{ unit: "minutes", value: 0 }]);
  });
});

describe("sortCountdowns", () => {
  it("puts pinned first, then what's coming soonest, then what's gone most recently", () => {
    const now = at("2026-10-11T12:00:00");
    const list = [
      countdown({ id: "old", date: "2020-01-01" }),
      countdown({ id: "recent", date: "2026-10-01" }),
      countdown({ id: "far", date: "2027-06-01" }),
      countdown({ id: "soon", date: "2026-10-20" }),
      countdown({ id: "pin", date: "2019-01-01", pinned: true }),
    ];
    expect(sortCountdowns(list, now).map((e) => e.countdown.id)).toEqual(["pin", "soon", "far", "recent", "old"]);
  });
});
