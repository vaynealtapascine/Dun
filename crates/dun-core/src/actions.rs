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
    #[error("that item isn't a timer")]
    NotATimer,
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

/// Validates a draft exactly as saving would and returns when it would next
/// ring (up to `count` times), so previews can't disagree with the engine.
/// Timers preview as if started now.
pub fn preview(
    draft: &ItemDraft,
    now: Timestamp,
    tz: &TimeZone,
    count: usize,
) -> Result<Vec<Timestamp>> {
    validate(draft)?;
    Ok(match &draft.schedule {
        Schedule::OneOff { due } => vec![*due],
        Schedule::Timer { duration_ms } => vec![now.plus(*duration_ms)],
        Schedule::Recurring { recurrence, .. } => recurrence.upcoming(now, count, tz),
    })
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

// ---- timers ----

fn timer_duration(item: &Item) -> Result<i64> {
    match item.schedule {
        Some(Schedule::Timer { duration_ms }) => Ok(duration_ms),
        _ => Err(ActionError::NotATimer),
    }
}

/// Starts (or restarts) a timer from its full duration.
pub fn start_timer(state: &State, now: Timestamp, id: &str) -> Result<Change> {
    let item = live(state, id)?;
    let duration = timer_duration(item)?;
    let mut c = Change::default();
    c.item(
        id,
        field::TIMER,
        TimerState::Running {
            end_at: now.plus(duration),
        },
    );
    if let TimerState::Running { .. } | TimerState::Paused { .. } = item.timer {
        c.history.push(NewHistory {
            item_id: id.to_string(),
            kind: HistoryKind::Restart,
            occurrence: None,
            snooze_count: 0,
            title: item.title.clone(),
            ref_id: None,
            prev_completion: None,
        });
    }
    Ok(c)
}

/// Pauses a running timer, keeping the time left. A timer that has already
/// gone off can't be paused (mark it done or restart it instead).
pub fn pause_timer(state: &State, now: Timestamp, id: &str) -> Result<Change> {
    let item = live(state, id)?;
    timer_duration(item)?;
    let mut c = Change::default();
    if let TimerState::Running { end_at } = item.timer {
        if end_at > now {
            c.item(
                id,
                field::TIMER,
                TimerState::Paused {
                    remaining_ms: end_at.since(now),
                },
            );
        }
    }
    Ok(c)
}

pub fn resume_timer(state: &State, now: Timestamp, id: &str) -> Result<Change> {
    let item = live(state, id)?;
    timer_duration(item)?;
    let mut c = Change::default();
    if let TimerState::Paused { remaining_ms } = item.timer {
        c.item(
            id,
            field::TIMER,
            TimerState::Running {
                end_at: now.plus(remaining_ms.max(0)),
            },
        );
    }
    Ok(c)
}

/// Stops a timer without completing it.
pub fn reset_timer(state: &State, id: &str) -> Result<Change> {
    let item = live(state, id)?;
    timer_duration(item)?;
    let mut c = Change::default();
    if item.timer != TimerState::Idle {
        c.item(id, field::TIMER, TimerState::Idle);
    }
    Ok(c)
}

// ---- presets ----

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetDraft {
    pub name: String,
    pub duration_ms: i64,
    #[serde(default)]
    pub nag: Option<Nag>,
    #[serde(default)]
    pub chime: Option<ChimeRef>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub order: i64,
}

pub fn save_preset(state: &State, id: &str, draft: PresetDraft) -> Result<Change> {
    if draft.name.trim().is_empty() {
        return Err(ActionError::EmptyTitle);
    }
    if draft.duration_ms < 1_000 {
        return Err(ActionError::BadDuration);
    }
    let existing = state.presets.get(id);
    let mut c = Change::default();
    let mut put = |f: &str, v: serde_json::Value, same: bool| {
        if !same {
            c.writes.push(NewWrite {
                entity: Entity::Preset,
                id: id.to_string(),
                field: f.to_string(),
                value: v,
            });
        }
    };
    let nag = draft.nag.unwrap_or(state.settings.nag_default);
    put(
        field::NAME,
        draft.name.trim().into(),
        existing.is_some_and(|p| p.name == draft.name.trim()),
    );
    put(
        field::DURATION_MS,
        draft.duration_ms.into(),
        existing.is_some_and(|p| p.duration_ms == draft.duration_ms),
    );
    put(
        field::NAG,
        serde_json::to_value(nag).expect("nag"),
        existing.is_some_and(|p| p.nag == nag),
    );
    put(
        field::CHIME,
        serde_json::to_value(&draft.chime).expect("chime"),
        existing.is_some_and(|p| p.chime == draft.chime),
    );
    put(
        field::TAG,
        serde_json::to_value(&draft.tag).expect("tag"),
        existing.is_some_and(|p| p.tag == draft.tag),
    );
    put(
        field::ORDER,
        draft.order.into(),
        existing.is_some_and(|p| p.order == draft.order),
    );
    put(
        field::DELETED,
        false.into(),
        existing.is_some_and(|p| !p.deleted),
    );
    Ok(c)
}

pub fn delete_preset(state: &State, id: &str) -> Result<Change> {
    let preset = state.presets.get(id).ok_or(ActionError::NotFound)?;
    let mut c = Change::default();
    if !preset.deleted {
        c.writes
            .push(NewWrite::new(Entity::Preset, id, field::DELETED, true));
    }
    Ok(c)
}

/// Creates a running timer from a preset ("Tea 4m" → a new "Tea" timer).
pub fn start_preset(
    state: &State,
    device: &str,
    now: Timestamp,
    preset_id: &str,
    new_item_id: &str,
) -> Result<Change> {
    let preset = state
        .presets
        .get(preset_id)
        .filter(|p| !p.deleted)
        .ok_or(ActionError::NotFound)?;
    create_item(
        state,
        device,
        now,
        new_item_id,
        ItemDraft {
            title: preset.name.clone(),
            notes: String::new(),
            tag: preset.tag.clone(),
            schedule: Schedule::Timer {
                duration_ms: preset.duration_ms,
            },
            nag: Some(preset.nag),
            chime: preset.chime.clone(),
            quiet_exempt: None,
            start_timer: true,
        },
    )
}

// ---- tags ----

pub fn save_tag(state: &State, id: &str, name: &str, color: &str, order: i64) -> Result<Change> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ActionError::EmptyTitle);
    }
    let existing = state.tags.get(id);
    let mut c = Change::default();
    if existing.is_none_or(|t| t.name != name) {
        c.writes
            .push(NewWrite::new(Entity::Tag, id, field::NAME, name));
    }
    if existing.is_none_or(|t| t.color != color) {
        c.writes
            .push(NewWrite::new(Entity::Tag, id, field::COLOR, color));
    }
    if existing.is_none_or(|t| t.order != order) {
        c.writes
            .push(NewWrite::new(Entity::Tag, id, field::ORDER, order));
    }
    if existing.is_some_and(|t| t.deleted) {
        c.writes
            .push(NewWrite::new(Entity::Tag, id, field::DELETED, false));
    }
    Ok(c)
}

/// Deletes a tag. Items keep the id and simply show no tag.
pub fn delete_tag(state: &State, id: &str) -> Result<Change> {
    let tag = state.tags.get(id).ok_or(ActionError::NotFound)?;
    let mut c = Change::default();
    if !tag.deleted {
        c.writes
            .push(NewWrite::new(Entity::Tag, id, field::DELETED, true));
    }
    Ok(c)
}

// ---- settings ----

pub fn set_setting(key: &str, value: impl Serialize) -> Change {
    Change {
        writes: vec![NewWrite::new(
            Entity::Setting,
            field::SETTINGS_ID,
            key,
            value,
        )],
        history: Vec::new(),
    }
}

pub fn mute_for(now: Timestamp, minutes: u32) -> Change {
    set_setting(
        crate::model::item::setting_key::MUTE_UNTIL,
        Some(now.plus_minutes(i64::from(minutes))),
    )
}

pub fn mute_until_tomorrow(state: &State, now: Timestamp, tz: &TimeZone) -> Change {
    set_setting(
        crate::model::item::setting_key::MUTE_UNTIL,
        Some(crate::quiet::mute_until_tomorrow(
            now,
            &state.settings.quiet_hours,
            tz,
        )),
    )
}

pub fn unmute() -> Change {
    set_setting(
        crate::model::item::setting_key::MUTE_UNTIL,
        Option::<Timestamp>::None,
    )
}

/// "Snooze all" from the missed-rings summary.
pub fn snooze_many(
    state: &State,
    now: Timestamp,
    tz: &TimeZone,
    ids: &[String],
    minutes: u32,
) -> Result<Change> {
    let mut all = Change::default();
    for id in ids {
        match snooze(state, now, tz, id, None, minutes) {
            Ok(c) => {
                all.writes.extend(c.writes);
                all.history.extend(c.history);
            }
            Err(ActionError::NotFound | ActionError::Deleted) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(all)
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
    fn timer(title: &str, minutes: i64, start: bool) -> ItemDraft {
        ItemDraft {
            schedule: Schedule::Timer {
                duration_ms: minutes * MINUTE,
            },
            start_timer: start,
            ..reminder(title, Timestamp(0))
        }
    }

    #[test]
    fn pause_keeps_the_time_left_and_resume_continues_it() {
        let now = at(date(2026, 9, 16).at(9, 0, 0, 0));
        let mut s = created(timer("Laundry", 45, true), now);

        let later = now.plus(15 * MINUTE);
        let p = pause_timer(&s, later, "a").unwrap();
        apply(&mut s, &p, later);
        assert_eq!(
            s.items["a"].timer,
            TimerState::Paused {
                remaining_ms: 30 * MINUTE
            }
        );
        assert_eq!(
            status(
                s.items["a"].inputs().unwrap(),
                later.plus(60 * MINUTE),
                &UTC
            ),
            Status::Idle,
            "paused timers never ring"
        );

        let resumed_at = later.plus(60 * MINUTE);
        let r = resume_timer(&s, resumed_at, "a").unwrap();
        apply(&mut s, &r, resumed_at);
        assert_eq!(
            s.items["a"].timer,
            TimerState::Running {
                end_at: resumed_at.plus(30 * MINUTE)
            }
        );

        // Can't pause once it has gone off.
        assert!(pause_timer(&s, resumed_at.plus(31 * MINUTE), "a")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn restarting_a_finished_timer_rings_again_after_the_full_duration() {
        let now = at(date(2026, 9, 16).at(9, 0, 0, 0));
        let mut s = created(timer("Tea", 4, true), now);
        let rang = now.plus(4 * MINUTE);
        let d = done(&s, rang, &UTC, "a", None).unwrap();
        apply(&mut s, &d, rang);

        let again = rang.plus(MINUTE);
        let st = start_timer(&s, again, "a").unwrap();
        assert!(
            st.history.is_empty(),
            "starting an idle timer isn't a restart"
        );
        apply(&mut s, &st, again);
        assert!(
            status(s.items["a"].inputs().unwrap(), again.plus(4 * MINUTE), &UTC)
                .is_ringing(again.plus(4 * MINUTE)),
            "restarted timer rings after the full duration"
        );

        let rs = start_timer(&s, again.plus(MINUTE), "a").unwrap();
        assert_eq!(rs.history[0].kind, HistoryKind::Restart);
        assert_eq!(
            start_timer(&created(reminder("x", now), now), now, "a"),
            Err(ActionError::NotATimer)
        );
    }

    #[test]
    fn presets_create_running_timers_and_save_only_changes() {
        let now = at(date(2026, 9, 16).at(9, 0, 0, 0));
        let mut s = State::default();
        let draft = PresetDraft {
            name: "Pomodoro".into(),
            duration_ms: 25 * MINUTE,
            nag: Some(Nag::Repeat { interval_min: 2 }),
            chime: None,
            tag: None,
            order: 0,
        };
        let c = save_preset(&s, "p", draft.clone()).unwrap();
        apply(&mut s, &c, now);
        assert!(save_preset(&s, "p", draft).unwrap().is_empty());

        let c = start_preset(&s, "pc", now, "p", "t1").unwrap();
        apply(&mut s, &c, now);
        let t = &s.items["t1"];
        assert_eq!(t.title, "Pomodoro");
        assert_eq!(t.nag, Nag::Repeat { interval_min: 2 });
        assert_eq!(
            t.timer,
            TimerState::Running {
                end_at: now.plus(25 * MINUTE)
            }
        );

        let d = delete_preset(&s, "p").unwrap();
        apply(&mut s, &d, now);
        assert_eq!(
            start_preset(&s, "pc", now, "p", "t2"),
            Err(ActionError::NotFound)
        );
    }

    #[test]
    fn tags_save_and_delete() {
        let now = at(date(2026, 9, 16).at(9, 0, 0, 0));
        let mut s = State::default();
        let c = save_tag(&s, "t", " Home ", "#3a86ff", 1).unwrap();
        apply(&mut s, &c, now);
        assert_eq!(s.tags["t"].name, "Home");
        assert!(save_tag(&s, "t", "Home", "#3a86ff", 1).unwrap().is_empty());
        let d = delete_tag(&s, "t").unwrap();
        apply(&mut s, &d, now);
        assert!(s.tags["t"].deleted);
        let restored = save_tag(&s, "t", "Home", "#3a86ff", 1).unwrap();
        assert_eq!(restored.writes.len(), 1);
    }

    #[test]
    fn mute_helpers_and_snooze_all() {
        let now = at(date(2026, 9, 16).at(21, 0, 0, 0));
        let mut s = created(reminder("a", now.minus(MINUTE)), now.minus(2 * MINUTE));
        let c = create_item(&s, "pc", now, "b", reminder("b", now.minus(MINUTE))).unwrap();
        apply(&mut s, &c, now);

        let mute = mute_until_tomorrow(&s, now, &UTC);
        apply(&mut s, &mute, now);
        assert_eq!(
            s.settings.mute_until,
            Some(at(date(2026, 9, 17).at(7, 0, 0, 0)))
        );
        apply(&mut s, &unmute(), now);
        assert_eq!(s.settings.mute_until, None);
        apply(&mut s, &mute_for(now, 15), now);
        assert_eq!(s.settings.mute_until, Some(now.plus(15 * MINUTE)));

        let all = snooze_many(&s, now, &UTC, &["a".into(), "b".into(), "gone".into()], 15).unwrap();
        assert_eq!(all.writes.len(), 2);
    }
    #[test]
    fn preview_validates_and_lists_next_rings() {
        use crate::recurrence::{Recurrence, Rule};
        let now = at(date(2026, 9, 16).at(10, 0, 0, 0));
        let daily = ItemDraft {
            schedule: Schedule::Recurring {
                recurrence: Recurrence {
                    rule: Rule::Daily {
                        every: 1,
                        times: vec![time(9, 0, 0, 0), time(21, 0, 0, 0)],
                    },
                    start: date(2026, 9, 16).at(0, 0, 0, 0),
                    tz: None,
                },
                mode: Default::default(),
                effective_from: Timestamp(0),
            },
            ..reminder("Meds", Timestamp(0))
        };
        let next = preview(&daily, now, &UTC, 3).unwrap();
        assert_eq!(
            next,
            [
                at(date(2026, 9, 16).at(21, 0, 0, 0)),
                at(date(2026, 9, 17).at(9, 0, 0, 0)),
                at(date(2026, 9, 17).at(21, 0, 0, 0))
            ]
        );
        let timer = ItemDraft {
            schedule: Schedule::Timer {
                duration_ms: 45 * MINUTE,
            },
            ..reminder("x", now)
        };
        assert_eq!(
            preview(&timer, now, &UTC, 3).unwrap(),
            [now.plus(45 * MINUTE)]
        );
        assert_eq!(
            preview(&reminder(" ", now), now, &UTC, 3),
            Err(ActionError::EmptyTitle)
        );
    }
}
