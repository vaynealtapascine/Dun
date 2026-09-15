import { describe, expect, it } from "vitest";
import {
  fromLocalInput,
  joinDuration,
  roundUpMinutes,
  splitDuration,
  toCivilTime,
  toDateInput,
  toLocalInput,
  toTimeInput,
} from "./dates";

// TZ=UTC in vitest.config.
describe("dates", () => {
  const ms = Date.parse("2026-09-16T09:05:30Z");

  it("round-trips datetime-local values", () => {
    expect(toLocalInput(ms)).toBe("2026-09-16T09:05");
    expect(fromLocalInput("2026-09-16T09:05")).toBe(Date.parse("2026-09-16T09:05:00Z"));
    expect(fromLocalInput("2026-09-16")).toBeNull();
    expect(fromLocalInput("")).toBeNull();
    expect(toDateInput(ms)).toBe("2026-09-16");
  });

  it("converts civil times", () => {
    expect(toCivilTime("9:5")).toBe("09:05:00");
    expect(toCivilTime("21:30")).toBe("21:30:00");
    expect(toTimeInput("21:30:00")).toBe("21:30");
  });

  it("rounds up to the next step", () => {
    expect(roundUpMinutes(Date.parse("2026-09-16T09:05:30Z"), 15)).toBe(Date.parse("2026-09-16T09:15:00Z"));
    expect(roundUpMinutes(Date.parse("2026-09-16T09:15:00Z"), 15)).toBe(Date.parse("2026-09-16T09:30:00Z"));
  });

  it("splits and joins durations", () => {
    expect(splitDuration(3_723_000)).toEqual({ h: 1, m: 2, s: 3 });
    expect(joinDuration(1, 2, 3)).toBe(3_723_000);
    expect(joinDuration(0, 45, 0)).toBe(2_700_000);
  });
});
