import { describe, expect, it } from "vitest";
import { dayLabel, nagSummary, ordinal, relative, ruleSummary, when } from "./format";

// vitest.config pins TZ=UTC, so local time is UTC here.
const at = (iso: string) => Date.parse(`${iso}Z`);
const now = at("2026-09-16T10:00:00"); // a Wednesday

describe("dayLabel / when", () => {
  it.each([
    ["2026-09-16T23:00:00", "Today"],
    ["2026-09-17T08:00:00", "Tomorrow"],
    ["2026-09-15T08:00:00", "Yesterday"],
    ["2026-09-19T08:00:00", "Saturday"],
    ["2026-09-30T08:00:00", "Sep 30"],
    ["2027-01-02T08:00:00", "Jan 2, 2027"],
  ])("%s -> %s", (iso, label) => {
    expect(dayLabel(at(iso), now)).toBe(label);
  });

  it("omits the day for today", () => {
    expect(when(at("2026-09-16T21:05:00"), now)).toMatch(/^9:05\sPM$/);
    expect(when(at("2026-09-17T09:00:00"), now)).toMatch(/^Tomorrow 9:00\sAM$/);
  });
});

describe("relative", () => {
  it.each([
    [0, "now"],
    [-20_000, "just now"],
    [5 * 60_000, "in 5 min"],
    [130 * 60_000, "in 2 h 10 min"],
    [120 * 60_000, "in 2 h"],
    [-3 * 86_400_000, "3 days ago"],
    [36 * 3_600_000, "in 2 days"],
  ])("%d ms -> %s", (delta, text) => {
    expect(relative(now + delta, now)).toBe(text);
  });
});

describe("summaries", () => {
  it("ordinals", () => {
    expect([1, 2, 3, 4, 11, 12, 13, 21, 22, 31].map(ordinal)).toEqual([
      "1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "31st",
    ]);
  });

  it("rules read naturally", () => {
    const nine = "09:00:00";
    expect(ruleSummary({ kind: "interval", every: 90, unit: "minutes" })).toBe("Every 90 minutes");
    expect(ruleSummary({ kind: "interval", every: 1, unit: "hours" })).toBe("Every hour");
    expect(ruleSummary({ kind: "daily", every: 1, times: [nine] })).toMatch(/^Every day at 9:00\sAM$/);
    expect(ruleSummary({ kind: "daily", every: 2, times: [nine, "21:00:00"] })).toMatch(
      /^Every 2 days at 9:00\sAM and 9:00\sPM$/,
    );
    expect(ruleSummary({ kind: "weekly", every: 1, weekdays: [5, 1, 2, 3, 4], times: ["15:00:00"] })).toMatch(
      /^Weekdays at 3:00\sPM$/,
    );
    expect(ruleSummary({ kind: "weekly", every: 2, weekdays: [1, 3], times: [nine] })).toMatch(
      /^Every 2 weeks, Mon, Wed at 9:00\sAM$/,
    );
    expect(ruleSummary({ kind: "monthly", every: 1, day: 1, times: [nine] })).toMatch(/^Monthly on the 1st at/);
    expect(ruleSummary({ kind: "yearly", month: 3, day: 3, times: [nine] })).toMatch(/^Every year on Mar 3 at/);
  });

  it("nag", () => {
    expect(nagSummary({ mode: "repeat", intervalMin: 1 })).toBe("Nags every minute");
    expect(nagSummary({ mode: "repeat", intervalMin: 5 })).toBe("Nags every 5 minutes");
    expect(nagSummary({ mode: "once" })).toBe("Rings once");
  });
});
