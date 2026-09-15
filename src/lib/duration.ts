/**
 * Formats a remaining or elapsed duration for countdown displays.
 *
 * Under an hour: `m:ss` (e.g. `4:05`). An hour or more: `h:mm:ss`.
 * A day or more: `Nd h:mm:ss`. Negative values (overdue) get a leading `-`.
 * Sub-second remainders round up so a countdown never shows `0:00` while
 * time is still left.
 */
export function formatCountdown(ms: number): string {
  const sign = ms < 0 ? "-" : "";
  const totalSeconds = Math.ceil(Math.abs(ms) / 1000);
  const days = Math.floor(totalSeconds / 86_400);
  const hours = Math.floor((totalSeconds % 86_400) / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const ss = String(seconds).padStart(2, "0");

  if (days > 0) {
    return `${sign}${days}d ${hours}:${String(minutes).padStart(2, "0")}:${ss}`;
  }
  if (hours > 0) {
    return `${sign}${hours}:${String(minutes).padStart(2, "0")}:${ss}`;
  }
  return `${sign}${minutes}:${ss}`;
}

/** Compact human form for labels: `45m`, `1h 30m`, `2d 4h`. */
export function formatShort(ms: number): string {
  const totalMinutes = Math.round(Math.abs(ms) / 60_000);
  const days = Math.floor(totalMinutes / 1440);
  const hours = Math.floor((totalMinutes % 1440) / 60);
  const minutes = totalMinutes % 60;
  const parts: string[] = [];
  if (days) parts.push(`${days}d`);
  if (hours) parts.push(`${hours}h`);
  if (minutes && !days) parts.push(`${minutes}m`);
  return parts.length ? parts.join(" ") : "0m";
}
