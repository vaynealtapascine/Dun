//! The parts of an item that decide *when* it rings. Each type here is the
//! JSON value of one synced register (see `sync::merge` for how concurrent
//! edits combine).

use serde::{Deserialize, Serialize};

use crate::recurrence::Recurrence;
use crate::time::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Schedule {
    /// Rings once at `due`.
    #[serde(rename_all = "camelCase")]
    OneOff { due: Timestamp },
    /// Rings on every slot of `recurrence`.
    #[serde(rename_all = "camelCase")]
    Recurring {
        recurrence: Recurrence,
        mode: RepeatMode,
        /// Set whenever the rule is edited. Slots before it never count as
        /// missed, so an edit can't produce a burst of phantom rings.
        effective_from: Timestamp,
    },
    /// A countdown of `duration_ms`; its run state lives in [`TimerState`].
    #[serde(rename_all = "camelCase")]
    Timer { duration_ms: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum RepeatMode {
    /// The next ring is the next slot of the pattern ("every day at 9").
    #[default]
    FromSchedule,
    /// The next ring counts from when the last one was marked done
    /// ("every 3 hours after I take it").
    AfterCompletion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum TimerState {
    #[default]
    Idle,
    #[serde(rename_all = "camelCase")]
    Running { end_at: Timestamp },
    #[serde(rename_all = "camelCase")]
    Paused { remaining_ms: i64 },
}

/// Which occurrences are finished.
///
/// Merges as a max-register on `(gen, through)`: two devices marking the same
/// occurrence done write identical values, and "undo" bumps `gen` so it beats
/// a stale Done from the other device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Completion {
    /// Due instant of the latest finished occurrence.
    pub through: Option<Timestamp>,
    /// When it was marked done (drives "after completion" repeats).
    pub done_at: Option<Timestamp>,
    pub gen: u32,
}

/// A snooze applies only to the occurrence it names, so Done always beats a
/// stale snooze without comparing timestamps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snooze {
    pub occurrence: Timestamp,
    pub until: Timestamp,
    pub count: u32,
}

/// How an item nags once it rings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum Nag {
    #[serde(rename_all = "camelCase")]
    Repeat {
        interval_min: u32,
    },
    Once,
}

impl Default for Nag {
    fn default() -> Self {
        Nag::Repeat { interval_min: 1 }
    }
}
