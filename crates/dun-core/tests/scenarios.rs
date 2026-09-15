//! End-to-end scenarios through `Engine` with real SQLite stores: a PC and a
//! phone that sync by exchanging `changes_since` pages, as the network
//! protocol will.

use std::collections::{BTreeMap, BTreeSet};

use dun_core::actions::ItemDraft;
use dun_core::engine::Engine;
use dun_core::model::{Schedule, TimerState};
use dun_core::scheduler::{ringing_soon, Ack, AlertLevel, Effect, PcView, Role};
use dun_core::time::{resolve, TimeZone, Timestamp, MINUTE, SECOND};
use jiff::civil::date;

const UTC: TimeZone = TimeZone::UTC;

struct Device {
    engine: Engine,
    /// Highest seq of the *other* device already merged here.
    pulled: i64,
}

fn device(id: &str) -> Device {
    Device {
        engine: Engine::open_in_memory_as(id).unwrap(),
        pulled: 0,
    }
}

/// Delivers everything `from` has that `to` hasn't seen; returns how many
/// registers actually changed on `to`.
fn sync(from: &Device, to: &mut Device, now: Timestamp) -> usize {
    let mut changed = 0;
    loop {
        let changes = from.engine.store().changes_since(to.pulled, 50).unwrap();
        changed += to
            .engine
            .merge_remote(now, &changes.regs, &changes.history)
            .unwrap()
            .regs_changed;
        to.pulled = changes.max_seq;
        if !changes.more {
            return changed;
        }
    }
}

fn both_ways(pc: &mut Device, phone: &mut Device, now: Timestamp) {
    // Each receiver keeps its own cursor into the sender's changes.
    sync(pc, phone, now);
    sync(phone, pc, now);
    // The PC re-issued what it merged from the phone; delivering those echoes
    // must change nothing, after which both feeds are drained.
    assert_eq!(sync(pc, phone, now), 0, "echoes are no-ops");
    assert!(pc
        .engine
        .store()
        .changes_since(phone.pulled, 50)
        .unwrap()
        .regs
        .is_empty());
    assert!(phone
        .engine
        .store()
        .changes_since(pc.pulled, 50)
        .unwrap()
        .regs
        .is_empty());
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

fn alerts(effects: &[Effect]) -> Vec<(String, AlertLevel)> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::ShowAlert { item_id, level, .. } => Some((item_id.clone(), *level)),
            _ => None,
        })
        .collect()
}

fn cleared(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::ClearAlert { item_id } => Some(item_id.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn done_on_the_phone_stops_the_pc_nagging() {
    let t0 = resolve(date(2026, 9, 16).at(8, 55, 0, 0), &UTC);
    let due = t0.plus(5 * MINUTE);
    let mut pc = device("pc");
    let mut phone = device("phone");

    let id = pc
        .engine
        .create_item(t0, reminder("Pay rent", due))
        .unwrap();
    both_ways(&mut pc, &mut phone, t0);
    assert_eq!(phone.engine.state().items[&id].title, "Pay rent");

    let a = pc.engine.evaluate(due, &UTC, Role::Solo).unwrap();
    let b = phone
        .engine
        .evaluate(due, &UTC, Role::Phone { pc: None })
        .unwrap();
    assert_eq!(alerts(&a), [(id.clone(), AlertLevel::Full)]);
    assert_eq!(alerts(&b), [(id.clone(), AlertLevel::Full)]);

    let pressed = due.plus(20 * SECOND);
    phone.engine.done(pressed, &UTC, &id, Some(due)).unwrap();
    both_ways(&mut pc, &mut phone, pressed);

    let a = pc
        .engine
        .evaluate(due.plus(MINUTE), &UTC, Role::Solo)
        .unwrap();
    assert!(alerts(&a).is_empty());
    assert_eq!(cleared(&a), std::slice::from_ref(&id));
    let history = pc.engine.history(Some(&id), None, 10).unwrap();
    assert_eq!(
        history.len(),
        1,
        "the phone's Done shows up in the PC's history"
    );
    assert_eq!(history[0].device, "phone");
}

#[test]
fn undo_on_the_pc_beats_a_stale_done_from_the_phone() {
    let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &UTC);
    let mut pc = device("pc");
    let mut phone = device("phone");
    let id = pc.engine.create_item(t0, reminder("Meds", t0)).unwrap();
    both_ways(&mut pc, &mut phone, t0);

    // Both devices mark it done while apart; then the PC undoes its own Done.
    pc.engine.done(t0.plus(MINUTE), &UTC, &id, None).unwrap();
    phone
        .engine
        .done(t0.plus(2 * MINUTE), &UTC, &id, None)
        .unwrap();
    let undo_id = pc.engine.history(Some(&id), None, 1).unwrap()[0].id.clone();
    pc.engine.undo(t0.plus(3 * MINUTE), &undo_id).unwrap();

    both_ways(&mut pc, &mut phone, t0.plus(4 * MINUTE));
    for d in [&mut pc, &mut phone] {
        let now = t0.plus(5 * MINUTE);
        let e = d.engine.evaluate(now, &UTC, Role::Solo).unwrap();
        assert_eq!(
            alerts(&e).len(),
            1,
            "undo wins everywhere, so it rings again"
        );
    }
    assert!(matches!(
        pc.engine.undo(t0.plus(6 * MINUTE), &undo_id),
        Err(dun_core::engine::EngineError::Action(
            dun_core::actions::ActionError::AlreadyUndone
        ))
    ));
}

#[test]
fn restart_from_disk_neither_loses_nor_repeats_alerts() {
    let dir = std::env::temp_dir().join(format!("dun-scenario-{}", dun_core::ids::new_id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("dun.sqlite3");
    let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &UTC);
    let id = {
        let mut e = Engine::open(&path).unwrap();
        let id = e.create_item(t0, reminder("Stretch", t0)).unwrap();
        assert_eq!(alerts(&e.evaluate(t0, &UTC, Role::Solo).unwrap()).len(), 1);
        id
    };

    // Restart 20 seconds later: nothing new yet.
    let mut e = Engine::open(&path).unwrap();
    assert_eq!(e.state().items[&id].title, "Stretch");
    assert!(alerts(&e.evaluate(t0.plus(20 * SECOND), &UTC, Role::Solo).unwrap()).is_empty());
    // The next nag arrives on schedule.
    assert_eq!(
        alerts(&e.evaluate(t0.plus(MINUTE), &UTC, Role::Solo).unwrap()).len(),
        1
    );
    drop(e);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn handoff_round_trip_between_engines() {
    let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &UTC);
    let mut pc = device("pc");
    let mut phone = device("phone");
    let id = pc
        .engine
        .create_item(t0.minus(MINUTE), reminder("Call back", t0))
        .unwrap();
    both_ways(&mut pc, &mut phone, t0.minus(MINUTE));

    // Away from the PC. The phone checks in with what it's about to ring.
    let mut acks = BTreeMap::new();
    for (item, occ) in phone.engine.due_for_alert(t0, &UTC) {
        acks.insert(
            item,
            Ack {
                occurrence: occ,
                at: t0,
            },
        );
    }
    let pc_view = PcView {
        attended: false,
        ringing: ringing_soon(pc.engine.state(), t0, &UTC, 5 * SECOND),
    };
    let on_pc = pc
        .engine
        .evaluate(
            t0,
            &UTC,
            Role::Hub {
                attended: false,
                acks: &acks,
            },
        )
        .unwrap();
    let on_phone = phone
        .engine
        .evaluate(t0, &UTC, Role::Phone { pc: Some(&pc_view) })
        .unwrap();
    assert_eq!(alerts(&on_pc), [(id.clone(), AlertLevel::Silent)]);
    assert_eq!(alerts(&on_phone), [(id.clone(), AlertLevel::Full)]);

    // Back at the PC a minute later: the PC rings, the phone stands down.
    let back = t0.plus(MINUTE);
    let on_pc = pc
        .engine
        .evaluate(
            back,
            &UTC,
            Role::Hub {
                attended: true,
                acks: &acks,
            },
        )
        .unwrap();
    assert_eq!(alerts(&on_pc), [(id.clone(), AlertLevel::Full)]);
    let pc_view = PcView {
        attended: true,
        ringing: ringing_soon(pc.engine.state(), back, &UTC, 5 * SECOND),
    };
    let on_phone = phone
        .engine
        .evaluate(back, &UTC, Role::Phone { pc: Some(&pc_view) })
        .unwrap();
    assert!(alerts(&on_phone).is_empty());
    assert_eq!(cleared(&on_phone), [id]);
}

#[test]
fn timers_presets_and_settings_sync() {
    let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &UTC);
    let mut pc = device("pc");
    let mut phone = device("phone");

    let preset = pc
        .engine
        .save_preset(
            t0,
            None,
            dun_core::actions::PresetDraft {
                name: "Tea".into(),
                duration_ms: 4 * MINUTE,
                nag: None,
                chime: None,
                tag: None,
                order: 0,
            },
        )
        .unwrap();
    pc.engine.mute_for(t0, 15).unwrap();
    both_ways(&mut pc, &mut phone, t0);
    assert_eq!(phone.engine.state().presets[&preset].name, "Tea");
    assert_eq!(
        phone.engine.state().settings.mute_until,
        Some(t0.plus(15 * MINUTE))
    );

    let timer = phone.engine.start_preset(t0.plus(MINUTE), &preset).unwrap();
    phone
        .engine
        .pause_timer(t0.plus(2 * MINUTE), &timer)
        .unwrap();
    both_ways(&mut pc, &mut phone, t0.plus(2 * MINUTE));
    assert_eq!(
        pc.engine.state().items[&timer].timer,
        TimerState::Paused {
            remaining_ms: 3 * MINUTE
        }
    );

    pc.engine.unmute(t0.plus(3 * MINUTE)).unwrap();
    both_ways(&mut pc, &mut phone, t0.plus(3 * MINUTE));
    assert_eq!(phone.engine.state().settings.mute_until, None);
    let ids: BTreeSet<_> = phone.engine.state().items.keys().cloned().collect();
    assert_eq!(ids.len(), 1);
}

#[test]
fn backup_replace_through_the_engine_updates_state() {
    let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &UTC);
    let mut e = Engine::open_in_memory_as("pc").unwrap();
    let keep = e
        .create_item(t0, reminder("Keep", t0.plus(MINUTE)))
        .unwrap();
    let backup = e.export_backup(t0).unwrap();
    let extra = e
        .create_item(t0.plus(MINUTE), reminder("Extra", t0.plus(MINUTE)))
        .unwrap();

    e.import_backup(
        t0.plus(2 * MINUTE),
        &backup,
        dun_core::backup::ImportMode::Replace,
    )
    .unwrap();
    assert!(!e.state().items[&keep].deleted);
    assert!(e.state().items[&extra].deleted);
}
