const pad = (n: number) => String(n).padStart(2, "0");

/** ms -> "YYYY-MM-DDTHH:MM" for `<input type="datetime-local">` (local time). */
export function toLocalInput(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** "YYYY-MM-DDTHH:MM" (local) -> ms, or null if incomplete/invalid. */
export function fromLocalInput(value: string): number | null {
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}/.test(value)) return null;
  const ms = new Date(value).getTime();
  return Number.isNaN(ms) ? null : ms;
}

/** ms -> "YYYY-MM-DD" (local). */
export function toDateInput(ms: number): string {
  return toLocalInput(ms).slice(0, 10);
}

/** "HH:MM" or "HH:MM:SS" -> "HH:MM:SS" as the core expects. */
export function toCivilTime(value: string): string {
  const [h = "00", m = "00", s = "00"] = value.split(":");
  return `${h.padStart(2, "0")}:${m.padStart(2, "0")}:${s.padStart(2, "0")}`;
}

/** "HH:MM:SS" -> "HH:MM" for `<input type="time">`. */
export function toTimeInput(value: string): string {
  return value.slice(0, 5);
}

/** The next whole `step` minutes after `ms` (e.g. next quarter hour). */
export function roundUpMinutes(ms: number, step: number): number {
  const stepMs = step * 60_000;
  return Math.ceil((ms + 1) / stepMs) * stepMs;
}

export function splitDuration(ms: number): { h: number; m: number; s: number } {
  const total = Math.max(0, Math.round(ms / 1000));
  return { h: Math.floor(total / 3600), m: Math.floor((total % 3600) / 60), s: total % 60 };
}

export function joinDuration(h: number, m: number, s: number): number {
  return ((h || 0) * 3600 + (m || 0) * 60 + (s || 0)) * 1000;
}
