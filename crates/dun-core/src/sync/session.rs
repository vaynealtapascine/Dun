//! One sync exchange, without any networking.
//!
//! The client (a phone, or a PC syncing with another PC) builds a request
//! from its store, the PC answers from its own, and each applies what it got.
//! Keeping this here means the rules (drift guard, echo suppression, cursors,
//! handoff acks, presence) are tested with real engines and no sockets, and
//! the transport crate stays thin.

use crate::engine::{Engine, EngineError};
use crate::scheduler::ringing_soon;
use crate::sync::mesh::Mesh;
use crate::sync::protocol::{
    error, is_newer, DueItem, ErrorBody, Pull, Push, SyncRequest, SyncResponse, UpdateOffer,
    PAGE_ROWS, SKEW_WARN_MS,
};
use crate::time::{TimeZone, Timestamp, DAY, SECOND};

/// How far ahead the PC looks when telling the phone what it is about to ring.
pub const RINGING_SOON_MS: i64 = 5 * SECOND;

/// Clocks further apart than this can't be reconciled safely, so the push is
/// refused rather than stamped into the future.
pub const MAX_DRIFT_MS: i64 = DAY;

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(
        "{0} and this PC disagree about the time by more than a day; fix the clocks and try again"
    )]
    ClockDrift(String),
    #[error(transparent)]
    Engine(#[from] EngineError),
}

impl SessionError {
    pub fn body(&self) -> ErrorBody {
        match self {
            SessionError::ClockDrift(_) => ErrorBody::new(error::CLOCK_DRIFT, self.to_string()),
            SessionError::Engine(e) => ErrorBody::new(error::BAD_REQUEST, e.to_string()),
        }
    }
}

/// What the PC knows that isn't in the store.
pub struct ServerView<'a> {
    /// Someone is at this PC.
    pub attended: bool,
    /// Addresses the phone should try next time, freshest first.
    pub addrs: Vec<String>,
    pub tz: &'a TimeZone,
    /// An Android build this PC is holding, whatever its version.
    pub update: Option<UpdateOffer>,
}

/// Handles a client's request: merge what it pushed, note what it is about to
/// ring, and answer with a page of everything it hasn't seen. A PC client also
/// trades presence and acknowledgements through `mesh`.
pub fn handle_sync(
    engine: &mut Engine,
    request: &SyncRequest,
    now: Timestamp,
    view: &ServerView<'_>,
    mesh: &mut Mesh,
) -> Result<SyncResponse, SessionError> {
    let skew = request.now.since(now);
    if skew.abs() > MAX_DRIFT_MS {
        return Err(SessionError::ClockDrift(request.device_id.clone()));
    }

    if !request.push.is_empty() {
        engine.merge_remote(now, &request.push.registers, &request.push.history)?;
    }

    // The phone reports these just before it alerts them, so the PC can stay
    // quiet while the phone is covering the ring. Another PC passes on the
    // ones it was given.
    mesh.merge_acks(&request.due_items, now);
    let from_pc = request.attended.is_some();
    let own_id = engine.device_id().to_string();
    if from_pc {
        mesh.merge_presence(&own_id, &request.presence);
    }

    let pull = page_for(engine, request.pulled_upto, &request.device_id)?;

    Ok(SyncResponse {
        pc_now: now,
        acked_upto: request.push.max_seq,
        pull,
        attended: mesh.anyone_attended(view.attended, now),
        ringing_soon: ringing_soon(engine.state(), now, view.tz, RINGING_SOON_MS)
            .into_iter()
            .map(|(item, occurrence)| DueItem {
                item,
                occurrence,
                at: None,
            })
            .collect(),
        addrs: view.addrs.clone(),
        skew_warning: (skew.abs() > SKEW_WARN_MS).then_some(skew),
        // Offered only to a phone running something older, so a device that is
        // already up to date is never nagged to reinstall what it has.
        update: view
            .update
            .clone()
            .filter(|u| is_newer(&u.version, &request.app_ver)),
        acks: if from_pc { mesh.acks_out() } else { Vec::new() },
        presence: if from_pc {
            mesh.presence_for(&own_id, view.attended, now, &request.device_id)
        } else {
            Vec::new()
        },
    })
}

/// A page of local changes after `cursor`, leaving out rows the peer wrote
/// itself (it already has them, and echoing them back doubles every sync).
fn page_for(engine: &Engine, cursor: i64, peer_device: &str) -> Result<Pull, EngineError> {
    let changes = engine.store().changes_since(cursor, PAGE_ROWS)?;
    Ok(Pull {
        registers: changes
            .regs
            .into_iter()
            .filter(|r| r.device != peer_device)
            .collect(),
        history: changes
            .history
            .into_iter()
            .filter(|h| h.device != peer_device)
            .collect(),
        new_pulled_upto: changes.max_seq,
        more: changes.more,
    })
}

/// Builds the phone's next request: everything it changed since `pushed_upto`
/// (except rows the PC wrote), plus what it is about to alert.
pub fn build_request(
    engine: &Engine,
    device_id: &str,
    peer_device: &str,
    pulled_upto: i64,
    pushed_upto: i64,
    due_items: Vec<DueItem>,
    now: Timestamp,
) -> Result<SyncRequest, EngineError> {
    let page = page_for(engine, pushed_upto, peer_device)?;
    Ok(SyncRequest {
        device_id: device_id.to_string(),
        now,
        pulled_upto,
        push: Push {
            registers: page.registers,
            history: page.history,
            max_seq: page.new_pulled_upto,
        },
        due_items,
        app_ver: crate::VERSION.to_string(),
        schema_ver: crate::store::CURRENT_VERSION,
        attended: None,
        presence: Vec::new(),
    })
}

/// Builds a PC's request to another PC: the same as a phone's, plus whether
/// someone is here, who else is, and the acknowledgements to pass on.
#[allow(clippy::too_many_arguments)]
pub fn build_pc_request(
    engine: &Engine,
    mesh: &Mesh,
    attended: bool,
    peer_device: &str,
    pulled_upto: i64,
    pushed_upto: i64,
    now: Timestamp,
) -> Result<SyncRequest, EngineError> {
    let own_id = engine.device_id();
    let mut request = build_request(
        engine,
        own_id,
        peer_device,
        pulled_upto,
        pushed_upto,
        mesh.acks_out(),
        now,
    )?;
    request.attended = Some(attended);
    request.presence = mesh.presence_for(own_id, attended, now, peer_device);
    Ok(request)
}

/// Takes in the presence and acknowledgements another PC answered with.
pub fn apply_mesh(engine: &Engine, mesh: &mut Mesh, response: &SyncResponse, now: Timestamp) {
    mesh.merge_acks(&response.acks, now);
    mesh.merge_presence(engine.device_id(), &response.presence);
}

/// Cursors after applying a response: `(pulled_upto, pushed_upto)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursors {
    pub pulled_upto: i64,
    pub pushed_upto: i64,
    /// The response had more waiting; sync again straight away.
    pub more: bool,
}

/// Merges what the PC sent and returns the new cursors.
pub fn apply_response(
    engine: &mut Engine,
    request: &SyncRequest,
    response: &SyncResponse,
    now: Timestamp,
) -> Result<Cursors, EngineError> {
    if !response.pull.registers.is_empty() || !response.pull.history.is_empty() {
        engine.merge_remote(now, &response.pull.registers, &response.pull.history)?;
    }
    Ok(Cursors {
        pulled_upto: response.pull.new_pulled_upto.max(request.pulled_upto),
        pushed_upto: response.acked_upto.max(0),
        more: response.pull.more,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ItemDraft;
    use crate::model::Schedule;
    use crate::scheduler::Role;
    use crate::time::{resolve, MINUTE};
    use jiff::civil::date;
    use std::collections::BTreeMap;

    const UTC: TimeZone = TimeZone::UTC;

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

    struct Pair {
        pc: Engine,
        phone: Engine,
        mesh: Mesh,
        /// Phone's cursors.
        cursors: Cursors,
    }

    impl Pair {
        fn new() -> Self {
            Pair {
                pc: Engine::open_in_memory_as("pc").unwrap(),
                phone: Engine::open_in_memory_as("phone").unwrap(),
                mesh: Mesh::default(),
                cursors: Cursors {
                    pulled_upto: 0,
                    pushed_upto: 0,
                    more: false,
                },
            }
        }

        /// One full exchange, looping while the PC has more pages.
        fn sync(&mut self, now: Timestamp, attended: bool, due: Vec<DueItem>) -> SyncResponse {
            loop {
                let request = build_request(
                    &self.phone,
                    "phone",
                    "pc",
                    self.cursors.pulled_upto,
                    self.cursors.pushed_upto,
                    due.clone(),
                    now,
                )
                .unwrap();
                let view = ServerView {
                    attended,
                    addrs: vec!["192.168.1.10".into()],
                    tz: &UTC,
                    update: None,
                };
                let response =
                    handle_sync(&mut self.pc, &request, now, &view, &mut self.mesh).unwrap();
                self.cursors = apply_response(&mut self.phone, &request, &response, now).unwrap();
                if !self.cursors.more {
                    return response;
                }
            }
        }
    }

    /// A PC in a mesh test: its engine, what it hears from other PCs, and its
    /// cursors for each PC it syncs with as a client.
    struct Node {
        engine: Engine,
        mesh: Mesh,
        attended: bool,
        cursors: BTreeMap<String, Cursors>,
    }

    impl Node {
        fn new(id: &str) -> Self {
            Node {
                engine: Engine::open_in_memory_as(id).unwrap(),
                mesh: Mesh::default(),
                attended: false,
                cursors: BTreeMap::new(),
            }
        }
    }

    /// `client` syncs with `server` until it has everything.
    fn pc_sync(client: &mut Node, server: &mut Node, now: Timestamp) -> SyncResponse {
        let server_id = server.engine.device_id().to_string();
        loop {
            let c = client.cursors.get(&server_id).copied().unwrap_or(Cursors {
                pulled_upto: 0,
                pushed_upto: 0,
                more: false,
            });
            let request = build_pc_request(
                &client.engine,
                &client.mesh,
                client.attended,
                &server_id,
                c.pulled_upto,
                c.pushed_upto,
                now,
            )
            .unwrap();
            let view = ServerView {
                attended: server.attended,
                addrs: vec![],
                tz: &UTC,
                update: None,
            };
            let response =
                handle_sync(&mut server.engine, &request, now, &view, &mut server.mesh).unwrap();
            let next = apply_response(&mut client.engine, &request, &response, now).unwrap();
            apply_mesh(&client.engine, &mut client.mesh, &response, now);
            client.cursors.insert(server_id.clone(), next);
            if !next.more {
                return response;
            }
        }
    }

    #[test]
    fn pcs_that_are_not_paired_still_converge_through_one_that_is() {
        let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &UTC);
        let (mut a, mut b, mut c) = (Node::new("a"), Node::new("b"), Node::new("c"));
        let from_a = a
            .engine
            .create_item(t0, reminder("From A", t0.plus(MINUTE)))
            .unwrap();
        let from_c = c
            .engine
            .create_item(t0, reminder("From C", t0.plus(MINUTE)))
            .unwrap();

        // A and C each sync with B only, in whatever order they happen to.
        let mut now = t0;
        for _ in 0..2 {
            now = now.plus(10 * SECOND);
            pc_sync(&mut a, &mut b, now);
            pc_sync(&mut c, &mut b, now);
        }
        for node in [&a, &b, &c] {
            let items = &node.engine.state().items;
            assert!(items.contains_key(&from_a) && items.contains_key(&from_c));
        }

        // Done on C reaches A by way of B.
        c.engine
            .done(now, &UTC, &from_a, Some(t0.plus(MINUTE)))
            .unwrap();
        pc_sync(&mut c, &mut b, now.plus(SECOND));
        pc_sync(&mut a, &mut b, now.plus(2 * SECOND));
        assert_eq!(
            a.engine.state().items[&from_a].completion,
            c.engine.state().items[&from_a].completion
        );

        // And then it goes quiet: nothing bounces around the mesh forever.
        let quiet = pc_sync(&mut a, &mut b, now.plus(3 * SECOND));
        assert!(quiet.pull.registers.is_empty() && quiet.pull.history.is_empty());
        let quiet = pc_sync(&mut c, &mut b, now.plus(3 * SECOND));
        assert!(quiet.pull.registers.is_empty() && quiet.pull.history.is_empty());
    }

    #[test]
    fn presence_and_phone_acks_reach_pcs_that_are_not_paired() {
        let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &UTC);
        let (mut a, mut b, mut c) = (Node::new("a"), Node::new("b"), Node::new("c"));
        c.attended = true;

        // A phone paired with C says it is covering a ring.
        let phone = Engine::open_in_memory_as("phone").unwrap();
        let ack = DueItem {
            item: "meds".into(),
            occurrence: t0,
            at: None,
        };
        let request = build_request(&phone, "phone", "c", 0, 0, vec![ack], t0).unwrap();
        let view = ServerView {
            attended: true,
            addrs: vec![],
            tz: &UTC,
            update: None,
        };
        let response = handle_sync(&mut c.engine, &request, t0, &view, &mut c.mesh).unwrap();
        assert!(response.attended);

        pc_sync(&mut c, &mut b, t0.plus(SECOND));
        pc_sync(&mut a, &mut b, t0.plus(2 * SECOND));

        assert_eq!(
            a.mesh.acks.get("meds").map(|ack| ack.at),
            Some(t0),
            "the ack keeps the time C heard it, not when A did"
        );
        assert_eq!(a.mesh.peer_attended_at(), Some(t0.plus(SECOND)));
        assert!(a.mesh.anyone_attended(false, t0.plus(2 * SECOND)));

        // Someone leaves C; within the freshness window everyone knows.
        c.attended = false;
        let later = t0.plus(SECOND + crate::scheduler::PRESENCE_FRESH_MS + SECOND);
        pc_sync(&mut c, &mut b, later);
        pc_sync(&mut a, &mut b, later);
        assert!(!a.mesh.anyone_attended(false, later));
        assert!(!b.mesh.anyone_attended(false, later));
    }

    #[test]
    fn changes_flow_both_ways_and_settle() {
        let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &UTC);
        let mut p = Pair::new();
        let on_pc =
            p.pc.create_item(t0, reminder("Pay rent", t0.plus(MINUTE)))
                .unwrap();
        let on_phone = p
            .phone
            .create_item(t0, reminder("Buy milk", t0.plus(2 * MINUTE)))
            .unwrap();

        let response = p.sync(t0, false, vec![]);
        assert_eq!(response.addrs, ["192.168.1.10"]);
        assert!(
            p.phone.state().items.contains_key(&on_pc),
            "phone got the PC's item"
        );
        assert!(
            p.pc.state().items.contains_key(&on_phone),
            "PC got the phone's item"
        );

        // A second sync with nothing new transfers nothing at all.
        let quiet = p.sync(t0.plus(MINUTE), false, vec![]);
        assert!(quiet.pull.registers.is_empty() && quiet.pull.history.is_empty());
        assert!(!quiet.pull.more);
    }

    #[test]
    fn an_update_is_offered_only_to_a_phone_running_something_older() {
        let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &UTC);
        let mut p = Pair::new();
        let offer = UpdateOffer {
            version: "0.2.0".into(),
            size: 12_345,
            sha256: "ab".repeat(32),
        };
        let ask = |app_ver: &str| {
            let mut request = build_request(&p.phone, "phone", "pc", 0, 0, vec![], t0).unwrap();
            request.app_ver = app_ver.to_string();
            request
        };
        let view = ServerView {
            attended: false,
            addrs: vec![],
            tz: &UTC,
            update: Some(offer.clone()),
        };

        let behind = handle_sync(&mut p.pc, &ask("0.1.0"), t0, &view, &mut p.mesh).unwrap();
        assert_eq!(behind.update, Some(offer.clone()));

        let current = handle_sync(&mut p.pc, &ask("0.2.0"), t0, &view, &mut p.mesh).unwrap();
        assert_eq!(current.update, None, "it already has this one");

        let ahead = handle_sync(&mut p.pc, &ask("0.3.0"), t0, &view, &mut p.mesh).unwrap();
        assert_eq!(ahead.update, None, "never offer a step backwards");
    }

    #[test]
    fn rows_are_not_echoed_back_to_their_author() {
        let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &UTC);
        let mut p = Pair::new();
        p.phone.create_item(t0, reminder("From phone", t0)).unwrap();
        p.sync(t0, false, vec![]);

        // The PC re-issued the phone's rows with its own seq; the next page
        // must not send them back.
        let request =
            build_request(&p.phone, "phone", "pc", 0, 0, vec![], t0.plus(MINUTE)).unwrap();
        let view = ServerView {
            attended: false,
            addrs: vec![],
            tz: &UTC,
            update: None,
        };
        let response =
            handle_sync(&mut p.pc, &request, t0.plus(MINUTE), &view, &mut p.mesh).unwrap();
        assert!(
            response.pull.registers.iter().all(|r| r.device != "phone"),
            "the PC echoed the phone's own rows back"
        );
    }

    #[test]
    fn due_items_become_handoff_acks_and_the_pc_reports_what_it_rings() {
        let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &UTC);
        let mut p = Pair::new();
        let id =
            p.pc.create_item(t0.minus(MINUTE), reminder("Meds", t0))
                .unwrap();
        p.sync(t0.minus(MINUTE), false, vec![]);

        let response = p.sync(
            t0,
            false,
            vec![DueItem {
                item: id.clone(),
                occurrence: t0,
                at: None,
            }],
        );
        assert_eq!(
            p.mesh.acks.get(&id),
            Some(&crate::scheduler::Ack {
                occurrence: t0,
                at: t0
            })
        );
        assert!(
            response.acks.is_empty() && response.presence.is_empty(),
            "a phone gets nothing it doesn't use"
        );
        assert_eq!(
            response.ringing_soon,
            [DueItem {
                item: id,
                occurrence: t0,
                at: None,
            }]
        );
    }

    #[test]
    fn done_on_the_phone_reaches_the_pc_and_stops_its_ring() {
        let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &UTC);
        let mut p = Pair::new();
        let id =
            p.pc.create_item(t0.minus(MINUTE), reminder("Stretch", t0))
                .unwrap();
        p.sync(t0.minus(MINUTE), false, vec![]);
        assert_eq!(
            p.pc.evaluate(t0, &UTC, Role::Solo)
                .unwrap()
                .iter()
                .filter(|e| matches!(e, crate::scheduler::Effect::ShowAlert { .. }))
                .count(),
            1
        );

        p.phone
            .done(t0.plus(10 * SECOND), &UTC, &id, Some(t0))
            .unwrap();
        p.sync(t0.plus(11 * SECOND), false, vec![]);

        let effects =
            p.pc.evaluate(t0.plus(12 * SECOND), &UTC, Role::Solo)
                .unwrap();
        assert!(effects.iter().any(
            |e| matches!(e, crate::scheduler::Effect::ClearAlert { item_id } if *item_id == id)
        ));
    }

    #[test]
    fn pages_are_bounded_and_the_phone_loops_until_drained() {
        let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &UTC);
        let mut p = Pair::new();
        // Each item writes several registers, so this is well over one page.
        for i in 0..(PAGE_ROWS / 4) {
            p.pc.create_item(t0, reminder(&format!("Item {i}"), t0.plus(MINUTE)))
                .unwrap();
        }
        let first = build_request(&p.phone, "phone", "pc", 0, 0, vec![], t0).unwrap();
        let view = ServerView {
            attended: false,
            addrs: vec![],
            tz: &UTC,
            update: None,
        };
        let response = handle_sync(&mut p.pc, &first, t0, &view, &mut p.mesh).unwrap();
        assert!(response.pull.more, "expected paging");
        assert!(response.pull.registers.len() <= PAGE_ROWS);

        p.sync(t0, false, vec![]); // loops while `more`
        assert_eq!(p.phone.state().items.len(), PAGE_ROWS / 4);
    }

    #[test]
    fn clock_skew_warns_and_wild_drift_is_refused() {
        let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &UTC);
        let mut p = Pair::new();
        let view = ServerView {
            attended: true,
            addrs: vec![],
            tz: &UTC,
            update: None,
        };

        let mut request = build_request(&p.phone, "phone", "pc", 0, 0, vec![], t0).unwrap();
        request.now = t0.plus(45 * SECOND);
        let response = handle_sync(&mut p.pc, &request, t0, &view, &mut p.mesh).unwrap();
        assert_eq!(response.skew_warning, Some(45 * SECOND));
        assert!(response.attended);

        request.now = t0.plus(2 * DAY);
        assert!(matches!(
            handle_sync(&mut p.pc, &request, t0, &view, &mut p.mesh),
            Err(SessionError::ClockDrift(_))
        ));
    }
}
