import type { Nag, Rule, Schedule } from "./api/types";

const DAY = 86_400_000;

function startOfDay(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

/** Whole calendar days from `now`'s date to `ms`'s date (local time). */
export function dayDiff(ms: number, now: number): number {
  return Math.round((startOfDay(ms) - startOfDay(now)) / DAY);
}

export function clock(ms: number): string {
  return new Date(ms).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
}

/** "Today", "Tomorrow", "Yesterday", a weekday within the week, else "Sep 20". */
export function dayLabel(ms: number, now: number): string {
  const diff = dayDiff(ms, now);
  if (diff === 0) return "Today";
  if (diff === 1) return "Tomorrow";
  if (diff === -1) return "Yesterday";
  const d = new Date(ms);
  if (diff > 1 && diff < 7) return d.toLocaleDateString(undefined, { weekday: "long" });
  const sameYear = d.getFullYear() === new Date(now).getFullYear();
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric", year: sameYear ? undefined : "numeric" });
}

/** "9:00 AM" today, otherwise prefixed with the day: "Tomorrow 9:00 AM". */
export function when(ms: number, now: number): string {
  const diff = dayDiff(ms, now);
  return diff === 0 ? clock(ms) : `${dayLabel(ms, now)} ${clock(ms)}`;
}

/** "in 5 min", "in 2 h 10 min", "3 days ago". Rounds to the minute. */
export function relative(ms: number, now: number): string {
  const delta = ms - now;
  const future = delta >= 0;
  const minutes = Math.round(Math.abs(delta) / 60_000);
  let text: string;
  if (minutes < 1) return future ? "now" : "just now";
  if (minutes < 60) text = `${minutes} min`;
  else if (minutes < 60 * 24) {
    const h = Math.floor(minutes / 60);
    const m = minutes % 60;
    text = m ? `${h} h ${m} min` : `${h} h`;
  } else {
    const days = Math.round(minutes / (60 * 24));
    text = days === 1 ? "1 day" : `${days} days`;
  }
  return future ? `in ${text}` : `${text} ago`;
}

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

export function ordinal(n: number): string {
  const s = ["th", "st", "nd", "rd"];
  const v = n % 100;
  return `${n}${s[(v - 20) % 10] ?? s[v] ?? s[0]}`;
}

/** "09:30:00" -> "9:30 AM" in the user's locale. */
export function civilClock(time: string): string {
  const [h = 0, m = 0] = time.split(":").map(Number);
  const d = new Date(2000, 0, 1, h, m);
  return clock(d.getTime());
}

function times(list: string[]): string {
  const parts = list.map(civilClock);
  return parts.length <= 2 ? parts.join(" and ") : `${parts.slice(0, -1).join(", ")} and ${parts.at(-1)}`;
}

export function ruleSummary(rule: Rule): string {
  switch (rule.kind) {
    case "interval": {
      const unit = rule.unit === "minutes" ? "minute" : "hour";
      return rule.every === 1 ? `Every ${unit}` : `Every ${rule.every} ${unit}s`;
    }
    case "daily":
      return `${rule.every === 1 ? "Every day" : `Every ${rule.every} days`} at ${times(rule.times)}`;
    case "weekly": {
      const days = [...rule.weekdays].sort((a, b) => a - b);
      const label =
        days.join() === "1,2,3,4,5"
          ? "Weekdays"
          : days.join() === "6,7"
            ? "Weekends"
            : days.length === 7
              ? "Every day"
              : days.map((d) => WEEKDAYS[d - 1]).join(", ");
      const every = rule.every === 1 ? "" : `Every ${rule.every} weeks, `;
      return `${every}${label} at ${times(rule.times)}`;
    }
    case "monthly":
      return `${rule.every === 1 ? "Monthly" : `Every ${rule.every} months`} on the ${ordinal(rule.day)} at ${times(rule.times)}`;
    case "yearly":
      return `Every year on ${MONTHS[rule.month - 1]} ${rule.day} at ${times(rule.times)}`;
  }
}

export function nagSummary(nag: Nag): string {
  if (nag.mode === "once") return "Rings once";
  return nag.intervalMin === 1 ? "Nags every minute" : `Nags every ${nag.intervalMin} minutes`;
}

export function scheduleSummary(schedule: Schedule | null): string {
  if (!schedule) return "No schedule";
  switch (schedule.kind) {
    case "oneOff":
      return "Once";
    case "recurring":
      return schedule.mode === "afterCompletion"
        ? `${ruleSummary(schedule.recurrence.rule)}, counted from when you finish`
        : ruleSummary(schedule.recurrence.rule);
    case "timer":
      return "Timer";
  }
}
