import { describe, expect, it } from "vitest";
import { displayAccelerator, fromKeyEvent, hasModifier } from "./accelerator";

const key = (code: string, mods: Partial<Record<"ctrlKey" | "altKey" | "shiftKey" | "metaKey", boolean>> = {}) => ({
  code,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...mods,
});

describe("fromKeyEvent", () => {
  it("builds accelerators from codes, not layout-dependent keys", () => {
    expect(fromKeyEvent(key("KeyN", { ctrlKey: true, altKey: true }))).toBe("CommandOrControl+Alt+N");
    expect(fromKeyEvent(key("Space", { ctrlKey: true, shiftKey: true }))).toBe("CommandOrControl+Shift+Space");
    expect(fromKeyEvent(key("Digit1", { metaKey: true, altKey: true }))).toBe("Alt+Super+1");
    expect(fromKeyEvent(key("F9"))).toBe("F9");
  });

  it("waits while only modifiers are down and ignores unusable keys", () => {
    expect(fromKeyEvent(key("ControlLeft", { ctrlKey: true }))).toBeNull();
    expect(fromKeyEvent(key("AltRight", { altKey: true }))).toBeNull();
    expect(fromKeyEvent(key("IntlBackslash", { ctrlKey: true }))).toBeNull();
  });

  it("knows whether a modifier is present", () => {
    expect(hasModifier("CommandOrControl+Alt+N")).toBe(true);
    expect(hasModifier("F9")).toBe(false);
  });
});

describe("displayAccelerator", () => {
  it.each([
    ["CommandOrControl+Alt+N", "Ctrl + Alt + N"],
    ["Ctrl+Shift+KeyK", "Ctrl + Shift + K"],
    ["Alt+Super+Digit1", "Alt + Win + 1"],
    ["CommandOrControl+ArrowUp", "Ctrl + ↑"],
    ["", "None"],
  ])("%s -> %s", (input, shown) => {
    expect(displayAccelerator(input)).toBe(shown);
  });
});
