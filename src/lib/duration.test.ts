import { describe, expect, it } from "vitest";
import { formatCountdown, formatShort } from "./duration";

describe("formatCountdown", () => {
  it.each([
    [0, "0:00"],
    [1, "0:01"],
    [999, "0:01"],
    [1000, "0:01"],
    [59_001, "1:00"],
    [245_000, "4:05"],
    [3_600_000, "1:00:00"],
    [3_723_000, "1:02:03"],
    [86_400_000, "1d 0:00:00"],
    [90_061_000, "1d 1:01:01"],
    [-65_000, "-1:05"],
  ])("%d ms -> %s", (ms, expected) => {
    expect(formatCountdown(ms)).toBe(expected);
  });
});

describe("formatShort", () => {
  it.each([
    [0, "0m"],
    [45 * 60_000, "45m"],
    [90 * 60_000, "1h 30m"],
    [2 * 3_600_000, "2h"],
    [(2 * 24 + 4) * 3_600_000 + 15 * 60_000, "2d 4h"],
  ])("%d ms -> %s", (ms, expected) => {
    expect(formatShort(ms)).toBe(expected);
  });
});
