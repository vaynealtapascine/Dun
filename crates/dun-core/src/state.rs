//! In-memory projection of registers into items, tags, presets and settings.
//!
//! A register whose value doesn't parse (a newer peer's format, corruption)
//! leaves that field at its default and is counted in
//! [`State::unreadable`] rather than failing the whole load.

use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::model::item::setting_key;
use crate::model::{Item, Preset, Settings, Tag};
use crate::sync::{field, Entity, Reg};

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub items: BTreeMap<String, Item>,
    pub tags: BTreeMap<String, Tag>,
    pub presets: BTreeMap<String, Preset>,
    pub settings: Settings,
    /// Registers skipped because their value couldn't be read.
    pub unreadable: usize,
}

impl State {
    pub fn from_registers<'a>(regs: impl IntoIterator<Item = &'a Reg>) -> Self {
        let mut state = State::default();
        for r in regs {
            state.apply(r);
        }
        state
    }

    /// Folds one (winning) register into the projection.
    pub fn apply(&mut self, r: &Reg) {
        let ok = match r.entity {
            Entity::Item => {
                let item = self
                    .items
                    .entry(r.id.clone())
                    .or_insert_with(|| Item::new(r.id.clone()));
                apply_item(item, r)
            }
            Entity::Tag => {
                let tag = self.tags.entry(r.id.clone()).or_insert_with(|| Tag {
                    id: r.id.clone(),
                    name: String::new(),
                    color: "#888888".into(),
                    order: 0,
                    deleted: false,
                });
                apply_tag(tag, r)
            }
            Entity::Preset => {
                let preset = self.presets.entry(r.id.clone()).or_insert_with(|| Preset {
                    id: r.id.clone(),
                    name: String::new(),
                    duration_ms: 0,
                    nag: Default::default(),
                    chime: None,
                    tag: None,
                    order: 0,
                    deleted: false,
                });
                apply_preset(preset, r)
            }
            Entity::Setting => apply_setting(&mut self.settings, r),
        };
        if !ok {
            self.unreadable += 1;
        }
    }

    /// Live (not deleted) items.
    pub fn live_items(&self) -> impl Iterator<Item = &Item> {
        self.items.values().filter(|i| !i.deleted)
    }
}

fn set<T: DeserializeOwned>(slot: &mut T, r: &Reg) -> bool {
    match serde_json::from_value(r.value.clone()) {
        Ok(v) => {
            *slot = v;
            true
        }
        Err(_) => false,
    }
}

/// Unknown fields are accepted silently: a newer peer may add fields this
/// build doesn't know yet.
fn apply_item(item: &mut Item, r: &Reg) -> bool {
    match r.field.as_str() {
        field::CREATED => set(&mut item.created, r),
        field::TITLE => set(&mut item.title, r),
        field::NOTES => set(&mut item.notes, r),
        field::TAG => set(&mut item.tag, r),
        field::SCHEDULE => set(&mut item.schedule, r),
        field::TIMER => set(&mut item.timer, r),
        field::NAG => set(&mut item.nag, r),
        field::CHIME => set(&mut item.chime, r),
        field::QUIET_EXEMPT => set(&mut item.quiet_exempt_override, r),
        field::COMPLETION => set(&mut item.completion, r),
        field::SNOOZE => set(&mut item.snooze, r),
        field::DELETED => set(&mut item.deleted, r),
        _ => true,
    }
}

fn apply_tag(tag: &mut Tag, r: &Reg) -> bool {
    match r.field.as_str() {
        field::NAME => set(&mut tag.name, r),
        field::COLOR => set(&mut tag.color, r),
        field::ORDER => set(&mut tag.order, r),
        field::DELETED => set(&mut tag.deleted, r),
        _ => true,
    }
}

fn apply_preset(p: &mut Preset, r: &Reg) -> bool {
    match r.field.as_str() {
        field::NAME => set(&mut p.name, r),
        field::DURATION_MS => set(&mut p.duration_ms, r),
        field::NAG => set(&mut p.nag, r),
        field::CHIME => set(&mut p.chime, r),
        field::TAG => set(&mut p.tag, r),
        field::ORDER => set(&mut p.order, r),
        field::DELETED => set(&mut p.deleted, r),
        _ => true,
    }
}

fn apply_setting(s: &mut Settings, r: &Reg) -> bool {
    if r.id != field::SETTINGS_ID {
        return true;
    }
    match r.field.as_str() {
        setting_key::QUIET_HOURS => set(&mut s.quiet_hours, r),
        setting_key::MUTE_UNTIL => set(&mut s.mute_until, r),
        setting_key::NAG_DEFAULT => set(&mut s.nag_default, r),
        setting_key::DATE_ONLY_TIME => set(&mut s.date_only_time, r),
        setting_key::MISSED_SUMMARY_THRESHOLD => set(&mut s.missed_summary_threshold, r),
        setting_key::HANDOFF => set(&mut s.handoff, r),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hlc::Hlc;
    use crate::model::{ItemKind, Nag, Schedule};
    use crate::time::Timestamp;
    use serde_json::json;

    fn reg(entity: Entity, id: &str, field: &str, value: serde_json::Value) -> Reg {
        Reg {
            entity,
            id: id.into(),
            field: field.into(),
            value,
            hlc: Hlc::new(1, 0),
            device: "pc".into(),
        }
    }

    #[test]
    fn projects_items_tags_presets_and_settings() {
        let regs = vec![
            reg(Entity::Item, "a", field::TITLE, json!("Laundry")),
            reg(
                Entity::Item,
                "a",
                field::SCHEDULE,
                json!({"kind": "timer", "durationMs": 2_700_000}),
            ),
            reg(
                Entity::Item,
                "a",
                field::TIMER,
                json!({"state": "running", "endAt": 1_000}),
            ),
            reg(Entity::Item, "a", field::NAG, json!({"mode": "once"})),
            reg(Entity::Tag, "t", field::NAME, json!("Home")),
            reg(Entity::Tag, "t", field::COLOR, json!("#3a86ff")),
            reg(Entity::Preset, "p", field::NAME, json!("Tea")),
            reg(Entity::Preset, "p", field::DURATION_MS, json!(240_000)),
            reg(
                Entity::Setting,
                field::SETTINGS_ID,
                setting_key::MUTE_UNTIL,
                json!(5_000),
            ),
            reg(
                Entity::Setting,
                field::SETTINGS_ID,
                setting_key::QUIET_HOURS,
                json!({"enabled": true, "start": "22:30:00", "end": "06:45:00"}),
            ),
        ];
        let s = State::from_registers(&regs);
        let a = &s.items["a"];
        assert_eq!(a.title, "Laundry");
        assert_eq!(a.kind(), ItemKind::Timer);
        assert!(
            a.quiet_exempt(),
            "timers ring through quiet hours by default"
        );
        assert_eq!(a.nag, Nag::Once);
        assert!(matches!(
            a.schedule,
            Some(Schedule::Timer {
                duration_ms: 2_700_000
            })
        ));
        assert_eq!(s.tags["t"].color, "#3a86ff");
        assert_eq!(s.presets["p"].duration_ms, 240_000);
        assert_eq!(s.settings.mute_until, Some(Timestamp(5_000)));
        assert!(s.settings.quiet_hours.enabled);
        assert_eq!(s.unreadable, 0);
    }

    #[test]
    fn unreadable_values_keep_defaults_and_are_counted() {
        let regs = vec![
            reg(Entity::Item, "a", field::TITLE, json!("ok")),
            reg(
                Entity::Item,
                "a",
                field::SCHEDULE,
                json!({"kind": "fromTheFuture"}),
            ),
            reg(Entity::Item, "a", "someNewField", json!(42)),
        ];
        let s = State::from_registers(&regs);
        assert_eq!(s.items["a"].title, "ok");
        assert_eq!(s.items["a"].schedule, None);
        assert!(
            s.items["a"].inputs().is_none(),
            "no schedule means it never rings"
        );
        assert_eq!(s.unreadable, 1);
    }

    #[test]
    fn quiet_exemption_can_be_overridden_either_way() {
        let regs = vec![
            reg(
                Entity::Item,
                "r",
                field::SCHEDULE,
                json!({"kind": "oneOff", "due": 1}),
            ),
            reg(Entity::Item, "r", field::QUIET_EXEMPT, json!(true)),
            reg(
                Entity::Item,
                "t",
                field::SCHEDULE,
                json!({"kind": "timer", "durationMs": 1}),
            ),
            reg(Entity::Item, "t", field::QUIET_EXEMPT, json!(false)),
        ];
        let s = State::from_registers(&regs);
        assert!(s.items["r"].quiet_exempt());
        assert!(!s.items["t"].quiet_exempt());
    }

    #[test]
    fn deleted_items_are_hidden_from_live_items() {
        let regs = vec![
            reg(Entity::Item, "a", field::TITLE, json!("keep")),
            reg(Entity::Item, "b", field::TITLE, json!("gone")),
            reg(Entity::Item, "b", field::DELETED, json!(true)),
        ];
        let s = State::from_registers(&regs);
        let live: Vec<_> = s.live_items().map(|i| i.id.as_str()).collect();
        assert_eq!(live, ["a"]);
    }
}
