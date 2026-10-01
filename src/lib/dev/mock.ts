// Dev-only: a fake backend so the UI can be previewed in a plain browser
// (`npm run dev`, then open http://localhost:1420). Never bundled into the app:
// main.ts only imports this when running under Vite dev outside Tauri.

import { mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { HistoryRow, ItemDraft, ItemView, LocalSettings, PresetDraft, Snapshot, StatusView, SyncStatus } from "../api/types";

const MIN = 60_000;

export function installMockBackend() {
  // Lets the app tell a browser preview apart from a real (e.g. Android) webview.
  (window as unknown as Record<string, unknown>).__DUN_PREVIEW__ = true;
  const now = Date.now();
  const base = (id: string, title: string, status: StatusView, extra: Partial<ItemView> = {}): ItemView => ({
    id,
    created: { at: now - 86_400_000, kind: extra.kind ?? "reminder", by: "preview" },
    title,
    notes: "",
    tag: null,
    schedule: { kind: "oneOff", due: status.kind === "due" ? status.occurrence : status.kind === "upcoming" ? status.at : now },
    timer: { state: "idle" },
    nag: { mode: "repeat", intervalMin: 1 },
    chime: null,
    quietExemptOverride: null,
    completion: { through: null, doneAt: null, gen: 0 },
    snooze: null,
    deleted: false,
    kind: "reminder",
    quietExempt: false,
    status,
    ring: null,
    ...extra,
  });
  const due = (at: number, snoozed = false, missed = 1): Extract<StatusView, { kind: "due" }> => ({
    kind: "due",
    occurrence: at,
    firstMissed: missed > 1 ? at - (missed - 1) * 86_400_000 : null,
    missedCount: missed,
    ringsAt: at,
    snoozed,
  });

  const snapshot: Snapshot = {
    now,
    tz: "Local",
    deviceId: "preview",
    tags: [
      { id: "home", name: "Home", color: "#3cb371", order: 0, deleted: false },
      { id: "work", name: "Work", color: "#3a86ff", order: 1, deleted: false },
    ],
    presets: [
      { id: "tea", name: "Tea", durationMs: 4 * MIN, nag: { mode: "repeat", intervalMin: 1 }, chime: null, tag: null, order: 0, deleted: false },
      { id: "pomo", name: "Pomodoro", durationMs: 25 * MIN, nag: { mode: "repeat", intervalMin: 1 }, chime: null, tag: "work", order: 1, deleted: false },
      { id: "laundry", name: "Laundry", durationMs: 45 * MIN, nag: { mode: "repeat", intervalMin: 5 }, chime: null, tag: "home", order: 2, deleted: false },
    ],
    settings: {
      quietHours: { enabled: true, start: "23:00:00", end: "07:00:00" },
      muteUntil: null,
      nagDefault: { mode: "repeat", intervalMin: 1 },
      dateOnlyTime: "09:00:00",
      missedSummaryThreshold: 3,
      handoff: { enabled: true, failLoudAfterS: 120, phoneGraceS: 120 },
    },
    summary: [],
    items: [
      base("rent", "Pay rent", due(now - 3 * MIN), { tag: "home", ring: { alerts: 3, nextAlertAt: now + MIN, held: null } }),
      base(
        "meds",
        "Evening meds",
        due(now - 26 * 60 * MIN, false, 2),
        {
          kind: "recurring",
          schedule: {
            kind: "recurring",
            recurrence: { rule: { kind: "daily", every: 1, times: ["21:00:00"] }, start: "2026-09-01T00:00:00", tz: null },
            mode: "fromSchedule",
            effectiveFrom: 0,
          },
          ring: { alerts: 1, nextAlertAt: now + MIN, held: null },
        },
      ),
      base("call", "Call the dentist", { ...due(now - 5 * MIN, true), ringsAt: now + 12 * MIN }, { tag: "work", snooze: { occurrence: now - 5 * MIN, until: now + 12 * MIN, count: 2 } }),
      base("stretch", "Stretch", { kind: "upcoming", at: now + 95 * MIN }, {
        kind: "recurring",
        schedule: {
          kind: "recurring",
          recurrence: { rule: { kind: "weekly", every: 1, weekdays: [1, 2, 3, 4, 5], times: ["15:00:00"] }, start: "2026-09-01T00:00:00", tz: null },
          mode: "fromSchedule",
          effectiveFrom: 0,
        },
      }),
      base("bins", "Take the bins out", { kind: "upcoming", at: now + 26 * 60 * MIN }, { tag: "home", notes: "Recycling week" }),
      base("passport", "Renew passport", { kind: "upcoming", at: now + 9 * 86_400_000 }),
      base("pomo-1", "Pomodoro", { kind: "upcoming", at: now + 17 * MIN + 24_000 }, {
        kind: "timer",
        quietExempt: true,
        tag: "work",
        schedule: { kind: "timer", durationMs: 25 * MIN },
        timer: { state: "running", endAt: now + 17 * MIN + 24_000 },
      }),
      base("eggs", "Eggs", due(now - 20_000), {
        kind: "timer",
        quietExempt: true,
        schedule: { kind: "timer", durationMs: 7 * MIN },
        timer: { state: "running", endAt: now - 20_000 },
      }),
      base("bread", "Bread proof", { kind: "idle" }, {
        kind: "timer",
        quietExempt: true,
        schedule: { kind: "timer", durationMs: 120 * MIN },
        timer: { state: "paused", remainingMs: 38 * MIN },
      }),
    ],
  };

  const local: LocalSettings = {
    chime: { kind: "bundled", id: "bell" },
    volume: 0.8,
    hotkey: "CommandOrControl+Alt+N",
    autostart: true,
    idleThresholdS: 300,
    theme: "system",
    customSounds: [],
  };

  // A blocky stand-in for the real QR, just to see the layout.
  const PREVIEW_QR =
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 21" shape-rendering="crispEdges">' +
    '<rect width="21" height="21" fill="#fff"/>' +
    Array.from({ length: 21 * 21 }, (_, i) =>
      (i * 7919) % 3 === 0
        ? `<rect x="${i % 21}" y="${Math.floor(i / 21)}" width="1" height="1" fill="#000"/>`
        : "",
    ).join("") +
    "</svg>";

  const sync: SyncStatus = {
    enabled: true,
    listening: true,
    port: 47823,
    addrs: ["192.168.1.10", "100.64.0.2"],
    fingerprint: "ab".repeat(32),
    peers: [
      { deviceId: "phone-1", name: "Pixel 8", pairedAt: now - 3 * 86_400_000, lastSeenAt: now - 4 * MIN },
    ],
    pairing: null,
    publicNetworks: [],
  };

  const history: HistoryRow[] = [
    { id: "h1", itemId: "stretch", kind: "done", occurrence: now - 20 * 60 * MIN, at: now - 19 * 60 * MIN, snoozeCount: 1, title: "Stretch", refId: null, prevCompletion: null, device: "phone" },
    { id: "h2", itemId: "rent", kind: "done", occurrence: now - 2 * 86_400_000, at: now - 2 * 86_400_000 + 7 * MIN, snoozeCount: 0, title: "Water plants", refId: null, prevCompletion: null, device: "preview" },
  ];

  let sequence = 0;
  const nextId = () => `preview-${Date.now()}-${++sequence}`;
  // The native bridge serializes JSON. Reading through reactive proxies here
  // reproduces that boundary; structuredClone rejects Svelte's proxies.
  const copy = <T,>(value: T): T => value === undefined ? value : JSON.parse(JSON.stringify(value));
  const live = (id: unknown) => {
    const item = snapshot.items.find((i) => i.id === id);
    if (!item) throw new Error("Item not found.");
    return item;
  };

  function quietUntil(t: number): number | null {
    const quiet = snapshot.settings.quietHours;
    if (!quiet.enabled) return null;
    const date = new Date(t);
    const seconds = date.getHours() * 3600 + date.getMinutes() * 60 + date.getSeconds();
    const civil = (value: string) => value.split(":").reduce((total, part) => total * 60 + Number(part), 0);
    const start = civil(quiet.start);
    const end = civil(quiet.end);
    const active = start > end ? seconds >= start || seconds < end : seconds >= start && seconds < end;
    if (!active) return null;
    date.setHours(Math.floor(end / 3600), Math.floor((end % 3600) / 60), end % 60, 0);
    if (date.getTime() <= t) date.setDate(date.getDate() + 1);
    return date.getTime();
  }

  // Only the browser preview owns this evaluator. Production snapshots still
  // come entirely from Rust; this gives UI actions the same event-driven flow.
  function evaluate(t = Date.now()): boolean {
    snapshot.now = t;
    let changed = false;
    for (const item of snapshot.items) {
      const before = JSON.stringify([item.status, item.ring]);
      const schedule = item.schedule;
      let occurrence: number | null = null;
      if (!schedule) item.status = { kind: "unscheduled" };
      else if (schedule.kind === "timer") {
        occurrence = item.timer.state === "running" ? item.timer.endAt : null;
        if (occurrence == null) item.status = { kind: "idle" };
      } else if (schedule.kind === "oneOff") occurrence = schedule.due;
      else {
        // Calendar recurrence is intentionally approximate in the browser.
        if (item.status.kind === "due") occurrence = item.status.occurrence;
        else if (item.status.kind === "upcoming") occurrence = item.status.at;
      }
      if (occurrence != null) {
        if (item.completion.through != null && occurrence <= item.completion.through) item.status = { kind: "idle" };
        else if (occurrence > t) item.status = { kind: "upcoming", at: occurrence };
        else {
          const snooze = item.snooze?.occurrence === occurrence ? item.snooze : null;
          const previous = item.status.kind === "due" && item.status.occurrence === occurrence ? item.status : null;
          item.status = {
            ...due(occurrence),
            firstMissed: previous?.firstMissed ?? null,
            missedCount: previous?.missedCount ?? 1,
            ringsAt: Math.max(occurrence, snooze?.until ?? occurrence),
            snoozed: (snooze?.until ?? 0) > t,
          };
        }
      }
      if (item.status.kind !== "due" || item.status.snoozed) item.ring = null;
      else {
        const quietEnd = item.quietExempt ? null : quietUntil(t);
        const mutedUntil = (snapshot.settings.muteUntil ?? 0) > t ? snapshot.settings.muteUntil : null;
        const held = quietEnd != null ? "quiet" : mutedUntil != null ? "mute" :
          item.ring?.held === "handoff" && snapshot.settings.handoff.enabled ? "handoff" : null;
        const previousHold = item.ring?.held;
        item.ring ??= { alerts: 0, nextAlertAt: t, held: null };
        item.ring.held = held;
        if (held) item.ring.nextAlertAt = quietEnd ?? mutedUntil ?? item.ring.nextAlertAt;
        else if (previousHold || (item.ring.nextAlertAt != null && item.ring.nextAlertAt <= t)) {
          item.ring.alerts += 1;
          item.ring.nextAlertAt = item.nag.mode === "repeat" ? t + item.nag.intervalMin * MIN : null;
        }
      }
      changed ||= before !== JSON.stringify([item.status, item.ring]);
    }
    return changed;
  }

  function publish() {
    evaluate();
    void emit("state-changed", copy(snapshot));
  }

  function create(draft: ItemDraft): string {
    if (!draft.title.trim()) throw new Error("Give it a title.");
    if (draft.schedule.kind === "timer" && (!Number.isSafeInteger(draft.schedule.durationMs) || draft.schedule.durationMs < 1000)) {
      throw new Error("Set a valid timer duration.");
    }
    const t = Date.now();
    const kind = draft.schedule.kind === "oneOff" ? "reminder" : draft.schedule.kind === "timer" ? "timer" : "recurring";
    const id = nextId();
    const item = base(id, draft.title.trim(), { kind: "idle" }, {
      created: { at: t, kind, by: "preview" },
      kind,
      notes: draft.notes,
      tag: draft.tag,
      schedule: copy(draft.schedule),
      nag: copy(draft.nag ?? snapshot.settings.nagDefault),
      chime: copy(draft.chime),
      quietExemptOverride: draft.quietExempt,
      quietExempt: draft.quietExempt ?? kind === "timer",
    });
    if (draft.schedule.kind === "timer" && draft.startTimer) item.timer = { state: "running", endAt: t + draft.schedule.durationMs };
    if (draft.schedule.kind === "recurring") item.status = { kind: "upcoming", at: t + 60 * MIN };
    snapshot.items.push(item);
    return id;
  }

  function record(item: ItemView, kind: HistoryRow["kind"], occurrence: number | null): HistoryRow {
    const entry: HistoryRow = {
      id: nextId(), itemId: item.id, kind, occurrence, at: Date.now(),
      snoozeCount: item.snooze?.occurrence === occurrence ? item.snooze.count : 0,
      title: item.title, refId: null, prevCompletion: copy(item.completion), device: "preview",
    };
    history.unshift(entry);
    return entry;
  }

  function snooze(item: ItemView, minutes: number, occurrence: unknown) {
    if (!Number.isInteger(minutes) || minutes < 1 || minutes > 1440) throw new Error("Snooze must be between 1 and 1440 minutes.");
    if (item.status.kind !== "due" || (occurrence != null && occurrence !== item.status.occurrence)) return;
    const target = item.status.occurrence;
    item.snooze = { occurrence: target, until: Date.now() + minutes * MIN, count: (item.snooze?.occurrence === target ? item.snooze.count : 0) + 1 };
  }

  mockIPC((cmd, args) => {
    const a = copy(args ?? {}) as Record<string, unknown>;
    switch (cmd) {
      case "get_snapshot":
        evaluate();
        return copy(snapshot);
      case "get_local_settings":
        return copy(local);
      case "integration_status":
      case "integration_set_enabled":
      case "integration_rotate_token":
        return { enabled: a.enabled ?? false, running: a.enabled ?? false, url: "http://127.0.0.1:48475", token: "preview-key", error: null };
      case "list_chimes":
        return ["bell", "rise", "pulse", "soft"].map((id) => ({ chime: { kind: "bundled", id }, label: id[0]!.toUpperCase() + id.slice(1) }));
      case "get_history":
        return copy(history.filter((row) => (a.itemId == null || row.itemId === a.itemId) &&
          (a.before == null || row.at < Number(a.before))).sort((left, right) => right.at - left.at).slice(0, Number(a.limit ?? 100)));
      case "app_version":
        return "v1.0.0 (preview)";
      case "preview_schedule": {
        // Rough stand-in for the engine: good enough to see the preview UI.
        const s = (a.draft as { schedule: import("../api/types").Schedule }).schedule;
        const t = Date.now();
        if (s.kind === "oneOff") return [s.due];
        if (s.kind === "timer") return [t + s.durationMs];
        return [t + 60 * MIN, t + 25 * 60 * MIN, t + 49 * 60 * MIN];
      }
      case "backup_export":
        return "C:/Users/you/Documents/dun-backup.json";
      case "backup_import":
        return { registersChanged: 42, historyAdded: 3, deleted: 0, localSettings: 1 };
      case "import_sound":
        return { kind: "custom", sha256: "abc123", name: "My alarm" };
      case "sync_status":
        return sync;
      case "sync_set_enabled":
        sync.enabled = a.enabled as boolean;
        sync.listening = sync.enabled;
        return null;
      case "pairing_open": {
        sync.pairing = {
          code: "418302",
          expiresAt: Date.now() + 5 * MIN,
          triesLeft: 5,
          uri: "dun://pair?v=1&id=preview&n=Desk%20PC&p=47823&a=192.168.1.10,100.64.0.2&fp=" + "ab".repeat(32) + "&c=418302",
          qrSvg: PREVIEW_QR,
        };
        return sync.pairing;
      }
      case "pairing_close":
        sync.pairing = null;
        return null;
      case "forget_peer":
        sync.peers = sync.peers.filter((p) => p.deviceId !== a.deviceId);
        return null;
      case "hotkey_status":
        return null;
      case "set_local_settings":
        Object.assign(local, a.settings);
        void emit("local-settings-changed", copy(local));
        return null;
      case "create_item": {
        const id = create(a.draft as ItemDraft);
        publish();
        return id;
      }
      case "start_preset": {
        const preset = snapshot.presets.find((p) => p.id === a.id);
        if (!preset) throw new Error("Preset not found.");
        const id = create({ title: preset.name, notes: "", tag: preset.tag, schedule: { kind: "timer", durationMs: preset.durationMs },
          nag: preset.nag, chime: preset.chime, quietExempt: null, startTimer: true });
        publish();
        return id;
      }
      case "timer_action": {
        evaluate();
        const item = live(a.id);
        if (item.schedule?.kind !== "timer") throw new Error("Not a timer.");
        const t = Date.now();
        if (a.action === "start") {
          if (item.timer.state !== "idle") record(item, "restart", null);
          item.timer = { state: "running", endAt: t + item.schedule.durationMs };
          item.ring = null;
        } else if (a.action === "pause" && item.timer.state === "running" && item.timer.endAt > t) {
          item.timer = { state: "paused", remainingMs: item.timer.endAt - t };
        } else if (a.action === "resume" && item.timer.state === "paused") {
          item.timer = { state: "running", endAt: t + Math.max(0, item.timer.remainingMs) };
        } else if (a.action === "reset") item.timer = { state: "idle" };
        publish();
        return null;
      }
      case "mark_done": {
        evaluate();
        const item = live(a.id);
        const occurrence = a.occurrence == null ? item.status.kind === "due" ? item.status.occurrence : item.status.kind === "upcoming" ? item.status.at : null : Number(a.occurrence);
        if (occurrence == null) throw new Error("Nothing due.");
        if (item.completion.through == null || occurrence > item.completion.through) {
          record(item, "done", occurrence);
          item.completion = { through: occurrence, doneAt: Date.now(), gen: item.completion.gen };
          if (item.snooze && item.snooze.occurrence <= occurrence) item.snooze = null;
          if (item.timer.state === "running" && item.timer.endAt <= occurrence) item.timer = { state: "idle" };
          if (item.kind === "recurring") item.status = { kind: "upcoming", at: Date.now() + 24 * 60 * MIN };
        }
        publish();
        return null;
      }
      case "snooze":
        evaluate();
        snooze(live(a.id), Number(a.minutes), a.occurrence);
        publish();
        return null;
      case "snooze_all":
        evaluate();
        for (const id of a.ids as string[]) snooze(live(id), Number(a.minutes), null);
        publish();
        return null;
      case "delete_item": {
        const all = [...snapshot.items, ...(snapshot.archived ?? [])];
        const item = all.find((i) => i.id === a.id);
        if (!item) throw new Error("Item not found.");
        item.deleted = a.deleted as boolean;
        item.ring = null;
        snapshot.items = all.filter((i) => !i.deleted);
        snapshot.archived = all.filter((i) => i.deleted);
        publish();
        return null;
      }
      case "update_item": {
        const item = live(a.id);
        const draft = a.draft as ItemDraft;
        Object.assign(item, { title: draft.title.trim(), notes: draft.notes, tag: draft.tag, schedule: copy(draft.schedule),
          nag: copy(draft.nag ?? item.nag), chime: copy(draft.chime), quietExemptOverride: draft.quietExempt,
          quietExempt: draft.quietExempt ?? draft.schedule.kind === "timer" });
        item.kind = draft.schedule.kind === "oneOff" ? "reminder" : draft.schedule.kind === "timer" ? "timer" : "recurring";
        publish();
        return null;
      }
      case "save_preset": {
        const draft = a.draft as PresetDraft;
        if (!draft.name.trim() || !Number.isSafeInteger(draft.durationMs) || draft.durationMs < 1000) throw new Error("Set a name and valid timer duration.");
        const id = a.id as string | null ?? nextId();
        const preset = { ...copy(draft), id, name: draft.name.trim(), nag: copy(draft.nag ?? snapshot.settings.nagDefault), deleted: false };
        snapshot.presets = [...snapshot.presets.filter((p) => p.id !== id), preset];
        publish();
        return id;
      }
      case "delete_preset":
        snapshot.presets = snapshot.presets.filter((p) => p.id !== a.id);
        publish();
        return null;
      case "set_setting":
        Object.assign(snapshot.settings, { [a.key as string]: copy(a.value) });
        publish();
        return null;
      case "mute": {
        const tomorrow = new Date(Date.now());
        tomorrow.setDate(tomorrow.getDate() + 1);
        tomorrow.setHours(0, 0, 0, 0);
        snapshot.settings.muteUntil = a.untilTomorrow ? tomorrow.getTime() : a.minutes == null ? null : Date.now() + Number(a.minutes) * MIN;
        publish();
        return null;
      }
      default:
        return null;
    }
  }, { shouldMockEvents: true });

  const tick = setInterval(() => {
    if (evaluate()) void emit("state-changed", copy(snapshot));
  }, 1000);
  const stop = () => {
    clearInterval(tick);
    window.removeEventListener("beforeunload", stop);
  };
  window.addEventListener("beforeunload", stop, { once: true });
  import.meta.hot?.dispose(stop);
  return stop;
}
