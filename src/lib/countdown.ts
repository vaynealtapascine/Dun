import type { Countdown, CountdownDisplay, CountdownDraft, CountUnit } from "./api/types";
import { addDays, addMonths } from "./reminderCountdown";

/** Largest first. */
export const UNITS: CountUnit[] = ["years", "months", "weeks", "days", "hours", "minutes", "seconds"];

const LONG: Record<CountUnit, [string, string]> = {
  years: ["year", "years"],
  months: ["month", "months"],
  weeks: ["week", "weeks"],
  days: ["day", "days"],
  hours: ["hour", "hours"],
  minutes: ["minute", "minutes"],
  seconds: ["second", "seconds"],
};
const SHORT: Record<CountUnit, string> = { years: "y", months: "mo", weeks: "w", days: "d", hours: "h", minutes: "m", seconds: "s" };
const UNIT_MS: Partial<Record<CountUnit, number>> = { hours: 3_600_000, minutes: 60_000, seconds: 1000 };

export const DEFAULT_DISPLAY: CountdownDisplay = {
  largest: "days",
  smallest: "days",
  units: null,
  longUnits: true,
  size: "medium",
  icon: null,
  color: null,
  fill: "solid",
  showDate: true,
  showOrdinal: true,
  showProgress: false,
  showTotal: false,
  countFirstDay: false,
};

export function emptyDraft(today: string): CountdownDraft {
  return { title: "", notes: "", date: today, time: null, repeat: "none", direction: "auto", display: { ...DEFAULT_DISPLAY }, pinned: false };
}

export function unitLabel(unit: CountUnit, value: number, long: boolean): string {
  return long ? LONG[unit][value === 1 ? 0 : 1] : SHORT[unit];
}

/** Read older saved ranges until the user chooses individual units. */
export function displayUnits(display: CountdownDisplay): CountUnit[] {
  const selected = UNITS.filter((unit) => display.units?.includes(unit));
  if (selected.length) return selected;
  const range = UNITS.slice(UNITS.indexOf(display.largest), UNITS.indexOf(display.smallest) + 1);
  return range.length ? range : ["days"];
}

/** The civil date and time as a local instant. */
function base(c: Pick<Countdown, "date" | "time">): Date {
  const [y = 1970, mo = 1, d = 1] = c.date.split("-").map(Number);
  const [h = 0, mi = 0, s = 0] = (c.time ?? "00:00:00").split(":").map(Number);
  return new Date(y, mo - 1, d, h, mi, Math.floor(s));
}

/** Occurrence `k` (0 is the date itself), anchored to the original so Jan 31 stays the 31st where it can. */
export function occurrence(c: Pick<Countdown, "date" | "time" | "repeat">, k: number): Date {
  const start = base(c);
  switch (c.repeat) {
    case "weekly": return addDays(start, 7 * k);
    case "monthly": return addMonths(start, k);
    case "yearly": return addMonths(start, 12 * k);
    default: return start;
  }
}

/** Index of the last occurrence at or before `ref`, or -1 if the first is still ahead. */
function lastIndexAtOrBefore(c: Pick<Countdown, "date" | "time" | "repeat">, ref: number): number {
  const start = base(c).getTime();
  if (ref < start) return -1;
  if (c.repeat === "none") return 0;
  const period = c.repeat === "weekly" ? 7 * 86_400_000 : c.repeat === "monthly" ? 30.44 * 86_400_000 : 365.25 * 86_400_000;
  let k = Math.max(0, Math.floor((ref - start) / period) - 1);
  while (occurrence(c, k + 1).getTime() <= ref) k++;
  while (k > 0 && occurrence(c, k).getTime() > ref) k--;
  return k;
}

export type CountPart = { unit: CountUnit; value: number };

export type CountReading = {
  /** What is being counted: time left until `target`, or time gone since it. */
  mode: "until" | "since";
  target: Date;
  /** Which occurrence `target` is; 0 is the date itself. */
  index: number;
  /** The target is today (or, with a time, now). */
  isNow: boolean;
  /** The chosen direction couldn't apply: counting down to a date that has gone, or up from one still ahead. */
  flipped: boolean;
  parts: CountPart[];
  /** How far through the current repeat, 0–1, when there is a cycle to be through. */
  progress: number | null;
  /** Independent of the current occurrence, so an anniversary never resets it. */
  total: { start: Date; parts: CountPart[]; started: boolean } | null;
};

/**
 * Splits a span into enabled units (or a legacy largest-to-smallest range).
 * Skipped units flow into the next enabled unit, so years/months/days keeps
 * the remaining days rather than consuming them as hidden weeks. Calendar
 * units respect local month lengths and leap days; clocks use elapsed time.
 */
export function splitSpan(from: Date, to: Date, selected: CountUnit[] | CountUnit, smallest?: CountUnit): CountPart[] {
  const units = Array.isArray(selected)
    ? UNITS.filter((unit) => selected.includes(unit))
    : UNITS.slice(UNITS.indexOf(selected), UNITS.indexOf(smallest ?? selected) + 1);
  const parts: CountPart[] = [];
  let cursor = from;
  if (units.includes("years") || units.includes("months")) {
    let months = (to.getFullYear() - from.getFullYear()) * 12 + to.getMonth() - from.getMonth();
    while (months > 0 && addMonths(from, months) > to) months--;
    if (units.includes("years")) parts.push({ unit: "years", value: Math.floor(months / 12) });
    if (units.includes("months")) parts.push({ unit: "months", value: units.includes("years") ? months % 12 : months });
    else months = Math.floor(months / 12) * 12;
    cursor = addMonths(from, months);
  }
  if (units.includes("weeks") || units.includes("days")) {
    let days = Math.round((Date.UTC(to.getFullYear(), to.getMonth(), to.getDate()) - Date.UTC(cursor.getFullYear(), cursor.getMonth(), cursor.getDate())) / 86_400_000);
    while (days > 0 && addDays(cursor, days) > to) days--;
    if (units.includes("weeks")) parts.push({ unit: "weeks", value: Math.floor(days / 7) });
    if (units.includes("days")) parts.push({ unit: "days", value: units.includes("weeks") ? days % 7 : days });
    else days = Math.floor(days / 7) * 7;
    cursor = addDays(cursor, days);
  }
  let rest = Math.max(0, to.getTime() - cursor.getTime());
  for (const unit of units) {
    const size = UNIT_MS[unit];
    if (!size) continue;
    const value = Math.floor(rest / size);
    parts.push({ unit, value });
    rest -= value * size;
  }
  return parts;
}

/** What a countdown reads at `now`. */
export function readCountdown(c: Countdown, now: number): CountReading {
  const { display } = c;
  const units = displayUnits(display);
  const subDay = units.some((unit) => UNIT_MS[unit] !== undefined);
  // Counting in whole days, an occurrence is the whole of its day: today is
  // "today", not "365 days until next year's". Finer units tick from its time
  // (midnight without one).
  const dayOnly = !subDay;
  const ref = dayOnly ? new Date(new Date(now).setHours(0, 0, 0, 0)).getTime() : now;
  const calc = dayOnly ? { ...c, time: null } : c;

  const last = lastIndexAtOrBefore(calc, ref);
  const lastAt = last >= 0 ? occurrence(calc, last) : null;
  const nextIndex = lastAt?.getTime() === ref ? last : last + 1;
  const nextAt = c.repeat === "none" && nextIndex > 0 ? null : occurrence(calc, nextIndex);

  const today = lastAt !== null && lastAt.getTime() === ref;
  // On the day, "auto" reads as the start of a count when its first day counts as day 1.
  const auto = nextAt && !(today && display.countFirstDay) ? "until" : "since";
  let mode: "until" | "since" = c.direction === "auto" ? auto : c.direction;
  let flipped = false;
  if (mode === "until" && !nextAt) { mode = "since"; flipped = true; }
  if (mode === "since" && !lastAt) { mode = "until"; flipped = true; }

  const counted = mode === "until" ? nextAt! : lastAt!;
  const index = mode === "until" ? nextIndex : last;
  const isNow = counted.getTime() === ref || (!dayOnly && Math.abs(counted.getTime() - now) < 1000);

  let parts: CountPart[];
  if (mode === "until") {
    parts = splitSpan(new Date(Math.min(ref, counted.getTime())), counted, units);
  } else {
    // Counting the date itself as day 1 shifts the whole reading by one day.
    const end = display.countFirstDay ? addDays(new Date(ref), 1) : new Date(ref);
    parts = splitSpan(counted, end < counted ? counted : end, units);
  }

  let progress: number | null = null;
  if (c.repeat !== "none" && last >= 0) {
    const from = occurrence(c, last).getTime();
    const to = occurrence(c, last + 1).getTime();
    progress = Math.min(1, Math.max(0, (now - from) / (to - from)));
  }
  let total: CountReading["total"] = null;
  if (display.showTotal && c.repeat !== "none") {
    const start = base(calc);
    const started = ref >= start.getTime();
    const end = started && display.countFirstDay ? addDays(new Date(ref), 1) : new Date(ref);
    total = { start: base(c), started, parts: splitSpan(start, end < start ? start : end, units) };
  }
  return { mode, target: occurrence(c, index), index, isNow, flipped, parts, progress, total };
}

/** Drops empty leading units (and empty ones between), keeping at least the smallest. */
export function visibleParts(parts: CountPart[]): CountPart[] {
  const shown = parts.filter((p) => p.value !== 0);
  return shown.length ? shown : parts.slice(-1);
}

export function readingText(parts: CountPart[], long: boolean): string {
  return visibleParts(parts)
    .map((p) => (long ? `${p.value} ${unitLabel(p.unit, p.value, true)}` : `${p.value}${unitLabel(p.unit, p.value, false)}`))
    .join(" ");
}

/** Pinned first; then what's coming, soonest first; then what's gone, latest first. */
export function sortCountdowns(list: Countdown[], now: number): { countdown: Countdown; reading: CountReading }[] {
  return list
    .map((countdown) => ({ countdown, reading: readCountdown(countdown, now) }))
    .sort((a, b) => {
      if (a.countdown.pinned !== b.countdown.pinned) return a.countdown.pinned ? -1 : 1;
      if (a.reading.mode !== b.reading.mode) return a.reading.mode === "until" ? -1 : 1;
      const diff = a.reading.target.getTime() - b.reading.target.getTime();
      return (a.reading.mode === "until" ? diff : -diff) || a.countdown.title.localeCompare(b.countdown.title);
    });
}
