import { describe, expect, it } from "vitest";
import type { Countdown, CountdownDisplay } from "./api/types";
import { DEFAULT_DISPLAY, readCountdown, readingText, sortCountdowns, splitSpan, visibleParts } from "./countdown";

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

  it("keeps Jan 31 monthly on the last day of shorter months", () => {
    const r = readCountdown(countdown({ date: "2026-01-31", repeat: "monthly" }), at("2026-02-10T00:00:00"));
    expect(r.target.toISOString().slice(0, 10)).toBe("2026-02-28");
    expect(readCountdown(countdown({ date: "2026-01-31", repeat: "monthly" }), at("2026-03-02T00:00:00")).target.toISOString().slice(0, 10))
      .toBe("2026-03-31");
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
