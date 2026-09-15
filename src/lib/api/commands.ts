import { invoke } from "@tauri-apps/api/core";
import type {
  ChimeOption,
  ChimeRef,
  HistoryRow,
  ItemDraft,
  LocalSettings,
  Ms,
  PresetDraft,
  Settings,
  Snapshot,
} from "./types";

export const api = {
  snapshot: () => invoke<Snapshot>("get_snapshot"),

  createItem: (draft: ItemDraft) => invoke<string>("create_item", { draft }),
  updateItem: (id: string, draft: ItemDraft) => invoke<void>("update_item", { id, draft }),
  deleteItem: (id: string, deleted = true) => invoke<void>("delete_item", { id, deleted }),

  done: (id: string, occurrence?: Ms | null) => invoke<void>("mark_done", { id, occurrence: occurrence ?? null }),
  snooze: (id: string, minutes: number, occurrence?: Ms | null) =>
    invoke<void>("snooze", { id, minutes, occurrence: occurrence ?? null }),
  snoozeAll: (ids: string[], minutes: number) => invoke<void>("snooze_all", { ids, minutes }),
  undo: (historyId: string) => invoke<void>("undo", { historyId }),
  history: (itemId?: string | null, before?: Ms | null, limit = 100) =>
    invoke<HistoryRow[]>("get_history", { itemId: itemId ?? null, before: before ?? null, limit }),

  timer: (id: string, action: "start" | "pause" | "resume" | "reset") => invoke<void>("timer_action", { id, action }),

  savePreset: (id: string | null, draft: PresetDraft) => invoke<string>("save_preset", { id, draft }),
  deletePreset: (id: string) => invoke<void>("delete_preset", { id }),
  startPreset: (id: string) => invoke<string>("start_preset", { id }),

  saveTag: (id: string | null, name: string, color: string, order: number) =>
    invoke<string>("save_tag", { id, name, color, order }),
  deleteTag: (id: string) => invoke<void>("delete_tag", { id }),

  setSetting: <K extends keyof Settings>(key: K, value: Settings[K]) => invoke<void>("set_setting", { key, value }),
  muteFor: (minutes: number) => invoke<void>("mute", { minutes, untilTomorrow: false }),
  muteUntilTomorrow: () => invoke<void>("mute", { minutes: null, untilTomorrow: true }),
  unmute: () => invoke<void>("mute", { minutes: null, untilTomorrow: false }),

  localSettings: () => invoke<LocalSettings>("get_local_settings"),
  setLocalSettings: (settings: LocalSettings) => invoke<void>("set_local_settings", { settings }),

  chimes: () => invoke<ChimeOption[]>("list_chimes"),
  playChime: (chime: ChimeRef | null) => invoke<void>("play_chime", { chime }),
};

/** Tauri rejects with the Rust error string; normalise for display. */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
