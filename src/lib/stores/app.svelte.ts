import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api, errorText } from "../api/commands";
import { isDesktop } from "../platform";
import type { LocalSettings, Snapshot } from "../api/types";

/** Stand-in so shared views that read device settings still render on Android. */
const PHONE_LOCAL: LocalSettings = {
  chime: { kind: "bundled", id: "bell" },
  volume: 1,
  hotkey: "",
  autostart: false,
  idleThresholdS: 300,
  theme: "system",
  customSounds: [],
};

/**
 * App-wide reactive state: the latest snapshot from Rust (pushed on every
 * change), device-local settings, and a ticking "now" aligned to the core's
 * clock so countdowns agree with when things actually ring.
 */
class AppStore {
  snapshot = $state<Snapshot | null>(null);
  local = $state<LocalSettings | null>(null);
  error = $state<string | null>(null);
  /** A short confirmation ("Added “Tea”"), cleared after a few seconds. */
  notice = $state<string | null>(null);
  now = $state(Date.now());
  #noticeTimer: ReturnType<typeof setTimeout> | undefined;

  notify(message: string) {
    this.notice = message;
    clearTimeout(this.#noticeTimer);
    this.#noticeTimer = setTimeout(() => (this.notice = null), 2500);
  }

  /** Core clock minus browser clock, from the last snapshot. */
  #offset = 0;
  #unlisten: UnlistenFn[] = [];
  #tick: ReturnType<typeof setInterval> | undefined;

  setSnapshot(snap: Snapshot) {
    this.#offset = snap.now - Date.now();
    this.snapshot = snap;
    this.now = snap.now;
  }

  async start() {
    try {
      this.setSnapshot(await api.snapshot());
      // Device settings are the desktop's (tray, hotkey, autostart); the phone
      // keeps its own handful in Android settings and its sync status.
      this.local = isDesktop ? await api.localSettings() : PHONE_LOCAL;
    } catch (e) {
      this.error = errorText(e);
    }
    this.#unlisten.push(await listen<Snapshot>("state-changed", (e) => this.setSnapshot(e.payload)));
    this.#unlisten.push(await listen<LocalSettings>("local-settings-changed", (e) => (this.local = e.payload)));
    this.#tick = setInterval(() => (this.now = Date.now() + this.#offset), 1000);
  }

  stop() {
    this.#unlisten.forEach((f) => f());
    this.#unlisten = [];
    clearInterval(this.#tick);
  }

  /** Runs a command, surfacing its error in the UI instead of throwing. */
  async run<T>(fn: () => Promise<T>): Promise<T | undefined> {
    try {
      this.error = null;
      return await fn();
    } catch (e) {
      this.error = errorText(e);
      return undefined;
    }
  }

  tag(id: string | null) {
    return id ? this.snapshot?.tags.find((t) => t.id === id) : undefined;
  }
}

export const app = new AppStore();
