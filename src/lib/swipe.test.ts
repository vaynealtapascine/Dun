import { describe, expect, it } from "vitest";
import { axisOf, commitAt, offsetFor, REVEAL_PX, rows, settle, Velocity } from "./swipe";

describe("swipe gestures", () => {
  it("waits before deciding which way a gesture is going", () => {
    expect(axisOf(3, 2)).toBe(null);
    expect(axisOf(-20, 3)).toBe("x");
    // A finger scrolling the list belongs to the list.
    expect(axisOf(-6, 30)).toBe("y");
    expect(axisOf(-20, 25)).toBe("y");
  });

  it("only opens leftwards, and resists past the row's own width", () => {
    expect(offsetFor(30)).toBe(0);
    expect(offsetFor(0)).toBe(0);
    expect(offsetFor(-40)).toBe(40);
    expect(offsetFor(-REVEAL_PX)).toBe(REVEAL_PX);
    // Past the button the row keeps following the finger, toward a full swipe.
    expect(offsetFor(-200, 0, 360)).toBe(200);
    // Pulling 60px beyond the row's width moves it 20.
    expect(offsetFor(-420, 0, 360)).toBe(380);
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

  it("lets a flick decide before the distance does", () => {
    // A quick flick opens a row that has barely moved…
    expect(settle(20, 0.8)).toBe(REVEAL_PX);
    // …and a flick back shuts one that is mostly open.
    expect(settle(REVEAL_PX, -0.8)).toBe(0);
  });

  it("archives on a long swipe, but never by accident on a phone", () => {
    // Narrow rows still need twice the button's travel.
    expect(commitAt(300)).toBe(REVEAL_PX * 2);
    expect(commitAt(360)).toBeCloseTo(198);
    expect(commitAt(900)).toBeCloseTo(495);
    expect(settle(197, 0, 360)).toBe(REVEAL_PX);
    expect(settle(199, 0, 360)).toBe("archive");
    // A flick back doesn't undo a swipe that has already gone far enough.
    expect(settle(300, -1, 360)).toBe("archive");
  });

  it("measures how fast the finger was going when it let go", () => {
    const v = new Velocity();
    v.add(300, 0);
    v.add(280, 10);
    v.add(240, 30);
    expect(v.value).toBeCloseTo(2);
    // A finger that paused before lifting meant to stop.
    v.add(240, 200);
    expect(v.value).toBe(0);
  });
});
