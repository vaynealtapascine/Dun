// Global-shortcut strings in the format the Rust side parses
// ("CommandOrControl+Alt+N"), built from and shown for keyboard events.

type KeyLike = Pick<KeyboardEvent, "ctrlKey" | "altKey" | "shiftKey" | "metaKey" | "code">;

const MODIFIER_CODES = /^(Control|Alt|Shift|Meta|OS)(Left|Right)?$/;

/** Keys that can stand on their own after the modifiers. */
function keyName(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  const named = [
    "Space", "Enter", "Tab", "Backspace", "Delete", "Insert", "Home", "End", "PageUp", "PageDown",
    "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Minus", "Equal", "BracketLeft", "BracketRight",
    "Backslash", "Semicolon", "Quote", "Comma", "Period", "Slash", "Backquote",
  ];
  return named.includes(code) ? code : null;
}

/** The accelerator for a key press; `null` while only modifiers are held or for keys a shortcut can't use. */
export function fromKeyEvent(e: KeyLike): string | null {
  if (MODIFIER_CODES.test(e.code)) return null;
  const key = keyName(e.code);
  if (!key) return null;
  const mods = [e.ctrlKey && "CommandOrControl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(
    Boolean,
  ) as string[];
  return [...mods, key].join("+");
}

export function hasModifier(accelerator: string): boolean {
  return accelerator.split("+").length > 1;
}

const DISPLAY: Record<string, string> = {
  COMMANDORCONTROL: "Ctrl",
  CMDORCTRL: "Ctrl",
  CONTROL: "Ctrl",
  CTRL: "Ctrl",
  ALT: "Alt",
  OPTION: "Alt",
  SHIFT: "Shift",
  SUPER: "Win",
  META: "Win",
  CMD: "Win",
  COMMAND: "Win",
  ARROWUP: "↑",
  ARROWDOWN: "↓",
  ARROWLEFT: "←",
  ARROWRIGHT: "→",
};

/** "CommandOrControl+Alt+N" → "Ctrl + Alt + N". */
export function displayAccelerator(accelerator: string): string {
  if (!accelerator.trim()) return "None";
  return accelerator
    .split("+")
    .map((part) => DISPLAY[part.toUpperCase()] ?? part.replace(/^Key([A-Z])$/, "$1").replace(/^Digit(\d)$/, "$1"))
    .join(" + ");
}
