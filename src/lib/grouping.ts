import type { ItemView, Tag } from "./api/types";
import { dayDiff } from "./format";

export type GroupKey = "ringing" | "overdue" | "today" | "tomorrow" | "later";

export const GROUP_TITLES: Record<GroupKey, string> = {
  ringing: "Ringing",
  overdue: "Overdue",
  today: "Today",
  tomorrow: "Tomorrow",
  later: "Later",
};

export type Group = { key: GroupKey; items: ItemView[] };

/** When an item next needs attention, for sorting. */
export function sortTime(item: ItemView): number {
  switch (item.status.kind) {
    case "due":
      return item.status.ringsAt;
    case "upcoming":
      return item.status.at;
    default:
      return Number.MAX_SAFE_INTEGER;
  }
}

/**
 * Reminders tab groups. Ringing: due and alerting. Overdue: due but quiet for
 * now (snoozed, or held for quiet hours, mute or the phone). Upcoming items
 * fall into Today / Tomorrow / Later by local date. Timers and finished items
 * are left out.
 */
export function groupReminders(items: ItemView[], now: number): Group[] {
  const buckets: Record<GroupKey, ItemView[]> = { ringing: [], overdue: [], today: [], tomorrow: [], later: [] };
  for (const item of items) {
    if (item.kind === "timer") continue;
    const s = item.status;
    if (s.kind === "due") {
      const quiet = s.snoozed || item.ring?.held != null;
      buckets[quiet ? "overdue" : "ringing"].push(item);
    } else if (s.kind === "upcoming") {
      const diff = dayDiff(s.at, now);
      buckets[diff <= 0 ? "today" : diff === 1 ? "tomorrow" : "later"].push(item);
    }
  }
  return (Object.keys(buckets) as GroupKey[])
    .map((key) => ({ key, items: buckets[key].sort((a, b) => sortTime(a) - sortTime(b)) }))
    .filter((g) => g.items.length > 0);
}

export type TimerBuckets = {
  ringing: ItemView[];
  running: ItemView[];
  paused: ItemView[];
  idle: ItemView[];
  /** Expired first, then running and paused together in a stable order. */
  active: ItemView[];
};

/** Newest creation first; legacy timers without creation metadata use their ID. */
function compareTimerCreation(a: ItemView, b: ItemView): number {
  const aCreated = a.created?.at;
  const bCreated = b.created?.at;
  if (aCreated !== bCreated) {
    if (aCreated == null) return 1;
    if (bCreated == null) return -1;
    return bCreated - aCreated;
  }
  return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
}

/**
 * Timer positions depend on creation, never their changing countdown or title.
 * Starting moves a ready timer into the active group; pause/resume keeps its
 * position there. Expiry promotes it into the attention group, whose ordering
 * also survives snoozing and alert updates. Snapshot ordering is irrelevant.
 */
export function groupTimers(items: ItemView[]): TimerBuckets {
  const out: TimerBuckets = { ringing: [], running: [], paused: [], idle: [], active: [] };
  const timers = items.filter((item) => item.kind === "timer").sort(compareTimerCreation);
  for (const item of timers) {
    if (item.status.kind === "due") out.ringing.push(item);
    else if (item.timer.state === "running") {
      out.running.push(item);
      out.active.push(item);
    } else if (item.timer.state === "paused") {
      out.paused.push(item);
      out.active.push(item);
    } else out.idle.push(item);
  }
  out.active = [...out.ringing, ...out.active];
  return out;
}

/** Case-insensitive match on title, notes and tag name; optional tag filter. */
export function filterItems(items: ItemView[], query: string, tagId: string | null, tags: Tag[]): ItemView[] {
  const q = query.trim().toLowerCase();
  const tagName = (id: string | null) => tags.find((t) => t.id === id)?.name.toLowerCase() ?? "";
  return items.filter((item) => {
    if (tagId && item.tag !== tagId) return false;
    if (!q) return true;
    return (
      item.title.toLowerCase().includes(q) || item.notes.toLowerCase().includes(q) || tagName(item.tag).includes(q)
    );
  });
}
