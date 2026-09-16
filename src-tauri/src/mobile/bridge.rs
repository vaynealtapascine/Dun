//! Entry point shared by Android's background receivers (through JNI) and the
//! in-app command path.
//!
//! One event in, one [`Plan`] out: apply what the user pressed, check in with
//! the PC if anything is about to ring or is waiting to be pushed, evaluate as
//! the phone, and hand Kotlin the exact notifications and next alarm. Kotlin
//! decides nothing.

use std::time::Instant;

use dun_core::engine::Engine;
use dun_core::model::ChimeRef;
use dun_core::scheduler::{AlertLevel, Effect, Role};
use dun_core::sync::protocol::DueItem;
use dun_core::time::{TimeZone, Timestamp};

use super::core;
use super::plan::{Button, Event, Plan, Post, Response};
use super::sync::{self, CheckIn};

/// Reserved notification ids. Item notifications hash into everything else.
pub const SUMMARY_NOTIF_ID: i32 = 1;
/// Kotlin's fail-loud notification uses `Int.MAX_VALUE`.
const RESERVED_HIGH: i32 = i32::MAX;

/// Channel names Kotlin creates; `ring_<chime>` for the bundled chimes.
pub const CHANNEL_SILENT: &str = "ring_silent";
pub const CHANNEL_DEFAULT: &str = "ring_default";

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

pub fn handle_event(
    data_dir: &str,
    tz_id: &str,
    now_ms: i64,
    event_json: &str,
) -> Result<Plan, String> {
    let event: Event =
        serde_json::from_str(event_json).map_err(|e| format!("bad event {event_json}: {e}"))?;
    let tz = core::zone(tz_id);
    let now = core::now(now_ms);

    // 1. What the user pressed, if anything.
    core::with_engine(data_dir, |engine| apply(engine, &event, now, &tz))??;

    // 2. What we would alert, and who we sync with.
    let (device_id, peer, due, needs_sync) = core::with_engine(data_dir, |engine| {
        let due: Vec<DueItem> = engine
            .due_for_alert(now, &tz)
            .into_iter()
            .map(|(item, occurrence)| DueItem { item, occurrence })
            .collect();
        let peer = sync::paired_pc(&engine.peers());
        let needs_sync = peer
            .as_ref()
            .is_some_and(|p| sync::should_sync(engine, p, &due));
        (engine.device_id().to_string(), peer, due, needs_sync)
    })?;

    // 3. Check in before alerting, so the PC can stay quiet while we cover it.
    let check_in = match (peer, needs_sync || matches!(event, Event::SyncOnly)) {
        (Some(peer), true) => sync::check_in(data_dir, device_id, peer, due, now, &tz),
        (Some(_), false) => CheckIn {
            note: "nothing to sync".into(),
            ..Default::default()
        },
        (None, _) => CheckIn {
            note: "no PC paired".into(),
            ..Default::default()
        },
    };

    // 4. Decide, and turn the decision into notifications.
    core::with_engine(data_dir, |engine| {
        let effects = engine
            .evaluate(
                now,
                &tz,
                Role::Phone {
                    pc: check_in.pc.as_ref(),
                },
            )
            .map_err(|e| e.to_string())?;
        Ok(plan_from(&effects, engine, &check_in, now, &tz))
    })?
}

/// Decides and plans without syncing: used after a local change, where the
/// push follows separately so the UI never waits on the network.
pub fn plan_now(data_dir: &str, tz_id: &str, now_ms: i64) -> Result<Plan, String> {
    let tz = core::zone(tz_id);
    let now = core::now(now_ms);
    core::with_engine(data_dir, |engine| {
        let effects = engine
            .evaluate(now, &tz, Role::Phone { pc: None })
            .map_err(|e| e.to_string())?;
        Ok(plan_from(&effects, engine, &CheckIn::default(), now, &tz))
    })?
}

/// Applies a button press. Everything else only moves time along.
fn apply(engine: &mut Engine, event: &Event, now: Timestamp, tz: &TimeZone) -> Result<(), String> {
    match event {
        Event::Action {
            item_id,
            occ,
            button,
        } => {
            let occurrence = Some(Timestamp(*occ));
            let result = match button {
                Button::Done => engine.done(now, tz, item_id, occurrence),
                Button::Snooze5 => engine.snooze(now, tz, item_id, occurrence, 5),
                Button::Snooze15 => engine.snooze(now, tz, item_id, occurrence, 15),
            };
            match result {
                // The item may have been finished or deleted on the PC in the
                // meantime; the notification is simply stale.
                Err(e) => Err(format!("{button:?} on {item_id}: {e}")),
                Ok(()) => Ok(()),
            }
        }
        // Swiping away doesn't count as done: the nagging continues.
        Event::Dismissed { .. }
        | Event::Alarm { .. }
        | Event::Reschedule { .. }
        | Event::AppStarted
        | Event::SyncOnly => Ok(()),
    }
}

fn plan_from(
    effects: &[Effect],
    engine: &Engine,
    check_in: &CheckIn,
    now: Timestamp,
    tz: &TimeZone,
) -> Plan {
    let default_chime = engine
        .store()
        .local_get::<ChimeRef>("chime")
        .ok()
        .flatten()
        .unwrap_or(ChimeRef::Bundled { id: "bell".into() });

    let mut plan = Plan {
        log: check_in.note.clone(),
        pending_push: check_in.pending_push,
        ..Plan::default()
    };

    for effect in effects {
        match effect {
            Effect::ShowAlert {
                item_id,
                occurrence,
                level,
                title,
                notes,
                label,
            } => {
                let item = engine.state().items.get(item_id);
                let chime = item
                    .and_then(|i| i.chime.clone())
                    .unwrap_or_else(|| default_chime.clone());
                plan.post.push(Post {
                    notif_id: notif_id(item_id),
                    item_id: item_id.clone(),
                    occ: occurrence.0,
                    channel: channel_for(&chime, *level),
                    silent: *level == AlertLevel::Silent,
                    title: title.clone(),
                    text: label.describe(now, tz),
                    notes: (!notes.is_empty()).then(|| notes.clone()),
                    when: Some(occurrence.0),
                    actions: Button::RING.to_vec(),
                });
            }
            Effect::ClearAlert { item_id } => plan.cancel.push(notif_id(item_id)),
            Effect::ShowSummary { items, level } => {
                let titles: Vec<&str> = items.iter().map(|(_, title)| title.as_str()).collect();
                plan.post.push(Post {
                    notif_id: SUMMARY_NOTIF_ID,
                    item_id: String::new(),
                    occ: 0,
                    channel: channel_for(&default_chime, *level),
                    silent: *level == AlertLevel::Silent,
                    title: format!("{} missed reminders", items.len()),
                    text: titles.join(", "),
                    notes: None,
                    when: None,
                    // Tapping opens Dun; there's nothing sensible to press per item.
                    actions: Vec::new(),
                });
            }
            Effect::ClearSummary => plan.cancel.push(SUMMARY_NOTIF_ID),
            Effect::ScheduleWake { at } => plan.next_wake_at = Some(at.0),
            // The channel carries the sound on Android, and there's no tray.
            Effect::PlayChime { .. } | Effect::TrayStatus { .. } => {}
        }
    }
    plan
}

/// Bundled chimes get their own channel (Android ties sounds to channels);
/// anything else falls back to the default one.
fn channel_for(chime: &ChimeRef, level: AlertLevel) -> String {
    if level == AlertLevel::Silent {
        return CHANNEL_SILENT.to_string();
    }
    match chime {
        ChimeRef::Bundled { id } if is_bundled(id) => format!("ring_{id}"),
        _ => CHANNEL_DEFAULT.to_string(),
    }
}

fn is_bundled(id: &str) -> bool {
    matches!(id, "bell" | "rise" | "pulse" | "soft")
}

/// A stable notification id per item: FNV-1a, folded into a positive i32 that
/// avoids the reserved ids.
pub fn notif_id(item_id: &str) -> i32 {
    let mut hash: u32 = 2_166_136_261;
    for byte in item_id.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    let id = (hash & 0x7fff_ffff) as i32;
    match id {
        SUMMARY_NOTIF_ID | RESERVED_HIGH | 0 => id.wrapping_add(2).abs(),
        other => other,
    }
}

fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dun_core::actions::ItemDraft;
    use dun_core::model::Schedule;
    use dun_core::time::{resolve, MINUTE, SECOND};
    use jiff::civil::date;

    const UTC_ID: &str = "UTC";

    fn dir() -> tempfile::TempDir {
        core::reset();
        tempfile::tempdir().unwrap()
    }

    fn reminder(title: &str, due: Timestamp) -> ItemDraft {
        ItemDraft {
            title: title.into(),
            notes: "take the blue one".into(),
            tag: None,
            schedule: Schedule::OneOff { due },
            nag: None,
            chime: None,
            quiet_exempt: None,
            start_timer: false,
        }
    }

    fn event(data_dir: &std::path::Path, now: Timestamp, json: &str) -> Plan {
        let raw = handle_event_json(&data_dir.display().to_string(), UTC_ID, now.0, json);
        let response: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(response["ok"], true, "event failed: {raw}");
        serde_json::from_value(response["plan"].clone()).unwrap()
    }

    #[test]
    fn an_alarm_posts_the_ringing_item_and_the_next_wake() {
        let dir = dir();
        let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &TimeZone::UTC);
        let id = core::with_engine(&dir.path().display().to_string(), |engine| {
            engine
                .create_item(t0.minus(MINUTE), reminder("Meds", t0))
                .unwrap()
        })
        .unwrap();

        let plan = event(dir.path(), t0, r#"{"type":"alarm","lateByMs":40}"#);
        assert_eq!(plan.post.len(), 1);
        let post = &plan.post[0];
        assert_eq!(post.item_id, id);
        assert_eq!(post.occ, t0.0);
        assert_eq!(post.title, "Meds");
        assert_eq!(post.text, "Due 9:00 AM");
        assert_eq!(post.notes.as_deref(), Some("take the blue one"));
        assert_eq!(post.channel, "ring_bell");
        assert!(!post.silent);
        assert_eq!(post.actions, Button::RING.to_vec());
        // Nagging every minute by default.
        assert_eq!(plan.next_wake_at, Some(t0.plus(MINUTE).0));
    }

    #[test]
    fn done_from_a_notification_clears_it_and_stops_the_alarm() {
        let dir = dir();
        let path = dir.path().display().to_string();
        let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &TimeZone::UTC);
        let id = core::with_engine(&path, |engine| {
            engine
                .create_item(t0.minus(MINUTE), reminder("Meds", t0))
                .unwrap()
        })
        .unwrap();
        event(dir.path(), t0, r#"{"type":"alarm"}"#);

        let plan = event(
            dir.path(),
            t0.plus(10 * SECOND),
            &format!(
                r#"{{"type":"action","itemId":"{id}","occ":{},"button":"done"}}"#,
                t0.0
            ),
        );
        assert_eq!(plan.cancel, [notif_id(&id)]);
        assert!(plan.post.is_empty());
        assert_eq!(plan.next_wake_at, None, "nothing left to ring");
    }

    #[test]
    fn snooze_moves_the_alarm_and_says_so_next_time() {
        let dir = dir();
        let path = dir.path().display().to_string();
        let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &TimeZone::UTC);
        let id = core::with_engine(&path, |engine| {
            engine
                .create_item(t0.minus(MINUTE), reminder("Stretch", t0))
                .unwrap()
        })
        .unwrap();
        event(dir.path(), t0, r#"{"type":"alarm"}"#);

        let plan = event(
            dir.path(),
            t0,
            &format!(
                r#"{{"type":"action","itemId":"{id}","occ":{},"button":"snooze5"}}"#,
                t0.0
            ),
        );
        assert_eq!(plan.cancel, [notif_id(&id)]);
        assert_eq!(plan.next_wake_at, Some(t0.plus(5 * MINUTE).0));

        let later = event(dir.path(), t0.plus(5 * MINUTE), r#"{"type":"alarm"}"#);
        assert_eq!(later.post.len(), 1);
        assert!(
            later.post[0].text.contains("snoozed 1×"),
            "{}",
            later.post[0].text
        );
    }

    #[test]
    fn swiping_a_notification_away_does_not_stop_the_nagging() {
        let dir = dir();
        let path = dir.path().display().to_string();
        let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &TimeZone::UTC);
        let id = core::with_engine(&path, |engine| {
            engine
                .create_item(t0.minus(MINUTE), reminder("Meds", t0))
                .unwrap()
        })
        .unwrap();
        event(dir.path(), t0, r#"{"type":"alarm"}"#);

        let plan = event(
            dir.path(),
            t0.plus(SECOND),
            &format!(r#"{{"type":"dismissed","itemId":"{id}","occ":{}}}"#, t0.0),
        );
        assert!(plan.cancel.is_empty());
        assert_eq!(
            plan.next_wake_at,
            Some(t0.plus(MINUTE).0),
            "still due to nag"
        );
    }

    #[test]
    fn a_reboot_reschedules_from_what_is_stored() {
        let dir = dir();
        let path = dir.path().display().to_string();
        let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &TimeZone::UTC);
        core::with_engine(&path, |engine| {
            engine
                .create_item(t0, reminder("Later today", t0.plus(120 * MINUTE)))
                .unwrap()
        })
        .unwrap();

        // A fresh process (no in-memory state) still knows when to wake.
        core::reset();
        let plan = event(
            dir.path(),
            t0.plus(MINUTE),
            r#"{"type":"reschedule","reason":"boot"}"#,
        );
        assert!(plan.post.is_empty());
        assert_eq!(plan.next_wake_at, Some(t0.plus(120 * MINUTE).0));
    }

    #[test]
    fn a_bad_event_fails_loud_rather_than_panicking() {
        let dir = dir();
        let raw = handle_event_json(&dir.path().display().to_string(), UTC_ID, 0, "not json");
        let response: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(response["ok"], false);
        assert!(response["error"].as_str().unwrap().contains("bad event"));
    }

    #[test]
    fn notification_ids_are_stable_and_avoid_the_reserved_ones() {
        assert_eq!(notif_id("abc"), notif_id("abc"));
        assert_ne!(notif_id("abc"), notif_id("abd"));
        for id in ["", "a", "item-1", "01a0a699-4c73-70de-962a-c0e5a4b7d32a"] {
            let n = notif_id(id);
            assert!(n > 0, "{id} -> {n}");
            assert_ne!(n, SUMMARY_NOTIF_ID);
            assert_ne!(n, RESERVED_HIGH);
        }
    }
}
