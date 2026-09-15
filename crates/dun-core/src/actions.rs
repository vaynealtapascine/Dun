//! What user actions write.
//!
//! Each action is a pure function from the current [`State`] to a [`Change`]
//! (register writes plus history entries). Applying the change to the store
//! and re-projecting is the caller's job, so every rule here is testable
//! without a database.

use serde::{Deserialize, Serialize};

use crate::model::{
    ChimeRef, Completion, Created, Item, ItemKind, Nag, Schedule, Snooze, TimerState,
};
use crate::occurrence::{status, Status};
use crate::recurrence::RecurrenceError;
use crate::state::State;
use crate::store::{NewHistory, NewWrite};
use crate::sync::{field, Entity, HistoryKind, HistoryRow};
use crate::time::{TimeZone, Timestamp};

/// Longest snooze a button or the app can set.
pub const MAX_SNOOZE_MIN: u32 = 24 * 60;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Change {
    pub writes: Vec<NewWrite>,
    pub history: Vec<NewHistory>,
}

impl Change {
    pub fn is_empty(&self) -> bool {
        self.writes.is_empty() && self.history.is_empty()
    }

    fn item(&mut self, id: &str, field: &str, value: impl Serialize) {
        self.writes
            .push(NewWrite::new(Entity::Item, id, field, value));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ActionError {
    #[error("that item no longer exists")]
    NotFound,
    #[error("that item was deleted")]
    Deleted,
    #[error("the title can't be empty")]
    EmptyTitle,
    #[error("a timer needs a duration of at least one second")]
    BadDuration,
    #[error("snooze must be between 1 and {MAX_SNOOZE_MIN} minutes")]
    BadSnooze,
    #[error("nag interval must be between 1 and 1440 minutes")]
    BadNag,
    #[error(transparent)]
    Recurrence(#[from] RecurrenceError),
    #[error("nothing is due to mark done")]
    NothingDue,
    #[error("only a completion can be undone")]
    NotUndoable,
    #[error("that completion was already undone")]
    AlreadyUndone,
}

pub type Result<T> = std::result::Result<T, ActionError>;

/// The editable parts of an item, as sent by the form or quick-add.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDraft {
    pub title: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub tag: Option<String>,
    pub schedule: Schedule,
    /// `None` uses the synced default.
    #[serde(default)]
    pub nag: Option<Nag>,
    #[serde(default)]
    pub chime: Option<ChimeRef>,
    /// `None` uses the default for the kind (timers ring through quiet hours).
    #[serde(default)]
    pub quiet_exempt: Option<bool>,
    /// Timers only: start counting down immediately.
    #[serde(default)]
    pub start_timer: bool,
}

fn validate(draft: &ItemDraft) -> Result<()> {
    if draft.title.trim().is_empty() {
        return Err(ActionError::EmptyTitle);
    }
    match &draft.schedule {
        Schedule::Timer { duration_ms } if *duration_ms < 1_000 => {
            return Err(ActionError::BadDuration)
        }
        Schedule::Recurring { recurrence, .. } => recurrence.validate()?,
        _ => {}
    }
    if let Some(Nag::Repeat { interval_min }) = draft.nag {
        if !(1..=1440).contains(&interval_min) {
            return Err(ActionError::BadNag);
        }
    }
    Ok(())
}

fn kind_of(schedule: &Schedule) -> ItemKind {
    match schedule {
        Schedule::OneOff { .. } => ItemKind::Reminder,
        Schedule::Recurring { .. } => ItemKind::Recurring,
        Schedule::Timer { .. } => ItemKind::Timer,
    }
}

fn live<'a>(state: &'a State, id: &str) -> Result<&'a Item> {
    let item = state.items.get(id).ok_or(ActionError::NotFound)?;
    if item.deleted {
        return Err(ActionError::Deleted);
    }
    Ok(item)
}

pub fn create_item(
    state: &State,
    device: &str,
    now: Timestamp,
    id: &str,
    mut draft: ItemDraft,
) -> Result<Change> {
    validate(&draft)?;
    if let Schedule::Recurring { effective_from, .. } = &mut draft.schedule {
        *effective_from = now;
    }
    let mut c = Change::default();
    c.item(
        id,
        field::CREATED,
        Created {
            at: now,
            kind: kind_of(&draft.schedule),
            by: device.to_string(),
        },
    );
    c.item(id, field::TITLE, draft.title.trim());
    c.item(id, field::NOTES, &draft.notes);
    c.item(id, field::TAG, &draft.tag);
    c.item(
        id,
        field::NAG,
        draft.nag.unwrap_or(state.settings.nag_default),
    );
    c.item(id, field::CHIME, &draft.chime);
    c.item(id, field::QUIET_EXEMPT, draft.quiet_exempt);
    if let Schedule::Timer { duration_ms } = draft.schedule {
        let timer = if draft.start_timer {
            TimerState::Running {
                end_at: now.plus(duration_ms),
            }
        } else {
            TimerState::Idle
        };
        c.item(id, field::TIMER, timer);
    }
    c.item(id, field::SCHEDULE, &draft.schedule);
    Ok(c)
}

/// Applies an edited draft. Only fields that differ are written, so a
/// concurrent edit of another field on another device survives.
pub fn update_item(
    state: &State,
    now: Timestamp,
    id: &str,
    mut draft: ItemDraft,
) -> Result<Change> {
    validate(&draft)?;
    let item = live(state, id)?;
    let mut c = Change::default();

    if item.title != draft.title.trim() {
        c.item(id, field::TITLE, draft.title.trim());
    }
    if item.notes != draft.notes {
        c.item(id, field::NOTES, &draft.notes);
    }
    if item.tag != draft.tag {
        c.item(id, field::TAG, &draft.tag);
    }
    let nag = draft.nag.unwrap_or(item.nag);
    if item.nag != nag {
        c.item(id, field::NAG, nag);
    }
    if item.chime != draft.chime {
        c.item(id, field::CHIME, &draft.chime);
    }
    if item.quiet_exempt_override != draft.quiet_exempt {
        c.item(id, field::QUIET_EXEMPT, draft.quiet_exempt);
    }

    // Keep the old effective_from unless the rule itself changed.
    if let (
        Some(Schedule::Recurring {
            recurrence: old_rec,
            mode: old_mode,
            effective_from: old_from,
        }),
        Schedule::Recurring {
            recurrence,
            mode,
            effective_from,
        },
    ) = (&item.schedule, &mut draft.schedule)
    {
        *effective_from = if old_rec == recurrence && old_mode == mode {
            *old_from
        } else {
            now
        };
    } else if let Schedule::Recurring { effective_from, .. } = &mut draft.schedule {
        *effective_from = now;
    }

    if item.schedule.as_ref() != Some(&draft.schedule) {
        // Moving a finished one-off to a later time reopens it.
        if let (Schedule::OneOff { due }, Some(through)) =
            (&draft.schedule, item.completion.through)
        {
            if *due > through {
                c.item(
                    id,
                    field::COMPLETION,
                    Completion {
                        through: None,
                        done_at: None,
                        gen: item.completion.gen + 1,
                    },
                );
            }
        }
        c.item(id, field::SCHEDULE, &draft.schedule);
    }
    Ok(c)
}

pub fn set_deleted(state: &State, id: &str, deleted: bool) -> Result<Change> {
    let item = state.items.get(id).ok_or(ActionError::NotFound)?;
    let mut c = Change::default();
    if item.deleted != deleted {
        c.item(id, field::DELETED, deleted);
    }
    Ok(c)
}

/// Marks an occurrence done.
///
/// `occ` is the occurrence the user acted on (a toast or notification button
/// carries it). If it's stale because a newer occurrence has come due since,
/// only that older occurrence is completed and the newer one keeps ringing.
/// With `occ = None` the current occurrence is completed, or the next one if
/// nothing is due yet (finishing early).
pub fn done(
    state: &State,
    now: Timestamp,
    tz: &TimeZone,
    id: &str,
    occ: Option<Timestamp>,
) -> Result<Change> {
    let item = live(state, id)?;
    let inputs = item.inputs().ok_or(ActionError::NothingDue)?;
    let current = status(inputs, now, tz);

    let target = match (occ, current) {
        (Some(o), _) => o,
        (None, Status::Due { occ, .. }) => occ.due,
        (None, Status::Upcoming { at }) => at,
        (None, Status::Idle) => return Err(ActionError::NothingDue),
    };
    if item.completion.through.is_some_and(|t| t >= target) {
        return Ok(Change::default());
    }

    let snooze_count = item
        .snooze
        .filter(|s| s.occurrence == target)
        .map_or(0, |s| s.count);

    let mut c = Change::default();
    c.item(
        id,
        field::COMPLETION,
        Completion {
            through: Some(target),
            done_at: Some(now),
            gen: item.completion.gen,
        },
    );
    if item.snooze.is_some_and(|s| s.occurrence <= target) {
        c.item(id, field::SNOOZE, Option::<Snooze>::None);
    }
    if let TimerState::Running { end_at } = item.timer {
        if end_at <= target {
            c.item(id, field::TIMER, TimerState::Idle);
        }
    }
    c.history.push(NewHistory {
        item_id: id.to_string(),
        kind: HistoryKind::Done,
        occurrence: Some(target),
        snooze_count,
        title: item.title.clone(),
        ref_id: None,
        prev_completion: Some(item.completion),
    });
    Ok(c)
}

/// Snoozes the ringing occurrence `occ` (or the current one) for `minutes`.
/// A button from an occurrence that is no longer current does nothing.
pub fn snooze(
    state: &State,
    now: Timestamp,
    tz: &TimeZone,
    id: &str,
    occ: Option<Timestamp>,
    minutes: u32,
) -> Result<Change> {
    if !(1..=MAX_SNOOZE_MIN).contains(&minutes) {
        return Err(ActionError::BadSnooze);
    }
    let item = live(state, id)?;
    let Some(inputs) = item.inputs() else {
        return Ok(Change::default());
    };
    let Status::Due { occ: current, .. } = status(inputs, now, tz) else {
        return Ok(Change::default());
    };
    if occ.is_some_and(|o| o != current.due) {
        return Ok(Change::default());
    }
    let count = item
        .snooze
        .filter(|s| s.occurrence == current.due)
        .map_or(0, |s| s.count)
        + 1;
    let mut c = Change::default();
    c.item(
        id,
        field::SNOOZE,
        Some(Snooze {
            occurrence: current.due,
            until: now.plus_minutes(i64::from(minutes)),
            count,
        }),
    );
    Ok(c)
}

/// Reverses a completion. The restored completion gets a higher `gen`, so it
/// also beats a copy of the Done that another device hasn't synced yet.
pub fn undo(state: &State, entry: &HistoryRow, already_undone: bool) -> Result<Change> {
    if entry.kind != HistoryKind::Done {
        return Err(ActionError::NotUndoable);
    }
    if already_undone {
        return Err(ActionError::AlreadyUndone);
    }
    let item = live(state, &entry.item_id)?;
    let prev = entry.prev_completion.unwrap_or_default();

    let mut c = Change::default();
    c.item(
        &entry.item_id,
        field::COMPLETION,
        Completion {
            through: prev.through,
            done_at: prev.done_at,
            gen: item.completion.gen + 1,
        },
    );
    // A finished timer goes back to running (already overdue) so it rings again.
    if let (Some(Schedule::Timer { .. }), TimerState::Idle, Some(occ)) =
        (&item.schedule, item.timer, entry.occurrence)
    {
        c.item(
            &entry.item_id,
            field::TIMER,
            TimerState::Running { end_at: occ },
        );
    }
    c.history.push(NewHistory {
        item_id: entry.item_id.clone(),
        kind: HistoryKind::Undo,
        occurrence: entry.occurrence,
        snooze_count: 0,
        title: item.title.clone(),
        ref_id: Some(entry.id.clone()),
        prev_completion: None,
    });
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hlc::Hlc;
    use crate::recurrence::{Recurrence, Rule};
    use crate::time::{resolve, MINUTE};
    use jiff::civil::{date, time, DateTime};

    const UTC: TimeZone = TimeZone::UTC;

    fn at(dt: DateTime) -> Timestamp {
        resolve(dt, &UTC)
    }

    /// Applies a change to a state the way the store would (last write wins
    /// locally), returning the history it would append.
    fn apply(state: &mut State, change: &Change, now: Timestamp) -> Vec<HistoryRow> {
        for (i, w) in change.writes.iter().enumerate() {
            state.apply(&crate::sync::Reg {
                entity: w.entity,
                id: w.id.clone(),
                field: w.field.clone(),
                value: w.value.clone(),
                hlc: Hlc::new(now.0, i as u16),
                device: "pc".into(),
            });
        }
        change
            .history
            .iter()
            .map(|h| HistoryRow {
                id: crate::ids::new_id(),
                item_id: h.item_id.clone(),
                kind: h.kind,
                occurrence: h.occurrence,
                at: now,
                snooze_count: h.snooze_count,
                title: h.title.clone(),
                ref_id: h.ref_id.clone(),
                prev_completion: h.prev_completion,
                hlc: Hlc::new(now.0, 0),
                device: "pc".into(),
            })
            .collect()
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

    fn created(draft: ItemDraft, now: Timestamp) -> State {
        let mut s = State::default();
        let c = create_item(&s, "pc", now, "a", draft).unwrap();
        apply(&mut s, &c, now);
        s
    }

    #[test]
    fn create_validates_and_uses_synced_nag_default() {
        let now = at(date(2026, 9, 16).at(8, 0, 0, 0));
        let mut s = State::default();
        s.settings.nag_default = Nag::Repeat { interval_min: 5 };
        assert_eq!(
            create_item(&s, "pc", now, "a", reminder("  ", now)),
            Err(ActionError::EmptyTitle)
        );
        let c = create_item(&s, "pc", now, "a", reminder(" Pay rent ", now)).unwrap();
        apply(&mut s, &c, now);
        let item = &s.items["a"];
        assert_eq!(item.title, "Pay rent");
        assert_eq!(item.nag, Nag::Repeat { interval_min: 5 });
        assert_eq!(item.created.as_ref().unwrap().kind, ItemKind::Reminder);
    }

    #[test]
    fn started_timer_runs_from_now() {
        let now = at(date(2026, 9, 16).at(8, 0, 0, 0));
        let draft = ItemDraft {
            schedule: Schedule::Timer {
                duration_ms: 45 * MINUTE,
            },
            start_timer: true,
            ..reminder("Laundry", now)
        };
        let s = created(draft, now);
        assert_eq!(
            s.items["a"].timer,
            TimerState::Running {
                end_at: now.plus(45 * MINUTE)
            }
        );
    }

    #[test]
    fn done_records_history_and_undo_restores_with_higher_gen() {
        let due = at(date(2026, 9, 16).at(9, 0, 0, 0));
        let mut s = created(reminder("Pay rent", due), due.minus(MINUTE));
        let now = due.plus(3 * MINUTE);

        let snz = snooze(&s, now, &UTC, "a", Some(due), 5).unwrap();
        apply(&mut s, &snz, now);
        let snz = snooze(&s, now.plus(MINUTE), &UTC, "a", Some(due), 5).unwrap();
        apply(&mut s, &snz, now.plus(MINUTE));
        assert_eq!(s.items["a"].snooze.unwrap().count, 2);

        let d = done(&s, now.plus(2 * MINUTE), &UTC, "a", Some(due)).unwrap();
        let rows = apply(&mut s, &d, now.plus(2 * MINUTE));
        assert_eq!(rows[0].snooze_count, 2);
        assert_eq!(rows[0].prev_completion, Some(Completion::default()));
        assert_eq!(s.items["a"].completion.through, Some(due));
        assert_eq!(s.items["a"].snooze, None);
        assert_eq!(
            status(s.items["a"].inputs().unwrap(), now, &UTC),
            Status::Idle
        );

        // Done again is a no-op.
        assert!(done(&s, now.plus(3 * MINUTE), &UTC, "a", Some(due))
            .unwrap()
            .is_empty());

        let u = undo(&s, &rows[0], false).unwrap();
        apply(&mut s, &u, now.plus(4 * MINUTE));
        assert_eq!(
            s.items["a"].completion,
            Completion {
                through: None,
                done_at: None,
                gen: 1
            }
        );
        assert!(
            status(s.items["a"].inputs().unwrap(), now.plus(4 * MINUTE), &UTC)
                .is_ringing(now.plus(4 * MINUTE))
        );
        assert_eq!(u.history[0].ref_id.as_deref(), Some(rows[0].id.as_str()));
        assert_eq!(undo(&s, &rows[0], true), Err(ActionError::AlreadyUndone));
    }

    #[test]
    fn stale_buttons_act_only_on_their_own_occurrence() {
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
        let mut s = created(draft, at(date(2026, 9, 14).at(12, 0, 0, 0)));
        let yesterday = at(date(2026, 9, 15).at(9, 0, 0, 0));
        let today = at(date(2026, 9, 16).at(9, 0, 0, 0));
        let now = at(date(2026, 9, 16).at(9, 5, 0, 0));

        // A snooze button from yesterday's toast does nothing.
        assert!(snooze(&s, now, &UTC, "a", Some(yesterday), 15)
            .unwrap()
            .is_empty());

        // Its Done completes yesterday only; today's keeps ringing.
        let d = done(&s, now, &UTC, "a", Some(yesterday)).unwrap();
        apply(&mut s, &d, now);
        match status(s.items["a"].inputs().unwrap(), now, &UTC) {
            Status::Due { occ, .. } => assert_eq!((occ.due, occ.count), (today, 1)),
            other => panic!("expected today's occurrence to ring, got {other:?}"),
        }
    }

    #[test]
    fn finishing_early_completes_the_next_occurrence() {
        let due = at(date(2026, 9, 16).at(9, 0, 0, 0));
        let mut s = created(reminder("Early", due), due.minus(60 * MINUTE));
        let d = done(&s, due.minus(30 * MINUTE), &UTC, "a", None).unwrap();
        apply(&mut s, &d, due.minus(30 * MINUTE));
        assert_eq!(
            status(s.items["a"].inputs().unwrap(), due, &UTC),
            Status::Idle
        );
    }

    #[test]
    fn done_timer_goes_idle_and_undo_brings_it_back_ringing() {
        let now = at(date(2026, 9, 16).at(8, 0, 0, 0));
        let draft = ItemDraft {
            schedule: Schedule::Timer {
                duration_ms: 10 * MINUTE,
            },
            start_timer: true,
            ..reminder("Tea", now)
        };
        let mut s = created(draft, now);
        let rang = now.plus(11 * MINUTE);
        let d = done(&s, rang, &UTC, "a", None).unwrap();
        let rows = apply(&mut s, &d, rang);
        assert_eq!(s.items["a"].timer, TimerState::Idle);

        let u = undo(&s, &rows[0], false).unwrap();
        apply(&mut s, &u, rang.plus(MINUTE));
        assert!(
            status(s.items["a"].inputs().unwrap(), rang.plus(MINUTE), &UTC)
                .is_ringing(rang.plus(MINUTE))
        );
    }

    #[test]
    fn edits_write_only_changed_fields_and_moving_a_done_one_off_reopens_it() {
        let due = at(date(2026, 9, 16).at(9, 0, 0, 0));
        let mut s = created(reminder("Call mom", due), due.minus(MINUTE));
        let d = done(&s, due.plus(MINUTE), &UTC, "a", None).unwrap();
        apply(&mut s, &d, due.plus(MINUTE));

        let same = update_item(&s, due.plus(2 * MINUTE), "a", reminder("Call mom", due)).unwrap();
        assert!(same.is_empty());

        let later = due.plus(24 * 60 * MINUTE);
        let moved =
            update_item(&s, due.plus(2 * MINUTE), "a", reminder("Call mom", later)).unwrap();
        let fields: Vec<_> = moved.writes.iter().map(|w| w.field.as_str()).collect();
        assert_eq!(fields, [field::COMPLETION, field::SCHEDULE]);
        apply(&mut s, &moved, due.plus(2 * MINUTE));
        assert!(
            status(s.items["a"].inputs().unwrap(), later, &UTC).is_ringing(later),
            "moving a finished reminder later makes it ring again"
        );
    }

    #[test]
    fn deleted_items_reject_actions() {
        let due = at(date(2026, 9, 16).at(9, 0, 0, 0));
        let mut s = created(reminder("x", due), due);
        let del = set_deleted(&s, "a", true).unwrap();
        apply(&mut s, &del, due);
        assert_eq!(done(&s, due, &UTC, "a", None), Err(ActionError::Deleted));
        assert!(set_deleted(&s, "a", true).unwrap().is_empty());
        assert_eq!(
            snooze(&s, due, &UTC, "a", None, 0),
            Err(ActionError::BadSnooze)
        );
    }
}
