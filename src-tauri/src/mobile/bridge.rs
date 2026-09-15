//! Entry point shared by Android's background receivers (through JNI) and the
//! in-app command path. It opens the process-wide database, applies one event
//! and returns the plan Kotlin should carry out.
//!
//! **M1 spike:** the event handling below drives a single test ring so the
//! alarm → receiver → JNI → SQLite → notification loop can be exercised on a
//! real device before the core scheduler exists. It is replaced by the real
//! core in M10; the JSON contract in `plan.rs` stays.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use rusqlite::{params, Connection, OptionalExtension};

use super::plan::{Button, Event, Plan, Post, Response};

pub const SPIKE_ITEM: &str = "spike";
const SPIKE_NOTIF: i32 = 1;
const NAG_INTERVAL_MS: i64 = 60_000;
/// Alarms can arrive a little early; treat anything this close as due.
const EARLY_TOLERANCE_MS: i64 = 1_500;

struct Shared {
    dir: PathBuf,
    store: SpikeStore,
}

/// One connection for the whole process. Receivers and the app run in the same
/// process and load the same `.so`, so this is the only SQLite handle on the file.
static SHARED: Mutex<Option<Shared>> = Mutex::new(None);

/// JNI and in-app entry point. Never panics across the boundary: all failures
/// become `{"ok":false,...}` so Kotlin can fall back loud.
pub fn handle_event_json(data_dir: &str, tz_id: &str, now_ms: i64, event_json: &str) -> String {
    let started = Instant::now();
    let result = std::panic::catch_unwind(|| handle_event(data_dir, tz_id, now_ms, event_json));
    let (plan, error) = match result {
        Ok(Ok(plan)) => (Some(plan), None),
        Ok(Err(e)) => (None, Some(e)),
        Err(panic) => (None, Some(format!("panic: {}", panic_message(&panic)))),
    };
    let response = Response {
        ok: plan.is_some(),
        plan,
        error,
        handler_ms: started.elapsed().as_millis() as u64,
    };
    serde_json::to_string(&response)
        .unwrap_or_else(|e| format!(r#"{{"ok":false,"error":"serialize: {e}","handlerMs":0}}"#))
}

fn handle_event(
    data_dir: &str,
    tz_id: &str,
    now_ms: i64,
    event_json: &str,
) -> Result<Plan, String> {
    let event: Event =
        serde_json::from_str(event_json).map_err(|e| format!("bad event {event_json}: {e}"))?;
    let tz = jiff::tz::TimeZone::get(tz_id).unwrap_or(jiff::tz::TimeZone::UTC);

    let mut cold = false;
    let mut plan = with_store(data_dir, |store, opened| {
        cold = opened;
        store.handle(now_ms, &tz, &event)
    })?;
    if cold {
        plan.log = format!("[cold init] {}", plan.log);
    }
    Ok(plan)
}

/// Recent spike log lines, newest first.
pub fn recent_log(data_dir: &str, limit: usize) -> Result<Vec<(i64, String, String)>, String> {
    with_store(data_dir, |store, _| store.recent_log(limit))
}

/// Runs `f` against the process-wide store, opening it on first use. Opening
/// happens under the same lock, so two threads can never hold two connections.
/// `f` gets `true` when this call opened the database.
fn with_store<T>(
    data_dir: &str,
    f: impl FnOnce(&mut SpikeStore, bool) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = SHARED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let opened = guard.is_none();
    if opened {
        let store = SpikeStore::open(&Path::new(data_dir).join("dun-spike.sqlite3"))?;
        *guard = Some(Shared {
            dir: PathBuf::from(data_dir),
            store,
        });
    }
    let shared = guard.as_mut().expect("opened above");
    if shared.dir != Path::new(data_dir) {
        return Err(format!(
            "data dir changed from {} to {data_dir}; refusing to open a second database",
            shared.dir.display()
        ));
    }
    f(&mut shared.store, opened)
}

fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown".into())
}

#[derive(Debug, Clone, PartialEq)]
struct SpikeRow {
    active: bool,
    occ: i64,
    next_at: i64,
    rings: i64,
}

pub struct SpikeStore {
    conn: Connection,
}

impl SpikeStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self, String> {
        Self::init(Connection::open_in_memory().map_err(|e| e.to_string())?)
    }

    fn init(conn: Connection) -> Result<Self, String> {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             CREATE TABLE IF NOT EXISTS spike(
               id INTEGER PRIMARY KEY CHECK(id = 1),
               active INTEGER NOT NULL, occ INTEGER NOT NULL,
               next_at INTEGER NOT NULL, rings INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS spike_log(
               at INTEGER NOT NULL, event TEXT NOT NULL, detail TEXT NOT NULL);",
        )
        .map_err(|e| format!("init schema: {e}"))?;
        Ok(Self { conn })
    }

    fn load(&self) -> Result<Option<SpikeRow>, String> {
        self.conn
            .query_row(
                "SELECT active, occ, next_at, rings FROM spike WHERE id = 1",
                [],
                |r| {
                    Ok(SpikeRow {
                        active: r.get(0)?,
                        occ: r.get(1)?,
                        next_at: r.get(2)?,
                        rings: r.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(|e| e.to_string())
    }

    fn save(&self, row: &SpikeRow) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO spike(id, active, occ, next_at, rings) VALUES (1, ?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET active=?1, occ=?2, next_at=?3, rings=?4",
                params![row.active, row.occ, row.next_at, row.rings],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn log(&self, now: i64, event: &Event, detail: &str) -> Result<(), String> {
        let name = serde_json::to_string(event).unwrap_or_default();
        self.conn
            .execute(
                "INSERT INTO spike_log(at, event, detail) VALUES (?1, ?2, ?3)",
                params![now, name, detail],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Recent log lines, newest first, for the spike screen and the spike doc.
    pub fn recent_log(&self, limit: usize) -> Result<Vec<(i64, String, String)>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT at, event, detail FROM spike_log ORDER BY rowid DESC LIMIT ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([limit as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
    }

    pub fn handle(
        &mut self,
        now: i64,
        tz: &jiff::tz::TimeZone,
        event: &Event,
    ) -> Result<Plan, String> {
        let mut plan = Plan::default();
        let row = self.load()?;

        let detail = match (event, row) {
            (Event::SpikeStart { delay_ms }, _) => {
                let occ = now + (*delay_ms).max(0);
                self.save(&SpikeRow {
                    active: true,
                    occ,
                    next_at: occ,
                    rings: 0,
                })?;
                plan.cancel.push(SPIKE_NOTIF);
                plan.next_wake_at = Some(occ);
                format!("armed for {}", clock(occ, tz))
            }

            (_, None) | (_, Some(SpikeRow { active: false, .. })) => {
                plan.cancel.push(SPIKE_NOTIF);
                "nothing active".to_string()
            }

            (
                Event::Action {
                    item_id,
                    occ,
                    button,
                },
                Some(mut r),
            ) => {
                if item_id != SPIKE_ITEM || *occ != r.occ {
                    plan.next_wake_at = Some(r.next_at);
                    format!("stale action for occ {occ}; current {}", r.occ)
                } else {
                    plan.cancel.push(SPIKE_NOTIF);
                    match button {
                        Button::Done => {
                            r.active = false;
                            self.save(&r)?;
                            format!("done after {} rings", r.rings)
                        }
                        Button::Snooze5 | Button::Snooze15 => {
                            let mins = if *button == Button::Snooze5 { 5 } else { 15 };
                            r.next_at = now + mins * 60_000;
                            self.save(&r)?;
                            plan.next_wake_at = Some(r.next_at);
                            format!("snoozed {mins}m to {}", clock(r.next_at, tz))
                        }
                    }
                }
            }

            (Event::Dismissed { .. }, Some(r)) => {
                // Swiping away doesn't stop nagging; the next alarm stays armed.
                plan.next_wake_at = Some(r.next_at);
                format!("dismissed; next nag {}", clock(r.next_at, tz))
            }

            (
                Event::Alarm { .. }
                | Event::Reschedule { .. }
                | Event::AppStarted
                | Event::SyncOnly,
                Some(mut r),
            ) => {
                if now + EARLY_TOLERANCE_MS >= r.next_at {
                    let late = now - r.next_at;
                    r.rings += 1;
                    r.next_at = now + NAG_INTERVAL_MS;
                    self.save(&r)?;
                    let missed = matches!(event, Event::Reschedule { .. } | Event::AppStarted)
                        && late > 90_000;
                    let label = if missed {
                        format!("Missed at {}", clock(r.occ, tz))
                    } else {
                        format!("Due {}", clock(r.occ, tz))
                    };
                    plan.post.push(Post {
                        notif_id: SPIKE_NOTIF,
                        item_id: SPIKE_ITEM.into(),
                        occ: r.occ,
                        channel: "ring_default".into(),
                        silent: false,
                        title: "Dun test ring".into(),
                        text: format!("{label} · ring #{}", r.rings),
                        notes: Some(format!(
                            "Fired {} late. Next nag {}.",
                            secs(late),
                            clock(r.next_at, tz)
                        )),
                        when: Some(r.occ),
                        actions: Button::RING.to_vec(),
                    });
                    plan.next_wake_at = Some(r.next_at);
                    format!("ring #{} ({} late)", r.rings, secs(late))
                } else {
                    plan.next_wake_at = Some(r.next_at);
                    format!("early by {}; re-armed", secs(r.next_at - now))
                }
            }
        };

        self.log(now, event, &detail)?;
        plan.log = detail;
        Ok(plan)
    }
}

fn clock(ms: i64, tz: &jiff::tz::TimeZone) -> String {
    jiff::Timestamp::from_millisecond(ms)
        .map(|t| t.to_zoned(tz.clone()).strftime("%H:%M:%S").to_string())
        .unwrap_or_else(|_| ms.to_string())
}

fn secs(ms: i64) -> String {
    format!("{:.1}s", ms as f64 / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_789_500_000_000;

    fn store() -> SpikeStore {
        SpikeStore::open_in_memory().unwrap()
    }

    fn utc() -> jiff::tz::TimeZone {
        jiff::tz::TimeZone::UTC
    }

    #[test]
    fn start_arms_alarm_without_ringing() {
        let mut s = store();
        let plan = s
            .handle(T0, &utc(), &Event::SpikeStart { delay_ms: 30_000 })
            .unwrap();
        assert!(plan.post.is_empty());
        assert_eq!(plan.next_wake_at, Some(T0 + 30_000));
    }

    #[test]
    fn alarm_rings_and_nags_every_minute_until_done() {
        let mut s = store();
        s.handle(T0, &utc(), &Event::SpikeStart { delay_ms: 30_000 })
            .unwrap();

        let ring1 = s
            .handle(T0 + 30_200, &utc(), &Event::Alarm { late_by_ms: None })
            .unwrap();
        assert_eq!(ring1.post.len(), 1);
        assert_eq!(ring1.post[0].actions, Button::RING.to_vec());
        assert_eq!(ring1.next_wake_at, Some(T0 + 30_200 + 60_000));

        let ring2 = s
            .handle(T0 + 90_200, &utc(), &Event::Alarm { late_by_ms: None })
            .unwrap();
        assert!(ring2.post[0].text.contains("ring #2"));

        let done = s
            .handle(
                T0 + 95_000,
                &utc(),
                &Event::Action {
                    item_id: SPIKE_ITEM.into(),
                    occ: T0 + 30_000,
                    button: Button::Done,
                },
            )
            .unwrap();
        assert_eq!(done.cancel, vec![SPIKE_NOTIF]);
        assert_eq!(done.next_wake_at, None);

        let after = s
            .handle(T0 + 200_000, &utc(), &Event::Alarm { late_by_ms: None })
            .unwrap();
        assert!(after.post.is_empty());
        assert_eq!(after.next_wake_at, None);
    }

    #[test]
    fn dismiss_keeps_nagging_and_snooze_pushes_out() {
        let mut s = store();
        s.handle(T0, &utc(), &Event::SpikeStart { delay_ms: 0 })
            .unwrap();
        s.handle(T0, &utc(), &Event::Alarm { late_by_ms: None })
            .unwrap();

        let dismissed = s
            .handle(
                T0 + 5_000,
                &utc(),
                &Event::Dismissed {
                    item_id: SPIKE_ITEM.into(),
                    occ: T0,
                },
            )
            .unwrap();
        assert_eq!(dismissed.next_wake_at, Some(T0 + 60_000));

        let snoozed = s
            .handle(
                T0 + 10_000,
                &utc(),
                &Event::Action {
                    item_id: SPIKE_ITEM.into(),
                    occ: T0,
                    button: Button::Snooze15,
                },
            )
            .unwrap();
        assert_eq!(snoozed.next_wake_at, Some(T0 + 10_000 + 15 * 60_000));
    }

    #[test]
    fn stale_action_from_old_occurrence_is_ignored() {
        let mut s = store();
        s.handle(T0, &utc(), &Event::SpikeStart { delay_ms: 0 })
            .unwrap();
        s.handle(T0 + 1_000, &utc(), &Event::SpikeStart { delay_ms: 0 })
            .unwrap();
        let plan = s
            .handle(
                T0 + 2_000,
                &utc(),
                &Event::Action {
                    item_id: SPIKE_ITEM.into(),
                    occ: T0,
                    button: Button::Done,
                },
            )
            .unwrap();
        assert!(plan.log.starts_with("stale"));
        assert!(
            plan.next_wake_at.is_some(),
            "the current occurrence must stay armed"
        );
    }

    #[test]
    fn early_alarm_rearms_without_ringing() {
        let mut s = store();
        s.handle(T0, &utc(), &Event::SpikeStart { delay_ms: 60_000 })
            .unwrap();
        let plan = s
            .handle(T0 + 50_000, &utc(), &Event::Alarm { late_by_ms: None })
            .unwrap();
        assert!(plan.post.is_empty());
        assert_eq!(plan.next_wake_at, Some(T0 + 60_000));
    }

    #[test]
    fn reboot_after_due_rings_as_missed() {
        let mut s = store();
        s.handle(T0, &utc(), &Event::SpikeStart { delay_ms: 0 })
            .unwrap();
        let plan = s
            .handle(
                T0 + 10 * 60_000,
                &utc(),
                &Event::Reschedule {
                    reason: "boot".into(),
                },
            )
            .unwrap();
        assert!(plan.post[0].text.starts_with("Missed at"));
        assert_eq!(s.recent_log(10).unwrap().len(), 2);
    }

    #[test]
    fn json_entry_point_reports_errors_instead_of_panicking() {
        let dir = std::env::temp_dir().join(format!("dun-bridge-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = handle_event_json(dir.to_str().unwrap(), "UTC", T0, "{not json");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("bad event"));

        let out = handle_event_json(
            dir.to_str().unwrap(),
            "Nowhere/Invalid",
            T0,
            r#"{"type":"appStarted"}"#,
        );
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["ok"], true, "unknown tz falls back to UTC: {out}");
    }
}
