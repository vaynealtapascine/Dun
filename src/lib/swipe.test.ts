import { describe, expect, it } from "vitest";
import { axisOf, offsetFor, REVEAL_PX, rows, settle } from "./swipe";

describe("swipe gestures", () => {
  it("waits before deciding which way a gesture is going", () => {
    expect(axisOf(3, 2)).toBe(null);
    expect(axisOf(-20, 3)).toBe("x");
    // A finger scrolling the list belongs to the list.
    expect(axisOf(-6, 30)).toBe("y");
    expect(axisOf(-20, 25)).toBe("y");
  });

  it("only opens leftwards, and resists past the button", () => {
    expect(offsetFor(30)).toBe(0);
    expect(offsetFor(0)).toBe(0);
    expect(offsetFor(-40)).toBe(40);
    expect(offsetFor(-REVEAL_PX)).toBe(REVEAL_PX);
    // Pulling 60px further moves the row 20.
    expect(offsetFor(-REVEAL_PX - 60)).toBe(REVEAL_PX + 20);
  });

  it("carries on from where an already-open row was", () => {
    expect(offsetFor(0, REVEAL_PX)).toBe(REVEAL_PX);
    // Dragging an open row back to the right closes it.
    expect(offsetFor(REVEAL_PX, REVEAL_PX)).toBe(0);
    expect(offsetFor(30, REVEAL_PX)).toBe(REVEAL_PX - 30);
  });

  it("keeps only one row open", () => {
    let first = 0;
    let second = 0;
    const closeFirst = () => first++;
    const closeSecond = () => second++;

    rows.opened(closeFirst);
    expect(first).toBe(0);

    rows.opened(closeSecond);
    // Opening the second shuts the first.
    expect(first).toBe(1);
    expect(second).toBe(0);

    // Closing a row that already went isn't another close.
    rows.closed(closeSecond);
    rows.opened(closeFirst);
    expect(second).toBe(0);
  });

  it("lands open or shut, never half way", () => {
    expect(settle(0)).toBe(0);
    expect(settle(REVEAL_PX * 0.39)).toBe(0);
    expect(settle(REVEAL_PX * 0.4)).toBe(REVEAL_PX);
    expect(settle(REVEAL_PX + 30)).toBe(REVEAL_PX);
  });
});
