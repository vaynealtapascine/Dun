// Mirrors the camelCase JSON produced by dun-core and the Tauri commands.
// Timestamps are UTC milliseconds; civil times are "HH:MM:SS" strings.

export type Ms = number;

export type Nag = { mode: "repeat"; intervalMin: number } | { mode: "once" };

export type ChimeRef = { kind: "bundled"; id: string } | { kind: "custom"; sha256: string; name: string };

export type IntervalUnit = "minutes" | "hours";

export type Rule =
  | { kind: "interval"; every: number; unit: IntervalUnit }
  | { kind: "daily"; every: number; times: string[] }
  | { kind: "weekly"; every: number; weekdays: number[]; times: string[] }
  | { kind: "monthly"; every: number; day: number; times: string[] }
  | { kind: "yearly"; month: number; day: number; times: string[] };

export type Recurrence = {
  rule: Rule;
  /** Civil date-time "YYYY-MM-DDTHH:MM:SS" of the first possible slot. */
  start: string;
  /** IANA zone, or null to follow this device's zone. */
  tz: string | null;
};

export type RepeatMode = "fromSchedule" | "afterCompletion";

export type Schedule =
  | { kind: "oneOff"; due: Ms }
  | { kind: "recurring"; recurrence: Recurrence; mode: RepeatMode; effectiveFrom: Ms }
  | { kind: "timer"; durationMs: number };

export type TimerState = { state: "idle" } | { state: "running"; endAt: Ms } | { state: "paused"; remainingMs: number };

export type Completion = { through: Ms | null; doneAt: Ms | null; gen: number };

export type Snooze = { occurrence: Ms; until: Ms; count: number };

export type ItemKind = "reminder" | "recurring" | "timer";

export type StatusView =
  | { kind: "due"; occurrence: Ms; firstMissed: Ms | null; missedCount: number; ringsAt: Ms; snoozed: boolean }
  | { kind: "upcoming"; at: Ms }
  | { kind: "idle" }
  | { kind: "unscheduled" };

export type Hold = "quiet" | "mute" | "handoff";

export type ItemView = {
  id: string;
  created: { at: Ms; kind: ItemKind; by: string } | null;
  title: string;
  notes: string;
  tag: string | null;
  schedule: Schedule | null;
  timer: TimerState;
  nag: Nag;
  chime: ChimeRef | null;
  quietExemptOverride: boolean | null;
  completion: Completion;
  snooze: Snooze | null;
  deleted: boolean;
  kind: ItemKind;
  quietExempt: boolean;
  status: StatusView;
  ring: { alerts: number; nextAlertAt: Ms | null; held: Hold | null } | null;
};

export type Tag = { id: string; name: string; color: string; order: number; deleted: boolean };

export type Preset = {
  id: string;
  name: string;
  durationMs: number;
  nag: Nag;
  chime: ChimeRef | null;
  tag: string | null;
  order: number;
  deleted: boolean;
};

export type QuietHours = { enabled: boolean; start: string; end: string };

export type HandoffSettings = { enabled: boolean; failLoudAfterS: number; phoneGraceS: number };

export type Settings = {
  quietHours: QuietHours;
  muteUntil: Ms | null;
  nagDefault: Nag;
  dateOnlyTime: string;
  missedSummaryThreshold: number;
  handoff: HandoffSettings;
};

export type Snapshot = {
  now: Ms;
  tz: string;
  deviceId: string;
  items: ItemView[];
  tags: Tag[];
  presets: Preset[];
  settings: Settings;
  summary: string[];
};

export type HistoryKind = "done" | "undo" | "restart";

export type HistoryRow = {
  id: string;
  itemId: string;
  kind: HistoryKind;
  occurrence: Ms | null;
  at: Ms;
  snoozeCount: number;
  title: string;
  refId: string | null;
  prevCompletion: Completion | null;
  device: string;
};

export type LocalSettings = {
  chime: ChimeRef;
  volume: number;
  hotkey: string;
  autostart: boolean;
  idleThresholdS: number;
  theme: "system" | "light" | "dark";
  /** Sounds imported on this device. */
  customSounds: ChimeRef[];
};

export type ItemDraft = {
  title: string;
  notes: string;
  tag: string | null;
  schedule: Schedule;
  nag: Nag | null;
  chime: ChimeRef | null;
  quietExempt: boolean | null;
  startTimer: boolean;
};

export type PresetDraft = {
  name: string;
  durationMs: number;
  nag: Nag | null;
  chime: ChimeRef | null;
  tag: string | null;
  order: number;
};

export type ChimeOption = { chime: ChimeRef; label: string };
