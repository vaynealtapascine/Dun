//! The synced data: per-field registers and append-only history rows.

use serde::{Deserialize, Serialize};

use crate::hlc::{Hlc, Stamp};
use crate::model::Completion;
use crate::time::Timestamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Entity {
    Item,
    Tag,
    Preset,
    Setting,
}

impl Entity {
    pub fn as_str(self) -> &'static str {
        match self {
            Entity::Item => "item",
            Entity::Tag => "tag",
            Entity::Preset => "preset",
            Entity::Setting => "setting",
        }
    }

    pub fn parse(s: &str) -> Option<Entity> {
        Some(match s {
            "item" => Entity::Item,
            "tag" => Entity::Tag,
            "preset" => Entity::Preset,
            "setting" => Entity::Setting,
            _ => return None,
        })
    }
}

/// Field names. Kept in one place because they are part of the sync format.
pub mod field {
    pub const CREATED: &str = "created";
    pub const TITLE: &str = "title";
    pub const NOTES: &str = "notes";
    pub const TAG: &str = "tag";
    pub const SCHEDULE: &str = "schedule";
    pub const TIMER: &str = "timer";
    pub const NAG: &str = "nag";
    pub const CHIME: &str = "chime";
    pub const QUIET_EXEMPT: &str = "quietExempt";
    pub const COMPLETION: &str = "completion";
    pub const SNOOZE: &str = "snooze";
    pub const DELETED: &str = "deleted";

    pub const NAME: &str = "name";
    pub const COLOR: &str = "color";
    pub const ORDER: &str = "order";
    pub const DURATION_MS: &str = "durationMs";

    pub const DATE: &str = "date";
    pub const TIME: &str = "time";
    pub const REPEAT: &str = "repeat";
    pub const DIRECTION: &str = "direction";
    pub const DISPLAY: &str = "display";
    pub const PINNED: &str = "pinned";

    /// All settings live on one entity id.
    pub const SETTINGS_ID: &str = "global";

    /// Countdowns ride on the `setting` entity, as `countdown:<id>`. Builds
    /// before 1.4 refuse a sync carrying an entity they don't know, but they
    /// store and pass on setting ids other than [`SETTINGS_ID`] untouched.
    pub const COUNTDOWN_PREFIX: &str = "countdown:";

    /// The register id for countdown `id`.
    pub fn countdown_id(id: &str) -> String {
        format!("{COUNTDOWN_PREFIX}{id}")
    }
}

/// One field of one entity, with the stamp of the write that set it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reg {
    pub entity: Entity,
    pub id: String,
    pub field: String,
    pub value: serde_json::Value,
    pub hlc: Hlc,
    pub device: String,
}

impl Reg {
    pub fn stamp(&self) -> Stamp {
        Stamp {
            hlc: self.hlc,
            device: self.device.clone(),
        }
    }

    pub fn key(&self) -> (Entity, &str, &str) {
        (self.entity, &self.id, &self.field)
    }

    /// The countdown this register belongs to, if any.
    pub fn countdown(&self) -> Option<&str> {
        countdown_of(self.entity, &self.id)
    }
}

/// The countdown id behind an entity key, if it is one.
pub fn countdown_of(entity: Entity, id: &str) -> Option<&str> {
    match entity {
        Entity::Setting => id.strip_prefix(field::COUNTDOWN_PREFIX),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistoryKind {
    Done,
    Undo,
    Restart,
}

impl HistoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            HistoryKind::Done => "done",
            HistoryKind::Undo => "undo",
            HistoryKind::Restart => "restart",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "done" => HistoryKind::Done,
            "undo" => HistoryKind::Undo,
            "restart" => HistoryKind::Restart,
            _ => return None,
        })
    }
}

/// An immutable history entry. Devices union these by `id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRow {
    pub id: String,
    pub item_id: String,
    pub kind: HistoryKind,
    pub occurrence: Option<Timestamp>,
    pub at: Timestamp,
    pub snooze_count: u32,
    /// Title at the time, so history reads correctly after renames or deletes.
    pub title: String,
    /// For `undo`: the `done` entry it reverses.
    pub ref_id: Option<String>,
    /// For `done`: the completion it replaced, which `undo` restores.
    pub prev_completion: Option<Completion>,
    pub hlc: Hlc,
    pub device: String,
}
