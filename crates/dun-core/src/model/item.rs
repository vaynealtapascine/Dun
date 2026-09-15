//! Items, tags, presets and synced settings as the rest of the app sees them,
//! projected from registers.

use jiff::civil::Time;
use serde::{Deserialize, Serialize};

use crate::occurrence::Inputs;
use crate::quiet::QuietHours;
use crate::time::Timestamp;

use super::schedule::{Completion, Nag, Schedule, Snooze, TimerState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    Reminder,
    Recurring,
    Timer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Created {
    pub at: Timestamp,
    pub kind: ItemKind,
    pub by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ChimeRef {
    /// One of the chimes shipped with Dun (also available on Android).
    Bundled { id: String },
    /// A user-imported file, identified by content hash (desktop only).
    Custom { sha256: String, name: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub created: Option<Created>,
    pub title: String,
    pub notes: String,
    pub tag: Option<String>,
    pub schedule: Option<Schedule>,
    pub timer: TimerState,
    pub nag: Nag,
    pub chime: Option<ChimeRef>,
    /// `None` means "use the default for this kind" (see [`Item::quiet_exempt`]).
    pub quiet_exempt_override: Option<bool>,
    pub completion: Completion,
    pub snooze: Option<Snooze>,
    pub deleted: bool,
}

impl Item {
    pub fn new(id: impl Into<String>) -> Self {
        Item {
            id: id.into(),
            created: None,
            title: String::new(),
            notes: String::new(),
            tag: None,
            schedule: None,
            timer: TimerState::Idle,
            nag: Nag::default(),
            chime: None,
            quiet_exempt_override: None,
            completion: Completion::default(),
            snooze: None,
            deleted: false,
        }
    }

    pub fn kind(&self) -> ItemKind {
        match (&self.schedule, &self.created) {
            (Some(Schedule::OneOff { .. }), _) => ItemKind::Reminder,
            (Some(Schedule::Recurring { .. }), _) => ItemKind::Recurring,
            (Some(Schedule::Timer { .. }), _) => ItemKind::Timer,
            (None, Some(c)) => c.kind,
            (None, None) => ItemKind::Reminder,
        }
    }

    /// Whether this item rings through quiet hours. Timers do by default: a
    /// timer started at 23:30 is meant to go off.
    pub fn quiet_exempt(&self) -> bool {
        self.quiet_exempt_override
            .unwrap_or(self.kind() == ItemKind::Timer)
    }

    /// Items without a schedule (half-synced or corrupt) never ring.
    pub fn inputs(&self) -> Option<Inputs<'_>> {
        Some(Inputs {
            schedule: self.schedule.as_ref()?,
            timer: self.timer,
            completion: self.completion,
            snooze: self.snooze,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: String,
    pub name: String,
    /// CSS color, e.g. `#e0483e`.
    pub color: String,
    pub order: i64,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub duration_ms: i64,
    pub nag: Nag,
    pub chime: Option<ChimeRef>,
    pub tag: Option<String>,
    pub order: i64,
    pub deleted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffSettings {
    pub enabled: bool,
    /// An unattended PC rings anyway this long after an item is due if the
    /// phone hasn't confirmed it is covering the ring.
    pub fail_loud_after_s: u32,
    /// Extra slack after the phone's expected next check-in.
    pub phone_grace_s: u32,
}

impl Default for HandoffSettings {
    fn default() -> Self {
        HandoffSettings {
            enabled: true,
            fail_loud_after_s: 120,
            phone_grace_s: 120,
        }
    }
}

/// Settings every device shares.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub quiet_hours: QuietHours,
    pub mute_until: Option<Timestamp>,
    pub nag_default: Nag,
    /// Time used when quick-add parses a date without a time ("tomorrow").
    pub date_only_time: Time,
    /// More newly-missed rings than this collapse into one summary.
    pub missed_summary_threshold: u32,
    pub handoff: HandoffSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            quiet_hours: QuietHours::default(),
            mute_until: None,
            nag_default: Nag::default(),
            date_only_time: jiff::civil::time(9, 0, 0, 0),
            missed_summary_threshold: 3,
            handoff: HandoffSettings::default(),
        }
    }
}

/// Setting keys (the `field` of `setting/global` registers).
pub mod setting_key {
    pub const QUIET_HOURS: &str = "quietHours";
    pub const MUTE_UNTIL: &str = "muteUntil";
    pub const NAG_DEFAULT: &str = "nagDefault";
    pub const DATE_ONLY_TIME: &str = "dateOnlyTime";
    pub const MISSED_SUMMARY_THRESHOLD: &str = "missedSummaryThreshold";
    pub const HANDOFF: &str = "handoff";
}
