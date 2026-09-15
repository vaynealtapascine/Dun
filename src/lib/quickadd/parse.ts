import * as chrono from "chrono-node";
import type { Nag, RepeatMode, Rule, Schedule, Tag } from "../api/types";

/**
 * Turns one line of text into a schedule draft:
 *
 * - "Laundry in 45m", "Tea for 4 minutes"          → timer
 * - "Pay rent tomorrow 9am", "Call mom friday"      → one-off (date without a time uses `dateOnlyTime`)
 * - "Stretch every weekday at 3pm"                  → weekly on Mon–Fri
 * - "Meds every day at 9am and 9pm"                 → daily at two times
 * - "Stand up every 45 minutes"                     → interval
 * - "Rent monthly on the 1st", "Birthday every year on Mar 3"
 * - "... after I finish"                            → repeat counted from completion
 * - "#home" picks a tag; "nag 5m" / "ring once" sets nagging
 *
 * Whatever isn't recognised stays in the title. Everything is local time.
 */

export type ParsedKind = "once" | "repeat" | "timer";

export type Parsed = {
  title: string;
  kind: ParsedKind;
  /** Null when no time or duration was found (the form asks). */
  schedule: Schedule | null;
  tagId: string | null;
  /** A #tag that didn't match an existing tag. */
  unknownTag: string | null;
  nag: Nag | null;
};

export type ParseOptions = {
  now: number;
  tags: Tag[];
  /** "HH:MM:SS" used when only a date was given. */
  dateOnlyTime: string;
};

const pad = (n: number) => String(n).padStart(2, "0");
const civil = (h: number, m: number) => `${pad(h)}:${pad(m)}:00`;

const DAY_NAMES: Record<string, number> = {
  mon: 1, monday: 1, tue: 2, tues: 2, tuesday: 2, wed: 3, weds: 3, wednesday: 3,
  thu: 4, thur: 4, thurs: 4, thursday: 4, fri: 5, friday: 5, sat: 6, saturday: 6, sun: 7, sunday: 7,
};
const MONTH_NAMES: Record<string, number> = {
  jan: 1, january: 1, feb: 2, february: 2, mar: 3, march: 3, apr: 4, april: 4, may: 5, jun: 6, june: 6,
  jul: 7, july: 7, aug: 8, august: 8, sep: 9, sept: 9, september: 9, oct: 10, october: 10,
  nov: 11, november: 11, dec: 12, december: 12,
};
const DAY = "(?:mon|tues?|wed(?:s|nesday)?|thu(?:rs?)?|fri|sat|sun)[a-z]*";
const MONTH = "(?:jan|feb|mar|apr|may|jun|jul|aug|sept?|oct|nov|dec)[a-z]*";
const NUM_WORDS: Record<string, number> = { a: 1, an: 1, one: 1, two: 2, three: 3, four: 4, five: 5, ten: 10, fifteen: 15, twenty: 20, thirty: 30 };

/** A mutable view of the input so recognised pieces can be cut out. */
class Text {
  constructor(public s: string) {}
  take(re: RegExp): RegExpMatchArray | null {
    const m = this.s.match(re);
    if (m && m.index !== undefined) {
      this.s = `${this.s.slice(0, m.index)} ${this.s.slice(m.index + m[0].length)}`;
    }
    return m;
  }
}

function num(word: string | undefined, fallback = 1): number {
  if (!word) return fallback;
  const w = word.toLowerCase();
  return NUM_WORDS[w] ?? (Number.parseFloat(w) || fallback);
}

function unitMs(unit: string): number {
  const u = unit.toLowerCase();
  if (u.startsWith("h")) return 3_600_000;
  if (u.startsWith("s")) return 1_000;
  return 60_000;
}

/** "9", "9am", "9:30 pm", "21:00", "noon", "midnight" -> "HH:MM:SS". */
export function parseClock(text: string): string | null {
  const t = text.trim().toLowerCase();
  if (t === "noon") return civil(12, 0);
  if (t === "midnight") return civil(0, 0);
  const m = t.match(/^(\d{1,2})(?:[:.](\d{2}))?\s*(am|pm|a|p)?$/);
  if (!m) return null;
  let h = Number(m[1]);
  const min = m[2] ? Number(m[2]) : 0;
  const ampm = m[3]?.[0];
  if (h > 23 || min > 59) return null;
  if (ampm === "p" && h < 12) h += 12;
  if (ampm === "a" && h === 12) h = 0;
  return civil(h, min);
}

const CLOCK = "(?:\\d{1,2}(?:[:.]\\d{2})?\\s*(?:am|pm|a\\.m\\.|p\\.m\\.)?|noon|midnight)";

/** Pulls "at 9am and 9pm" / "at 15:00" out of the text. Bare numbers need "at". */
function takeTimes(text: Text): string[] {
  const withAt = text.take(new RegExp(`\\b(?:at|@)\\s+(${CLOCK}(?:\\s*(?:,|and|&)\\s*${CLOCK})*)`, "i"));
  const withoutAt = withAt
    ? null
    : text.take(new RegExp(`\\b((?:\\d{1,2}(?:[:.]\\d{2})?\\s*(?:am|pm)|\\d{1,2}[:.]\\d{2}|noon|midnight)(?:\\s*(?:,|and|&)\\s*${CLOCK})*)\\b`, "i"));
  const list = (withAt ?? withoutAt)?.[1];
  if (!list) return [];
  const times = list
    .split(/\s*(?:,|and|&)\s*/i)
    .map((p) => parseClock(p.replace(/\./g, "")))
    .filter((t): t is string => t != null);
  return [...new Set(times)].sort();
}

function takeDays(text: Text): number[] | null {
  if (text.take(/\b(?:every\s+|each\s+|on\s+)?(?:week\s?days?|workdays?)\b/i)) return [1, 2, 3, 4, 5];
  if (text.take(/\b(?:every\s+|each\s+|on\s+)?weekends?\b/i)) return [6, 7];
  const m = text.take(new RegExp(`\\b(?:every|each|on)\\s+(${DAY}(?:\\s*(?:,|and|&|\\/)\\s*${DAY})*)\\b`, "i"));
  if (!m?.[1]) return null;
  const days = m[1]
    .toLowerCase()
    .split(/\s*(?:,|and|&|\/)\s*/)
    .map((d) => DAY_NAMES[d] ?? DAY_NAMES[d.slice(0, 3)])
    .filter((d): d is number => d != null);
  return days.length ? [...new Set(days)].sort((a, b) => a - b) : null;
}

function startDate(now: number): string {
  const d = new Date(now);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

function recurring(rule: Rule, mode: RepeatMode, now: number): Schedule {
  const start =
    rule.kind === "interval"
      ? (() => {
          // Interval repeats start on the next whole minute.
          const d = new Date(Math.ceil((now + 1) / 60_000) * 60_000);
          return `${startDate(d.getTime())}T${pad(d.getHours())}:${pad(d.getMinutes())}:00`;
        })()
      : `${startDate(now)}T00:00:00`;
  return { kind: "recurring", recurrence: { rule, start, tz: null }, mode, effectiveFrom: 0 };
}

function cleanTitle(s: string): string {
  return s
    .replace(/\s+/g, " ")
    .trim()
    .replace(/^(?:remind me to|remind me|to)\s+/i, "")
    .replace(/[\s,;:–-]+$/g, "")
    .replace(/^[\s,;:–-]+/g, "")
    .replace(/\s+(?:at|on|in|for|every|by|to)$/i, "")
    .trim();
}

export function parseQuickAdd(input: string, opts: ParseOptions): Parsed {
  const text = new Text(` ${input.trim()} `);
  const out: Parsed = { title: "", kind: "once", schedule: null, tagId: null, unknownTag: null, nag: null };

  // #tag
  const tag = text.take(/(?:^|\s)#([\p{L}\p{N}_-]+)/u);
  if (tag?.[1]) {
    const found = opts.tags.find((t) => t.name.toLowerCase() === tag[1]!.toLowerCase());
    if (found) out.tagId = found.id;
    else out.unknownTag = tag[1];
  }

  // Nagging modifiers
  if (text.take(/\b(?:ring once|no nag(?:ging)?|don'?t nag)\b/i)) out.nag = { mode: "once" };
  const nag = text.take(/\bnag(?:\s+(?:me\s+)?every)?\s+(\d+)\s*(m|mins?|minutes?|h|hours?)\b/i);
  if (nag) {
    const minutes = Math.round((num(nag[1]) * unitMs(nag[2]!)) / 60_000);
    out.nag = { mode: "repeat", intervalMin: Math.min(1440, Math.max(1, minutes)) };
  }

  const mode: RepeatMode = text.take(/\b(?:after\s+(?:i'?m\s+|i\s+)?(?:done|finish(?:ed)?)|from\s+(?:when\s+i\s+finish|completion))\b/i)
    ? "afterCompletion"
    : "fromSchedule";

  // Interval repeats: "every 45 minutes", "every hour", "hourly"
  const interval =
    text.take(/\bevery\s+(\d+|a|an|one|two|three|four|five|ten|fifteen|twenty|thirty)?\s*(minutes?|mins?|m|hours?|hrs?|h)\b/i) ??
    (text.take(/\bhourly\b/i) ? ["hourly", "1", "hour"] : null);
  if (interval) {
    const every = Math.max(1, Math.round(num(interval[1])));
    const unit = unitMs(interval[2]!) === 3_600_000 ? "hours" : "minutes";
    out.kind = "repeat";
    out.schedule = recurring({ kind: "interval", every, unit }, mode, opts.now);
    out.title = cleanTitle(text.s);
    return out;
  }

  // Calendar repeats
  const today = new Date(opts.now);
  const days = takeDays(text);
  const weeks = text.take(/\b(?:every\s+(\d+)\s+weeks?|every\s+week|weekly)\b/i);
  const everyDays = text.take(/\b(?:every\s+(\d+|other)\s+days?|every\s*day|each\s+day|daily)\b/i);
  const monthly = text.take(/\b(?:monthly|every\s+(\d+\s+)?months?)(?:\s+on)?(?:\s+the)?(?:\s+(\d{1,2})(?:st|nd|rd|th)?)?\b/i);
  const yearly = text.take(
    new RegExp(`\\b(?:yearly|annually|every\\s+year)(?:\\s+on)?(?:\\s+(${MONTH})\\s+(\\d{1,2})(?:st|nd|rd|th)?|\\s+(\\d{1,2})(?:st|nd|rd|th)?\\s+(?:of\\s+)?(${MONTH}))?\\b`, "i"),
  );

  if (days || weeks || everyDays || monthly || yearly) {
    const times = takeTimes(text);
    const at = times.length ? times : [opts.dateOnlyTime];
    let rule: Rule;
    if (yearly) {
      const monthName = (yearly[1] ?? yearly[4])?.toLowerCase();
      const month = monthName ? (MONTH_NAMES[monthName] ?? MONTH_NAMES[monthName.slice(0, 3)] ?? today.getMonth() + 1) : today.getMonth() + 1;
      const day = Number(yearly[2] ?? yearly[3] ?? today.getDate());
      rule = { kind: "yearly", month, day, times: at };
    } else if (monthly) {
      const every = monthly[1] ? Number(monthly[1].trim()) : 1;
      rule = { kind: "monthly", every, day: Number(monthly[2] ?? today.getDate()), times: at };
    } else if (days || weeks) {
      const every = weeks?.[1] ? Number(weeks[1]) : 1;
      rule = { kind: "weekly", every, weekdays: days ?? [((today.getDay() + 6) % 7) + 1], times: at };
    } else {
      const n = everyDays?.[1];
      const every = n === "other" ? 2 : n ? Number(n) : 1;
      rule = { kind: "daily", every, times: at };
    }
    out.kind = "repeat";
    out.schedule = recurring(rule, mode, opts.now);
    out.title = cleanTitle(text.s);
    return out;
  }

  // Timers: "in 45m", "in 1h 30m", "for 4 minutes", "in an hour", "45 min timer"
  const dur = text.take(
    /\b(?:in|for|timer(?:\s+for)?)\s+((?:(?:\d+(?:\.\d+)?|a|an|one|two|three|four|five|ten|fifteen|twenty|thirty|half an?)\s*(?:hours?|hrs?|h|minutes?|mins?|m|seconds?|secs?|s)\b\s*(?:and\s*)?)+)/i,
  ) ?? text.take(/\b((?:\d+(?:\.\d+)?\s*(?:hours?|hrs?|h|minutes?|mins?|m|seconds?|secs?|s)\b\s*)+)timer\b/i);
  if (dur?.[1]) {
    let ms = 0;
    const partRe = /(\d+(?:\.\d+)?|a|an|one|two|three|four|five|ten|fifteen|twenty|thirty|half an?)\s*(hours?|hrs?|h|minutes?|mins?|m|seconds?|secs?|s)\b/gi;
    for (const part of dur[1].matchAll(partRe)) {
      const amount = /^half/i.test(part[1]!) ? 0.5 : num(part[1]);
      ms += amount * unitMs(part[2]!);
    }
    if (ms >= 1000) {
      out.kind = "timer";
      out.schedule = { kind: "timer", durationMs: Math.round(ms) };
      out.title = cleanTitle(text.s);
      return out;
    }
  }

  // One-off date/time via chrono
  const results = chrono.parse(text.s, new Date(opts.now), { forwardDate: true });
  const first = results[0];
  if (first) {
    const d = first.start.date();
    if (!first.start.isCertain("hour")) {
      const [h = 9, m = 0] = opts.dateOnlyTime.split(":").map(Number);
      d.setHours(h, m, 0, 0);
      // "today" with a default time that has already passed means tomorrow.
      if (d.getTime() <= opts.now) d.setDate(d.getDate() + 1);
    }
    text.s = `${text.s.slice(0, first.index)} ${text.s.slice(first.index + first.text.length)}`;
    out.schedule = { kind: "oneOff", due: d.getTime() };
  }
  out.title = cleanTitle(text.s);
  return out;
}
