/**
 * Formats a remaining or elapsed duration for countdown displays.
 *
 * Under an hour: `m:ss` (e.g. `4:05`). An hour or more: `h:mm:ss`.
 * A day or more: `Nd h:mm:ss`. Negative values (overdue) get a leading `-`.
 * Remaining time rounds up so a countdown never shows `0:00` while time is
 * still left; elapsed time rounds down so an overdue clock reaches `-0:01`
 * only after a whole second has passed.
 */
export function formatCountdown(ms: number): string {
  const totalSeconds = ms < 0 ? Math.floor(-ms / 1000) : Math.ceil(ms / 1000);
  const sign = ms < 0 && totalSeconds > 0 ? "-" : "";
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
  if (Math.abs(ms) > 0 && Math.abs(ms) < 60_000) return `${Math.ceil(Math.abs(ms) / 1000)}s`;
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

/** Exact, readable default so even a short unnamed timer is distinguishable. */
export function timerName(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return "Timer";
  const seconds = Math.max(1, Math.round(ms / 1000));
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  return `${[h && `${h}h`, m && `${m}m`, s && `${s}s`].filter(Boolean).join(" ")} timer`;
}
