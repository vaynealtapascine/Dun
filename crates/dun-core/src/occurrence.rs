//! Works out which occurrence of an item is current and when it rings.
//!
//! An occurrence is identified by its due instant. When several recurring
//! slots passed unnoticed (PC off, phone asleep) they collapse into one: the
//! latest slot is the occurrence, and `first`/`count` say what was missed.

use crate::model::{Completion, RepeatMode, Schedule, Snooze, TimerState};
use crate::recurrence::Rule;
use crate::time::{TimeZone, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Occurrence {
    /// Due instant; also the occurrence's identity.
    pub due: Timestamp,
    /// Earliest slot collapsed into this occurrence (== `due` unless missed).
    pub first: Timestamp,
    /// Number of slots collapsed (>= 1).
    pub count: u32,
}

impl Occurrence {
    fn single(due: Timestamp) -> Self {
        Occurrence {
            due,
            first: due,
            count: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// An occurrence is due. It rings from `rings_at` (its due instant, or the
    /// end of a snooze that applies to it).
    Due {
        occ: Occurrence,
        rings_at: Timestamp,
        snoozed: bool,
    },
    /// Nothing due yet; the next occurrence is at `at`.
    Upcoming { at: Timestamp },
    /// Nothing will ring: a finished one-off, or a paused/idle/finished timer.
    Idle,
}

impl Status {
    pub fn is_ringing(&self, now: Timestamp) -> bool {
        matches!(self, Status::Due { rings_at, .. } if *rings_at <= now)
    }

    /// When evaluation must run again for this item, if ever.
    pub fn next_wake(&self, now: Timestamp) -> Option<Timestamp> {
        match self {
            Status::Due { rings_at, .. } if *rings_at > now => Some(*rings_at),
            Status::Upcoming { at } => Some(*at),
            _ => None,
        }
    }
}

/// Everything about an item that affects when it rings.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    pub schedule: &'a Schedule,
    pub timer: TimerState,
    pub completion: Completion,
    pub snooze: Option<Snooze>,
}

pub fn status(inputs: Inputs<'_>, now: Timestamp, device_tz: &TimeZone) -> Status {
    let through = inputs.completion.through;
    let is_done = |due: Timestamp| through.is_some_and(|t| t >= due);

    let due_or_upcoming = |occ: Occurrence| {
        if occ.due > now {
            return Status::Upcoming { at: occ.due };
        }
        match inputs.snooze {
            Some(s) if s.occurrence == occ.due => Status::Due {
                occ,
                rings_at: s.until.max(occ.due),
                snoozed: s.until > now,
            },
            _ => Status::Due {
                occ,
                rings_at: occ.due,
                snoozed: false,
            },
        }
    };

    match inputs.schedule {
        Schedule::OneOff { due } => {
            if is_done(*due) {
                Status::Idle
            } else {
                due_or_upcoming(Occurrence::single(*due))
            }
        }

        Schedule::Timer { .. } => match inputs.timer {
            TimerState::Running { end_at } if !is_done(end_at) => {
                due_or_upcoming(Occurrence::single(end_at))
            }
            _ => Status::Idle,
        },

        Schedule::Recurring {
            recurrence,
            mode,
            effective_from,
        } => {
            // Slots strictly after `floor` count. Completion always raises the
            // floor so a finished occurrence can never ring again, even if a
            // peer's clock put `through` slightly in the future.
            let edit_floor = effective_from.minus(1);
            let mut floor = through.map_or(edit_floor, |t| t.max(edit_floor));

            if *mode == RepeatMode::AfterCompletion {
                let done_at = inputs.completion.done_at.filter(|d| *d >= *effective_from);
                if let (Rule::Interval { .. }, Some(done_at)) = (&recurrence.rule, done_at) {
                    let step = interval_ms(&recurrence.rule);
                    return due_or_upcoming(Occurrence::single(done_at.plus(step)));
                }
                if let Some(done_at) = done_at {
                    floor = floor.max(done_at);
                }
            }

            match recurrence.window(floor, now, device_tz) {
                Some(w) => due_or_upcoming(Occurrence {
                    due: w.last,
                    first: w.first,
                    count: w.count,
                }),
                None => match recurrence.next_after(floor.max(now), device_tz) {
                    Some(at) => Status::Upcoming { at },
                    None => Status::Idle,
                },
            }
        }
    }
}

fn interval_ms(rule: &Rule) -> i64 {
    match rule {
        Rule::Interval { every, unit } => {
            let unit_ms = match unit {
                crate::recurrence::IntervalUnit::Minutes => crate::time::MINUTE,
                crate::recurrence::IntervalUnit::Hours => crate::time::HOUR,
            };
            i64::from((*every).max(1)) * unit_ms
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recurrence::{IntervalUnit, Recurrence};
    use crate::time::{resolve, MINUTE};
    use jiff::civil::{date, time, DateTime};

    const UTC: TimeZone = TimeZone::UTC;

    fn at(dt: DateTime) -> Timestamp {
        resolve(dt, &UTC)
    }

    fn one_off(due: Timestamp) -> Schedule {
        Schedule::OneOff { due }
    }

    fn inputs(schedule: &Schedule) -> Inputs<'_> {
        Inputs {
            schedule,
            timer: TimerState::Idle,
            completion: Completion::default(),
            snooze: None,
        }
    }

    fn daily_nine(mode: RepeatMode, effective_from: Timestamp) -> Schedule {
        Schedule::Recurring {
            recurrence: Recurrence {
                rule: Rule::Daily {
                    every: 1,
                    times: vec![time(9, 0, 0, 0)],
                },
                start: date(2026, 9, 1).at(0, 0, 0, 0),
                tz: None,
            },
            mode,
            effective_from,
        }
    }

    #[test]
    fn one_off_upcoming_due_done() {
        let due = at(date(2026, 9, 16).at(9, 0, 0, 0));
        let s = one_off(due);
        assert_eq!(
            status(inputs(&s), due.minus(MINUTE), &UTC),
            Status::Upcoming { at: due }
        );

        let ringing = status(inputs(&s), due.plus(MINUTE), &UTC);
        assert!(ringing.is_ringing(due.plus(MINUTE)));

        let mut done = inputs(&s);
        done.completion.through = Some(due);
        assert_eq!(status(done, due.plus(MINUTE), &UTC), Status::Idle);
    }

    #[test]
    fn snooze_applies_only_to_its_occurrence() {
        let due = at(date(2026, 9, 16).at(9, 0, 0, 0));
        let s = one_off(due);
        let now = due.plus(2 * MINUTE);

        let mut snoozed = inputs(&s);
        snoozed.snooze = Some(Snooze {
            occurrence: due,
            until: now.plus(5 * MINUTE),
            count: 1,
        });
        let st = status(snoozed, now, &UTC);
        assert!(!st.is_ringing(now));
        assert_eq!(st.next_wake(now), Some(now.plus(5 * MINUTE)));
        // Once the snooze runs out it rings again.
        assert!(status(snoozed, now.plus(5 * MINUTE), &UTC).is_ringing(now.plus(5 * MINUTE)));

        let mut stale = inputs(&s);
        stale.snooze = Some(Snooze {
            occurrence: due.minus(60 * MINUTE),
            until: now.plus(60 * MINUTE),
            count: 3,
        });
        assert!(
            status(stale, now, &UTC).is_ringing(now),
            "a snooze for another occurrence is ignored"
        );
    }

    #[test]
    fn done_beats_a_later_snooze_for_the_same_occurrence() {
        let due = at(date(2026, 9, 16).at(9, 0, 0, 0));
        let s = one_off(due);
        let mut i = inputs(&s);
        i.completion.through = Some(due);
        i.snooze = Some(Snooze {
            occurrence: due,
            until: due.plus(15 * MINUTE),
            count: 1,
        });
        assert_eq!(status(i, due.plus(20 * MINUTE), &UTC), Status::Idle);
    }

    #[test]
    fn recurring_collapses_missed_days_into_one_ring() {
        let s = daily_nine(
            RepeatMode::FromSchedule,
            at(date(2026, 9, 1).at(0, 0, 0, 0)),
        );
        let mut i = inputs(&s);
        i.completion.through = Some(at(date(2026, 9, 12).at(9, 0, 0, 0)));

        // PC was off from the 12th until the 15th at 10:00.
        let now = at(date(2026, 9, 15).at(10, 0, 0, 0));
        match status(i, now, &UTC) {
            Status::Due { occ, rings_at, .. } => {
                assert_eq!(occ.due, at(date(2026, 9, 15).at(9, 0, 0, 0)));
                assert_eq!(occ.first, at(date(2026, 9, 13).at(9, 0, 0, 0)));
                assert_eq!(occ.count, 3);
                assert_eq!(rings_at, occ.due);
            }
            other => panic!("expected due, got {other:?}"),
        }

        // Marking the latest one done covers all three.
        i.completion.through = Some(at(date(2026, 9, 15).at(9, 0, 0, 0)));
        assert_eq!(
            status(i, now, &UTC),
            Status::Upcoming {
                at: at(date(2026, 9, 16).at(9, 0, 0, 0))
            }
        );
    }

    #[test]
    fn rule_edit_does_not_create_phantom_missed_rings() {
        let edited_at = at(date(2026, 9, 15).at(12, 0, 0, 0));
        let s = daily_nine(RepeatMode::FromSchedule, edited_at);
        let st = status(inputs(&s), edited_at.plus(MINUTE), &UTC);
        assert_eq!(
            st,
            Status::Upcoming {
                at: at(date(2026, 9, 16).at(9, 0, 0, 0))
            }
        );
    }

    #[test]
    fn new_slot_while_ringing_replaces_the_occurrence_and_drops_its_snooze() {
        let s = daily_nine(
            RepeatMode::FromSchedule,
            at(date(2026, 9, 1).at(0, 0, 0, 0)),
        );
        let yesterday = at(date(2026, 9, 15).at(9, 0, 0, 0));
        let mut i = inputs(&s);
        i.completion.through = Some(at(date(2026, 9, 14).at(9, 0, 0, 0)));
        i.snooze = Some(Snooze {
            occurrence: yesterday,
            until: at(date(2026, 9, 17).at(0, 0, 0, 0)),
            count: 1,
        });
        let now = at(date(2026, 9, 16).at(9, 1, 0, 0));
        match status(i, now, &UTC) {
            Status::Due { occ, snoozed, .. } => {
                assert_eq!(occ.due, at(date(2026, 9, 16).at(9, 0, 0, 0)));
                assert_eq!(occ.count, 2);
                assert!(!snoozed);
            }
            other => panic!("expected due, got {other:?}"),
        }
    }

    #[test]
    fn after_completion_interval_counts_from_done() {
        let s = Schedule::Recurring {
            recurrence: Recurrence {
                rule: Rule::Interval {
                    every: 3,
                    unit: IntervalUnit::Hours,
                },
                start: date(2026, 9, 16).at(8, 0, 0, 0),
                tz: None,
            },
            mode: RepeatMode::AfterCompletion,
            effective_from: at(date(2026, 9, 16).at(7, 0, 0, 0)),
        };
        // Never done: first ring is the start.
        let st = status(inputs(&s), at(date(2026, 9, 16).at(7, 30, 0, 0)), &UTC);
        assert_eq!(
            st,
            Status::Upcoming {
                at: at(date(2026, 9, 16).at(8, 0, 0, 0))
            }
        );

        // Took it late, at 10:17: next is 13:17, not 11:00.
        let mut i = inputs(&s);
        i.completion = Completion {
            through: Some(at(date(2026, 9, 16).at(8, 0, 0, 0))),
            done_at: Some(at(date(2026, 9, 16).at(10, 17, 0, 0))),
            gen: 0,
        };
        assert_eq!(
            status(i, at(date(2026, 9, 16).at(10, 18, 0, 0)), &UTC),
            Status::Upcoming {
                at: at(date(2026, 9, 16).at(13, 17, 0, 0))
            }
        );
        // Overdue later: a single occurrence, no collapsed count.
        match status(i, at(date(2026, 9, 16).at(20, 0, 0, 0)), &UTC) {
            Status::Due { occ, .. } => assert_eq!(occ.count, 1),
            other => panic!("expected due, got {other:?}"),
        }
    }

    #[test]
    fn after_completion_calendar_skips_slots_before_done() {
        let s = daily_nine(
            RepeatMode::AfterCompletion,
            at(date(2026, 9, 1).at(0, 0, 0, 0)),
        );
        let mut i = inputs(&s);
        // The 13th's occurrence was only finished on the 15th at 11:00.
        i.completion = Completion {
            through: Some(at(date(2026, 9, 13).at(9, 0, 0, 0))),
            done_at: Some(at(date(2026, 9, 15).at(11, 0, 0, 0))),
            gen: 0,
        };
        assert_eq!(
            status(i, at(date(2026, 9, 15).at(12, 0, 0, 0)), &UTC),
            Status::Upcoming {
                at: at(date(2026, 9, 16).at(9, 0, 0, 0))
            }
        );
    }

    #[test]
    fn completion_from_a_fast_peer_clock_never_re_rings() {
        let s = daily_nine(
            RepeatMode::FromSchedule,
            at(date(2026, 9, 1).at(0, 0, 0, 0)),
        );
        let mut i = inputs(&s);
        i.completion.through = Some(at(date(2026, 9, 16).at(9, 0, 0, 0)));
        // Local clock says it's still 08:59:30.
        let now = at(date(2026, 9, 16).at(8, 59, 30, 0));
        assert_eq!(
            status(i, now, &UTC),
            Status::Upcoming {
                at: at(date(2026, 9, 17).at(9, 0, 0, 0))
            }
        );
    }

    #[test]
    fn timers() {
        let s = Schedule::Timer {
            duration_ms: 45 * MINUTE,
        };
        let end = at(date(2026, 9, 16).at(9, 45, 0, 0));
        let now = at(date(2026, 9, 16).at(9, 30, 0, 0));

        let mut i = inputs(&s);
        assert_eq!(status(i, now, &UTC), Status::Idle);

        i.timer = TimerState::Paused {
            remaining_ms: 15 * MINUTE,
        };
        assert_eq!(status(i, now, &UTC), Status::Idle);

        i.timer = TimerState::Running { end_at: end };
        assert_eq!(status(i, now, &UTC), Status::Upcoming { at: end });
        assert!(status(i, end, &UTC).is_ringing(end));

        i.completion.through = Some(end);
        assert_eq!(status(i, end.plus(MINUTE), &UTC), Status::Idle);
    }
}
