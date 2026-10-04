import type { StatusView } from "./api/types";

export type ReminderCountdown = {
  years: number;
  months: number;
  days: number;
  hours: number;
  minutes: number;
  seconds: number;
  calendar: string;
  clock: string;
  spoken: string;
};

export type ReminderCountdownTarget = { at: number; label: string; overdue: boolean };

/** Select the occurrence to count, rather than a reminder's next repeated alert. */
export function selectReminderCountdown(status: StatusView, now: number): ReminderCountdownTarget | null {
  if (status.kind === "upcoming") {
    return { at: Math.max(now, status.at), label: status.at > now ? "Due in" : "Due now", overdue: false };
  }
  if (status.kind !== "due") return null;
  if (status.snoozed) {
    return {
      at: Math.max(now, status.ringsAt),
      label: status.ringsAt > now ? "Alerts resume in" : "Resuming alerts",
      overdue: false,
    };
  }
  const occurrence = status.firstMissed ?? status.occurrence;
  return {
    at: Math.min(now, occurrence),
    label: occurrence < now ? "Overdue by" : "Due now",
    overdue: occurrence < now,
  };
}

const DAY_MS = 86_400_000;
const pad = (value: number) => String(value).padStart(2, "0");

function daysInMonth(year: number, month: number): number {
  const end = new Date(0);
  end.setUTCFullYear(year, month + 1, 0);
  return end.getUTCDate();
}

/** Preserve local clock time and clamp dates such as Jan 31 to February's last day. */
function addMonths(date: Date, months: number): Date {
  const monthIndex = date.getMonth() + months;
  const year = date.getFullYear() + Math.floor(monthIndex / 12);
  const month = ((monthIndex % 12) + 12) % 12;
  const shifted = new Date(date.getTime());
  shifted.setFullYear(year, month, Math.min(date.getDate(), daysInMonth(year, month)));
  return shifted;
}

function calendarDay(date: Date): number {
  const civil = new Date(0);
  civil.setUTCFullYear(date.getFullYear(), date.getMonth(), date.getDate());
  civil.setUTCHours(0, 0, 0, 0);
  return civil.getTime() / DAY_MS;
}

function addDays(date: Date, days: number): Date {
  const shifted = new Date(date.getTime());
  shifted.setDate(date.getDate() + days);
  return shifted;
}

/**
 * Nonnegative local-calendar duration between two instants. Years, months and
 * days use civil dates; the remaining clock uses actual elapsed time, so a DST
 * calendar day can contain 23 or 25 hours. Future seconds round upward; elapsed
 * overdue seconds round downward so their displayed duration never runs ahead.
 */
export function splitReminderCountdown(from: number, to: number): ReminderCountdown {
  const first = Math.min(from, to);
  const last = Math.max(from, to);
  const start = new Date(first);
  // Round relative to the start, allowing whole-second carries into calendar days.
  const secondsBetween = (last - first) / 1000;
  const wholeSeconds = to >= from ? Math.ceil(secondsBetween) : Math.floor(secondsBetween);
  const end = new Date(first + wholeSeconds * 1000);
  let years = 0;
  let months = 0;
  let days = 0;
  let totalSeconds = 0;

  if (Number.isFinite(start.getTime()) && Number.isFinite(end.getTime())) {
    // Anchor every month anniversary to the original day. Adding years first
    // would permanently clamp Feb 29 and could produce a remainder of 12 months.
    let totalMonths = (end.getFullYear() - start.getFullYear()) * 12 + end.getMonth() - start.getMonth();
    let cursor = addMonths(start, totalMonths);
    if (cursor.getTime() > end.getTime()) cursor = addMonths(start, --totalMonths);
    years = Math.floor(totalMonths / 12);
    months = totalMonths % 12;

    days = calendarDay(end) - calendarDay(cursor);
    let dayCursor = addDays(cursor, days);
    if (dayCursor.getTime() > end.getTime()) dayCursor = addDays(cursor, --days);
    totalSeconds = Math.max(0, Math.round((end.getTime() - dayCursor.getTime()) / 1000));
  }

  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const parts: [number, string][] = [
    [years, "year"], [months, "month"], [days, "day"],
    [hours, "hour"], [minutes, "minute"], [seconds, "second"],
  ];
  const spoken = parts.filter(([value]) => value > 0).map(([value, unit]) => `${value} ${unit}${value === 1 ? "" : "s"}`).join(", ") || "0 seconds";

  return {
    years, months, days, hours, minutes, seconds,
    calendar: `${pad(years)}:${pad(months)}:${pad(days)}`,
    clock: `${pad(hours)}:${pad(minutes)}:${pad(seconds)}`,
    spoken,
  };
}

/**
 * Split the `YYy MMm DDd HH:MM:SS` readout where its first nonzero digit
 * begins. Everything before it, including padding zeros, suffixes and colons,
 * is leading filler; an all-zero readout keeps its seconds legible.
 */
export function splitReminderReadout(value: ReminderCountdown): { leading: string; rest: string } {
  const readout = `${pad(value.years)}y ${pad(value.months)}m ${pad(value.days)}d ${value.clock}`;
  const first = readout.search(/[1-9]/);
  const split = first < 0 ? readout.length - 2 : first;
  return { leading: readout.slice(0, split), rest: readout.slice(split) };
}
