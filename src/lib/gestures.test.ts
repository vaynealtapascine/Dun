import { describe, expect, it } from "vitest";
import { stepField } from "./gestures";

describe("duration field stepping", () => {
  it("steps and pads like the fields display", () => {
    expect(stepField("05", 1, 59)).toBe("06");
    expect(stepField("5", 10, 59)).toBe("15");
  });

  it("wraps around either end", () => {
    expect(stepField("59", 1, 59)).toBe("00");
    expect(stepField("00", -1, 59)).toBe("59");
    expect(stepField("03", -10, 59)).toBe("53");
    expect(stepField("999", 1, 999)).toBe("00");
  });

  it("treats an empty or unreadable field as zero", () => {
    expect(stepField("", 1, 59)).toBe("01");
    expect(stepField("x", -1, 59)).toBe("59");
  });
});
