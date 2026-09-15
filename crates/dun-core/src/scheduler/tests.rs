use jiff::civil::{date, time, DateTime};

use super::*;
use crate::actions::{self, Change, ItemDraft};
use crate::hlc::Hlc;
use crate::model::{Schedule, TimerState};
use crate::quiet::QuietHours;
use crate::recurrence::{Recurrence, Rule};
use crate::sync::Reg;
use crate::time::{resolve, HOUR};

/// Drives State + Scheduler the way the app does, without a database.
struct Harness {
    state: State,
    sched: Scheduler,
    tz: TimeZone,
    now: Timestamp,
    tick: u16,
}

impl Harness {
    fn at(dt: DateTime) -> Self {
        let tz = TimeZone::UTC;
        Harness {
            now: resolve(dt, &tz),
            state: State::default(),
            sched: Scheduler::new(),
            tz,
            tick: 0,
        }
    }

    fn apply(&mut self, change: Change) {
        for w in change.writes {
            self.tick = self.tick.wrapping_add(1);
            self.state.apply(&Reg {
                entity: w.entity,
                id: w.id,
                field: w.field,
                value: w.value,
                hlc: Hlc::new(self.now.0, self.tick),
                device: "pc".into(),
            });
        }
    }

    fn create(&mut self, id: &str, draft: ItemDraft) {
        let c = actions::create_item(&self.state, "pc", self.now, id, draft).unwrap();
        self.apply(c);
    }

    fn set_to(&mut self, dt: DateTime) {
        self.now = resolve(dt, &self.tz);
    }

    fn advance(&mut self, ms: i64) {
        self.now = self.now.plus(ms);
    }

    fn eval(&mut self) -> Vec<Effect> {
        self.sched.evaluate(&self.state, self.now, &self.tz)
    }

    fn done(&mut self, id: &str) {
        let c = actions::done(&self.state, self.now, &self.tz, id, None).unwrap();
        self.apply(c);
    }

    fn snooze(&mut self, id: &str, minutes: u32) {
        let c = actions::snooze(&self.state, self.now, &self.tz, id, None, minutes).unwrap();
        self.apply(c);
    }
}

fn reminder(title: &str, due: Timestamp) -> ItemDraft {
    ItemDraft {
        title: title.into(),
        notes: String::new(),
        tag: None,
        schedule: Schedule::OneOff { due },
        nag: None,
        chime: None,
        quiet_exempt: None,
        start_timer: false,
    }
}

fn alerts(effects: &[Effect]) -> Vec<(&str, AlertLevel)> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::ShowAlert { item_id, level, .. } => Some((item_id.as_str(), *level)),
            _ => None,
        })
        .collect()
}

fn cleared(effects: &[Effect]) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::ClearAlert { item_id } => Some(item_id.as_str()),
            _ => None,
        })
        .collect()
}

fn wake(effects: &[Effect]) -> Option<Timestamp> {
    effects.iter().find_map(|e| match e {
        Effect::ScheduleWake { at } => Some(*at),
        _ => None,
    })
}

fn chimes(effects: &[Effect]) -> usize {
    effects
        .iter()
        .filter(|e| matches!(e, Effect::PlayChime { .. }))
        .count()
}

fn label(effects: &[Effect], id: &str) -> AlertLabel {
    effects
        .iter()
        .find_map(|e| match e {
            Effect::ShowAlert { item_id, label, .. } if item_id == id => Some(*label),
            _ => None,
        })
        .expect("alert for item")
}

#[test]
fn nags_every_minute_until_done() {
    let mut h = Harness::at(date(2026, 9, 16).at(8, 59, 0, 0));
    let due = h.now.plus(MINUTE);
    h.create("rent", reminder("Pay rent", due));

    let e = h.eval();
    assert!(alerts(&e).is_empty());
    assert_eq!(wake(&e), Some(due));

    h.set_to(date(2026, 9, 16).at(9, 0, 0, 0));
    let e = h.eval();
    assert_eq!(alerts(&e), [("rent", AlertLevel::Full)]);
    assert_eq!(chimes(&e), 1);
    assert_eq!(wake(&e), Some(h.now.plus(MINUTE)));
    assert_eq!(label(&e, "rent").describe(h.now, &h.tz), "Due 9:00 AM");

    // Re-evaluating before the minute is up doesn't re-alert.
    h.advance(20 * SECOND);
    assert!(alerts(&h.eval()).is_empty());

    h.advance(40 * SECOND);
    assert_eq!(alerts(&h.eval()), [("rent", AlertLevel::Full)]);
    assert_eq!(h.sched.rings()["rent"].alerts, 2);

    h.done("rent");
    let e = h.eval();
    assert_eq!(cleared(&e), ["rent"]);
    assert!(h.sched.rings().is_empty());
    assert_eq!(wake(&e), None);
}

#[test]
fn ring_once_alerts_a_single_time() {
    let mut h = Harness::at(date(2026, 9, 16).at(9, 0, 0, 0));
    let draft = ItemDraft {
        nag: Some(Nag::Once),
        ..reminder("FYI", h.now)
    };
    h.create("fyi", draft);
    assert_eq!(alerts(&h.eval()).len(), 1);
    h.advance(10 * MINUTE);
    let e = h.eval();
    assert!(alerts(&e).is_empty());
    assert_eq!(wake(&e), None, "nothing left to wake for");
    assert_eq!(
        h.sched.rings().len(),
        1,
        "still listed as ringing until done"
    );
}

#[test]
fn custom_nag_interval_and_edits_apply_from_last_alert() {
    let mut h = Harness::at(date(2026, 9, 16).at(9, 0, 0, 0));
    let draft = ItemDraft {
        nag: Some(Nag::Repeat { interval_min: 10 }),
        ..reminder("Water plants", h.now)
    };
    h.create("w", draft.clone());
    h.eval();
    h.advance(3 * MINUTE);
    assert!(alerts(&h.eval()).is_empty());

    // Shorten to 2 minutes: the next nag is now overdue.
    let c = actions::update_item(
        &h.state,
        h.now,
        "w",
        ItemDraft {
            nag: Some(Nag::Repeat { interval_min: 2 }),
            ..draft
        },
    )
    .unwrap();
    h.apply(c);
    assert_eq!(alerts(&h.eval()).len(), 1);
}

#[test]
fn snooze_clears_then_rings_again_when_it_ends() {
    let mut h = Harness::at(date(2026, 9, 16).at(9, 0, 0, 0));
    h.create("a", reminder("Stretch", h.now));
    h.eval();

    h.advance(30 * SECOND);
    h.snooze("a", 5);
    let e = h.eval();
    assert_eq!(cleared(&e), ["a"]);
    assert_eq!(wake(&e), Some(h.now.plus(5 * MINUTE)));

    h.advance(5 * MINUTE);
    let e = h.eval();
    assert_eq!(alerts(&e), [("a", AlertLevel::Full)]);
    assert_eq!(label(&e, "a").snooze_count, 1);
}

#[test]
fn rings_found_late_are_labelled_missed() {
    let mut h = Harness::at(date(2026, 9, 16).at(9, 0, 0, 0));
    h.create("a", reminder("Call", h.now));
    // The PC was off; first evaluation is 2 hours later.
    h.advance(2 * HOUR);
    let e = h.eval();
    assert_eq!(label(&e, "a").describe(h.now, &h.tz), "Missed 9:00 AM");
    assert!(h.sched.rings()["a"].missed);
}

#[test]
fn collapsed_recurring_slots_report_the_count() {
    let mut h = Harness::at(date(2026, 9, 12).at(12, 0, 0, 0));
    let draft = ItemDraft {
        schedule: Schedule::Recurring {
            recurrence: Recurrence {
                rule: Rule::Daily {
                    every: 1,
                    times: vec![time(9, 0, 0, 0)],
                },
                start: date(2026, 9, 1).at(0, 0, 0, 0),
                tz: None,
            },
            mode: Default::default(),
            effective_from: Timestamp(0),
        },
        ..reminder("Meds", Timestamp(0))
    };
    h.create("meds", draft);
    h.set_to(date(2026, 9, 15).at(10, 0, 0, 0));
    let e = h.eval();
    assert_eq!(
        label(&e, "meds").describe(h.now, &h.tz),
        "Missed 3 times since Sun 9:00 AM"
    );
}

#[test]
fn quiet_hours_hold_reminders_but_not_timers() {
    let mut h = Harness::at(date(2026, 9, 16).at(23, 0, 0, 0));
    h.apply(settings_change(
        crate::model::item::setting_key::QUIET_HOURS,
        QuietHours {
            enabled: true,
            start: time(22, 30, 0, 0),
            end: time(7, 0, 0, 0),
        },
    ));
    let at_2330 = h.now.plus(30 * MINUTE);
    h.create("r", reminder("Take bins out", at_2330));
    h.create(
        "t",
        ItemDraft {
            schedule: Schedule::Timer {
                duration_ms: 30 * MINUTE,
            },
            start_timer: true,
            ..reminder("Tea", Timestamp(0))
        },
    );

    h.set_to(date(2026, 9, 16).at(23, 30, 0, 0));
    let e = h.eval();
    assert_eq!(
        alerts(&e),
        [("t", AlertLevel::Full)],
        "timer rings, reminder is held"
    );
    let morning = resolve(date(2026, 9, 17).at(7, 0, 0, 0), &h.tz);
    assert_eq!(h.sched.rings()["r"].hold, Some(Hold::Quiet));
    assert_eq!(h.sched.rings()["r"].next_alert_at, morning);

    h.done("t");
    h.eval();
    h.set_to(date(2026, 9, 17).at(3, 0, 0, 0));
    assert!(alerts(&h.eval()).is_empty());

    h.set_to(date(2026, 9, 17).at(7, 0, 0, 0));
    let e = h.eval();
    assert_eq!(alerts(&e), [("r", AlertLevel::Full)]);
    assert!(label(&e, "r")
        .describe(h.now, &h.tz)
        .ends_with("held for quiet hours"));
}

#[test]
fn turning_quiet_hours_off_releases_held_rings() {
    let mut h = Harness::at(date(2026, 9, 16).at(23, 0, 0, 0));
    let q = QuietHours {
        enabled: true,
        start: time(22, 0, 0, 0),
        end: time(7, 0, 0, 0),
    };
    h.apply(settings_change(
        crate::model::item::setting_key::QUIET_HOURS,
        q,
    ));
    h.create("r", reminder("x", h.now));
    assert!(alerts(&h.eval()).is_empty());

    h.advance(MINUTE);
    h.apply(settings_change(
        crate::model::item::setting_key::QUIET_HOURS,
        QuietHours {
            enabled: false,
            ..q
        },
    ));
    assert_eq!(alerts(&h.eval()), [("r", AlertLevel::Full)]);
}

#[test]
fn mute_shows_silently_once_then_rings_when_it_ends_or_is_cancelled() {
    let mut h = Harness::at(date(2026, 9, 16).at(9, 0, 0, 0));
    let until = h.now.plus(HOUR);
    h.apply(settings_change(
        crate::model::item::setting_key::MUTE_UNTIL,
        Some(until),
    ));
    h.create("a", reminder("A", h.now));
    h.create("b", reminder("B", h.now));

    let e = h.eval();
    assert_eq!(
        alerts(&e),
        [("a", AlertLevel::Silent), ("b", AlertLevel::Silent)]
    );
    assert_eq!(chimes(&e), 0);
    assert_eq!(wake(&e), Some(until));

    h.advance(10 * MINUTE);
    assert!(alerts(&h.eval()).is_empty(), "silent alert isn't repeated");

    // Cancel mute early: everything held rings now.
    h.apply(settings_change(
        crate::model::item::setting_key::MUTE_UNTIL,
        Option::<Timestamp>::None,
    ));
    let e = h.eval();
    assert_eq!(
        alerts(&e),
        [("a", AlertLevel::Full), ("b", AlertLevel::Full)]
    );
    assert_eq!(
        chimes(&e),
        1,
        "one chime per evaluation, however many items ring"
    );
}

#[test]
fn ring_states_survive_a_restart_without_re_alerting() {
    let mut h = Harness::at(date(2026, 9, 16).at(9, 0, 0, 0));
    h.create("a", reminder("A", h.now));
    h.eval();
    let saved = serde_json::to_string(h.sched.rings()).unwrap();

    h.sched = Scheduler::from_rings(serde_json::from_str(&saved).unwrap());
    h.advance(10 * SECOND);
    assert!(alerts(&h.eval()).is_empty());
    h.advance(50 * SECOND);
    assert_eq!(alerts(&h.eval()).len(), 1);
}

#[test]
fn deleting_or_finishing_a_timer_clears_its_alert() {
    let mut h = Harness::at(date(2026, 9, 16).at(9, 0, 0, 0));
    h.create(
        "t",
        ItemDraft {
            schedule: Schedule::Timer {
                duration_ms: MINUTE,
            },
            start_timer: true,
            ..reminder("Eggs", Timestamp(0))
        },
    );
    h.create("r", reminder("Other", h.now));
    h.advance(MINUTE);
    assert_eq!(alerts(&h.eval()).len(), 2);

    let del = actions::set_deleted(&h.state, "r", true).unwrap();
    h.apply(del);
    h.done("t");
    let e = h.eval();
    let mut c = cleared(&e);
    c.sort();
    assert_eq!(c, ["r", "t"]);
    assert_eq!(h.state.items["t"].timer, TimerState::Idle);
}

#[test]
fn tray_reports_ringing_count_and_next_up() {
    let mut h = Harness::at(date(2026, 9, 16).at(9, 0, 0, 0));
    h.create("now", reminder("Now", h.now));
    let later = h.now.plus(2 * HOUR);
    h.create("later", reminder("Later", later));
    let e = h.eval();
    let tray = e
        .iter()
        .find_map(|e| match e {
            Effect::TrayStatus { ringing, next } => Some((*ringing, next.clone())),
            _ => None,
        })
        .unwrap();
    assert_eq!(tray, (1, Some(("Later".to_string(), later))));
}

fn settings_change<T: serde::Serialize>(key: &str, value: T) -> Change {
    Change {
        writes: vec![crate::store::NewWrite::new(
            crate::sync::Entity::Setting,
            crate::sync::field::SETTINGS_ID,
            key,
            value,
        )],
        history: Vec::new(),
    }
}
