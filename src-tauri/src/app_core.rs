//! The process-wide engine and what surrounds it: the clock, device-local
//! settings, and the hook that tells the scheduler loop to re-evaluate.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use dun_core::engine::Engine;
use dun_core::model::ChimeRef;
use dun_core::snapshot;
use dun_core::time::{Clock, SystemClock, TimeZone, Timestamp};
use serde::{Deserialize, Serialize};

/// Settings that belong to this device only and never sync.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LocalSettings {
    pub chime: ChimeRef,
    /// 0.0–1.0
    pub volume: f32,
    /// Global quick-add shortcut, e.g. "CommandOrControl+Alt+N".
    pub hotkey: String,
    pub autostart: bool,
    /// Seconds without input before the PC counts as unattended.
    pub idle_threshold_s: u32,
    /// "system", "light" or "dark".
    pub theme: String,
    /// Sounds imported on this device, for the chime pickers.
    pub custom_sounds: Vec<ChimeRef>,
}

impl Default for LocalSettings {
    fn default() -> Self {
        LocalSettings {
            chime: ChimeRef::Bundled { id: "bell".into() },
            volume: 0.8,
            hotkey: "CommandOrControl+Alt+N".into(),
            autostart: true,
            idle_threshold_s: 300,
            theme: "system".into(),
            custom_sounds: Vec::new(),
        }
    }
}

const LOCAL_SETTINGS_KEY: &str = "device";

pub struct AppCore {
    engine: Mutex<Engine>,
    clock: SystemClock,
    data_dir: PathBuf,
    wake: Mutex<Option<Box<dyn Fn() + Send>>>,
}

impl AppCore {
    pub fn open(data_dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(data_dir)
            .map_err(|e| format!("create {}: {e}", data_dir.display()))?;
        let engine = Engine::open(&data_dir.join("dun.sqlite3")).map_err(|e| e.to_string())?;
        Ok(AppCore {
            engine: Mutex::new(engine),
            clock: SystemClock::from_env(),
            data_dir: data_dir.to_path_buf(),
            wake: Mutex::new(None),
        })
    }

    /// Debug builds only: `--dev-seed` (or `DUN_DEV_SEED=1`) adds a few items that ring within a
    /// minute, for exercising toasts, chimes and the UI end to end.
    pub fn dev_seed(&self) {
        if !cfg!(debug_assertions) {
            return;
        }
        use dun_core::actions::ItemDraft;
        use dun_core::model::Schedule;
        use dun_core::recurrence::{IntervalUnit, Recurrence, Rule};
        use dun_core::time::{MINUTE, SECOND};

        let now = self.now();
        let start = (now.plus(MINUTE)).to_zoned(&self.tz()).datetime();
        let draft = |title: &str, schedule: Schedule, start_timer: bool| ItemDraft {
            title: title.into(),
            notes: "Created by --dev-seed".into(),
            tag: None,
            schedule,
            nag: None,
            chime: None,
            quiet_exempt: None,
            start_timer,
        };
        let seeds = [
            draft(
                "Seed: reminder",
                Schedule::OneOff {
                    due: now.plus(30 * SECOND),
                },
                false,
            ),
            draft(
                "Seed: timer",
                Schedule::Timer {
                    duration_ms: 45 * SECOND,
                },
                true,
            ),
            draft(
                "Seed: every 2 minutes",
                Schedule::Recurring {
                    recurrence: Recurrence {
                        rule: Rule::Interval {
                            every: 2,
                            unit: IntervalUnit::Minutes,
                        },
                        start,
                        tz: None,
                    },
                    mode: Default::default(),
                    effective_from: now,
                },
                false,
            ),
        ];
        let mut engine = self.engine();
        for seed in seeds {
            if let Err(e) = engine.create_item(now, seed) {
                eprintln!("dev seed failed: {e}");
            }
        }
    }

    pub fn engine(&self) -> MutexGuard<'_, Engine> {
        self.engine.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn now(&self) -> Timestamp {
        self.clock.now()
    }

    pub fn tz(&self) -> TimeZone {
        self.clock.tz()
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Installs the callback that nudges the scheduler loop.
    pub fn set_wake(&self, f: impl Fn() + Send + 'static) {
        *self.wake.lock().unwrap_or_else(|p| p.into_inner()) = Some(Box::new(f));
    }

    /// Asks the scheduler to re-evaluate soon (after any state change).
    pub fn wake_scheduler(&self) {
        if let Some(f) = self.wake.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            f();
        }
    }

    pub fn snapshot_json(&self) -> serde_json::Value {
        let (now, tz) = (self.now(), self.tz());
        let engine = self.engine();
        let snap = snapshot::build(
            engine.state(),
            engine.scheduler(),
            engine.device_id(),
            now,
            &tz,
        );
        serde_json::to_value(snap).unwrap_or(serde_json::Value::Null)
    }

    pub fn local_settings(&self) -> LocalSettings {
        self.engine()
            .store()
            .local_get::<LocalSettings>(LOCAL_SETTINGS_KEY)
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    pub fn set_local_settings(&self, settings: &LocalSettings) -> Result<(), String> {
        // Store::local_set needs &mut; go through the engine's store handle.
        let mut engine = self.engine();
        engine
            .store_mut()
            .local_set(LOCAL_SETTINGS_KEY, settings)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_on_disk_and_round_trips_local_settings() {
        let dir = std::env::temp_dir().join(format!("dun-appcore-{}", dun_core::ids::new_id()));
        {
            let core = AppCore::open(&dir).unwrap();
            assert_eq!(core.local_settings(), LocalSettings::default());
            let custom = LocalSettings {
                volume: 0.25,
                theme: "dark".into(),
                ..LocalSettings::default()
            };
            core.set_local_settings(&custom).unwrap();
            let snap = core.snapshot_json();
            assert!(snap["items"].as_array().unwrap().is_empty());
            assert!(snap["deviceId"].as_str().is_some_and(|d| !d.is_empty()));
        }
        let reopened = AppCore::open(&dir).unwrap();
        assert_eq!(reopened.local_settings().volume, 0.25);
        assert_eq!(reopened.local_settings().theme, "dark");
        drop(reopened);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn partial_local_settings_fill_in_defaults() {
        let parsed: LocalSettings = serde_json::from_str(r#"{"volume": 0.5}"#).unwrap();
        assert_eq!(parsed.volume, 0.5);
        assert_eq!(parsed.hotkey, LocalSettings::default().hotkey);
    }
}
