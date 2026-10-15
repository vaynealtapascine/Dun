//! Read-only widget projection. Alert delivery and recurrence stay in the core.
use dun_core::engine::Engine;
use dun_core::model::{Schedule, TimerState};
use dun_core::occurrence::{status, Status};
use dun_core::time::{TimeZone, Timestamp};

use super::plan::{WidgetCountdown, WidgetItem};

pub fn items(engine: &Engine, now: Timestamp, tz: &TimeZone) -> Vec<WidgetItem> {
    let mut items: Vec<_> = engine
        .state()
        .items
        .values()
        .filter(|i| !i.deleted)
        .map(|item| {
            let timer = matches!(item.schedule, Some(Schedule::Timer { .. }));
            let mut view = WidgetItem {
                id: item.id.clone(),
                title: item.title.clone(),
                timer,
                state: "unavailable".into(),
                at: None,
                remaining_ms: None,
                occurrence: None,
                repeating_reminder: matches!(item.schedule, Some(Schedule::Recurring { .. })),
                countdown: None,
            };
            if let Some(inputs) = item.inputs() {
                match status(inputs, now, tz) {
                    Status::Upcoming { at } => {
                        view.state = "upcoming".into();
                        view.at = Some(at.0);
                    }
                    Status::Due {
                        occ,
                        rings_at,
                        snoozed,
                    } => {
                        view.state = if snoozed { "snoozed" } else { "due" }.into();
                        view.at = Some(rings_at.0);
                        view.occurrence = Some(occ.due.0);
                    }
                    Status::Idle => {
                        view.state = if timer { "ready" } else { "done" }.into();
                        if let TimerState::Paused { remaining_ms } = item.timer {
                            view.state = "paused".into();
                            view.remaining_ms = Some(remaining_ms);
                        }
                    }
                }
            }
            view
        })
        .collect();
    items.extend(
        engine
            .state()
            .countdowns
            .values()
            .filter(|c| !c.deleted)
            .filter_map(|c| {
                Some(WidgetItem {
                    // Separate namespaces: a countdown must never receive a reminder action.
                    id: format!("countdown:{}", c.id),
                    title: c.title.clone(),
                    timer: false,
                    state: "countdown".into(),
                    at: None,
                    remaining_ms: None,
                    occurrence: None,
                    repeating_reminder: false,
                    countdown: Some(WidgetCountdown {
                        date: c.date?,
                        time: c.time,
                        repeat: c.repeat,
                        direction: c.direction,
                        color: c.display.color.clone(),
                        fill: c.display.fill,
                    }),
                })
            }),
    );
    // Stable ties in Next due and stable ordering in the configuration list.
    items.sort_by(|a, b| a.title.cmp(&b.title).then(a.id.cmp(&b.id)));
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use dun_core::actions::ItemDraft;
    use dun_core::time::MINUTE;

    #[test]
    fn countdowns_export_banner_and_calendar_inputs_without_alert_actions() {
        use dun_core::actions::CountdownDraft;
        use dun_core::model::{CountDirection, CountdownDisplay, CountdownFill, CountdownRepeat};
        use jiff::civil::date;
        let mut e = Engine::open_in_memory_as("phone").unwrap();
        let now = Timestamp(1_800_000_000_000);
        let id = e
            .save_countdown(
                now,
                None,
                CountdownDraft {
                    title: "Birthday".into(),
                    notes: String::new(),
                    date: date(2024, 2, 29),
                    time: None,
                    repeat: CountdownRepeat::Yearly,
                    direction: CountDirection::Until,
                    display: CountdownDisplay {
                        color: Some("#287d87".into()),
                        fill: CountdownFill::Soft,
                        ..Default::default()
                    },
                    pinned: false,
                },
            )
            .unwrap();
        let view = items(&e, now, &TimeZone::UTC).remove(0);
        assert_eq!(view.id, format!("countdown:{id}"));
        assert!(!view.repeating_reminder);
        assert_eq!(view.occurrence, None);
        let spec = view.countdown.unwrap();
        assert_eq!(spec.repeat, CountdownRepeat::Yearly);
        assert_eq!(spec.color.as_deref(), Some("#287d87"));
        assert_eq!(spec.fill, CountdownFill::Soft);
        e.delete_countdown(now, &id).unwrap();
        assert!(items(&e, now, &TimeZone::UTC).is_empty());
    }

    #[test]
    fn widget_follows_snooze_pause_completion_and_archive() {
        let mut e = Engine::open_in_memory_as("phone").unwrap();
        let now = Timestamp(1_800_000_000_000);
        let tz = TimeZone::UTC;
        let draft = |schedule, start_timer| ItemDraft {
            title: "Tea".into(),
            notes: String::new(),
            tag: None,
            schedule,
            nag: None,
            chime: None,
            quiet_exempt: None,
            start_timer,
        };
        let id = e
            .create_item(now, draft(Schedule::OneOff { due: now }, false))
            .unwrap();
        e.snooze(now, &tz, &id, Some(now), 5).unwrap();
        let v = items(&e, now, &tz).remove(0);
        assert_eq!(v.state, "snoozed");
        assert_eq!(v.at, Some(now.plus(5 * MINUTE).0));
        assert_eq!(v.occurrence, Some(now.0));
        assert_eq!(items(&e, now.plus(5 * MINUTE), &tz)[0].state, "due");
        e.done(now, &tz, &id, Some(now)).unwrap();
        assert_eq!(items(&e, now, &tz)[0].at, None);
        e.set_deleted(now, &id, true).unwrap();
        assert!(items(&e, now, &tz).is_empty());
        let timer = e
            .create_item(
                now,
                draft(
                    Schedule::Timer {
                        duration_ms: 10 * MINUTE,
                    },
                    true,
                ),
            )
            .unwrap();
        assert_eq!(items(&e, now, &tz)[0].at, Some(now.plus(10 * MINUTE).0));
        e.pause_timer(now.plus(MINUTE), &timer).unwrap();
        let v = items(&e, now.plus(MINUTE), &tz).remove(0);
        assert_eq!(v.state, "paused");
        assert_eq!(v.remaining_ms, Some(9 * MINUTE));
        assert_eq!(v.at, None);
    }
}
