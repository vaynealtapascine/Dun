//! The read model the UI renders: every item with its live status, plus tags,
//! presets and settings, as one serializable value.

use serde::Serialize;

use crate::model::{Item, Preset, Settings, Tag};
use crate::occurrence::{status, Status};
use crate::scheduler::{Hold, Scheduler};
use crate::state::State;
use crate::time::{TimeZone, Timestamp};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StatusView {
    /// Ringing now (`snoozed: false`) or snoozed until `rings_at`.
    #[serde(rename_all = "camelCase")]
    Due {
        occurrence: Timestamp,
        first_missed: Option<Timestamp>,
        missed_count: u32,
        rings_at: Timestamp,
        snoozed: bool,
    },
    #[serde(rename_all = "camelCase")]
    Upcoming {
        at: Timestamp,
    },
    Idle,
    /// No readable schedule (e.g. synced from a newer version).
    Unscheduled,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RingView {
    pub alerts: u32,
    pub next_alert_at: Option<Timestamp>,
    /// "quiet", "mute" or "handoff" while an alert is being held back.
    pub held: Option<Hold>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemView<'a> {
    #[serde(flatten)]
    pub item: &'a Item,
    pub kind: crate::model::ItemKind,
    pub quiet_exempt: bool,
    pub status: StatusView,
    pub ring: Option<RingView>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot<'a> {
    pub now: Timestamp,
    pub tz: String,
    pub device_id: &'a str,
    pub items: Vec<ItemView<'a>>,
    pub tags: Vec<&'a Tag>,
    pub presets: Vec<&'a Preset>,
    pub settings: &'a Settings,
    pub summary: &'a [String],
}

pub fn build<'a>(
    state: &'a State,
    scheduler: &'a Scheduler,
    device_id: &'a str,
    now: Timestamp,
    tz: &TimeZone,
) -> Snapshot<'a> {
    let items = state
        .live_items()
        .map(|item| {
            let status = match item.inputs() {
                None => StatusView::Unscheduled,
                Some(inputs) => match status(inputs, now, tz) {
                    Status::Due {
                        occ,
                        rings_at,
                        snoozed,
                    } => StatusView::Due {
                        occurrence: occ.due,
                        first_missed: (occ.count > 1).then_some(occ.first),
                        missed_count: occ.count,
                        rings_at,
                        snoozed,
                    },
                    Status::Upcoming { at } => StatusView::Upcoming { at },
                    Status::Idle => StatusView::Idle,
                },
            };
            let ring = scheduler.rings().get(&item.id).map(|rs| RingView {
                alerts: rs.alerts,
                next_alert_at: (rs.next_alert_at < Timestamp::MAX).then_some(rs.next_alert_at),
                held: rs.hold,
            });
            ItemView {
                item,
                kind: item.kind(),
                quiet_exempt: item.quiet_exempt(),
                status,
                ring,
            }
        })
        .collect();

    Snapshot {
        now,
        tz: tz.iana_name().unwrap_or("UTC").to_string(),
        device_id,
        items,
        tags: state.tags.values().filter(|t| !t.deleted).collect(),
        presets: state.presets.values().filter(|p| !p.deleted).collect(),
        settings: &state.settings,
        summary: scheduler.summary(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ItemDraft;
    use crate::engine::Engine;
    use crate::model::Schedule;
    use crate::scheduler::Role;
    use crate::time::{resolve, MINUTE};
    use jiff::civil::date;

    #[test]
    fn snapshot_serializes_items_with_status_for_the_ui() {
        let tz = TimeZone::UTC;
        let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &tz);
        let mut e = Engine::open_in_memory_as("pc").unwrap();
        let draft = |title: &str, due| ItemDraft {
            title: title.into(),
            notes: "n".into(),
            tag: None,
            schedule: Schedule::OneOff { due },
            nag: None,
            chime: None,
            quiet_exempt: None,
            start_timer: false,
        };
        let now_id = e.create_item(t0, draft("Now", t0)).unwrap();
        let later_id = e
            .create_item(t0, draft("Later", t0.plus(60 * MINUTE)))
            .unwrap();
        e.evaluate(t0, &tz, Role::Solo).unwrap();

        let snap = build(e.state(), e.scheduler(), e.device_id(), t0, &tz);
        let json = serde_json::to_value(&snap).unwrap();
        let find = |id: &str| {
            json["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|i| i["id"] == id)
                .unwrap()
                .clone()
        };

        let now_item = find(&now_id);
        assert_eq!(now_item["title"], "Now");
        assert_eq!(now_item["kind"], "reminder");
        assert_eq!(now_item["status"]["kind"], "due");
        assert_eq!(now_item["status"]["snoozed"], false);
        assert_eq!(now_item["ring"]["alerts"], 1);

        let later = find(&later_id);
        assert_eq!(later["status"]["kind"], "upcoming");
        assert!(later["ring"].is_null());
        assert_eq!(json["deviceId"], "pc");
        assert_eq!(json["tz"], "UTC");
    }
}
