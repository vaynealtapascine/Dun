//! Two real engines syncing over TLS: the whole stack except the Tauri glue.

use std::sync::{Arc, Mutex};

use dun_core::actions::ItemDraft;
use dun_core::engine::Engine;
use dun_core::model::Schedule;
use dun_core::scheduler::{Ack, Effect, Role};
use dun_core::sync::mesh::Mesh;
use dun_core::sync::peers::{Peer, Peers};
use dun_core::sync::protocol::{
    DueItem, PairBack, PairRequest, PairResponse, SyncRequest, SyncResponse,
};
use dun_core::sync::session::{self, Cursors, ServerView};
use dun_core::time::{resolve, TimeZone, Timestamp, MINUTE, SECOND};
use dun_sync::cert::Identity;
use dun_sync::client::{self, SyncClient};
use dun_sync::pairing::{self, Pairing};
use dun_sync::server::{Backend, Refusal, Server};
use jiff::civil::date;

const UTC: TimeZone = TimeZone::UTC;

/// The PC: a real engine behind the real session rules.
struct Pc {
    engine: Mutex<Engine>,
    peers: Mutex<Peers>,
    pairing: Mutex<Option<Pairing>>,
    mesh: Mutex<Mesh>,
    now: Mutex<Timestamp>,
    attended: Mutex<bool>,
}

impl Pc {
    fn new(now: Timestamp) -> Arc<Pc> {
        Pc::named("pc", now)
    }

    fn named(id: &str, now: Timestamp) -> Arc<Pc> {
        Arc::new(Pc {
            engine: Mutex::new(Engine::open_in_memory_as(id).unwrap()),
            peers: Mutex::new(Peers::default()),
            pairing: Mutex::new(Some(Pairing::open(now))),
            mesh: Mutex::new(Mesh::default()),
            now: Mutex::new(now),
            attended: Mutex::new(false),
        })
    }

    fn code(&self) -> String {
        self.pairing
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .code()
            .to_string()
    }

    fn set_now(&self, now: Timestamp) {
        *self.now.lock().unwrap() = now;
    }
}

impl Backend for Pc {
    fn pair(&self, request: PairRequest) -> Result<PairResponse, Refusal> {
        let now = *self.now.lock().unwrap();
        let mut open = self.pairing.lock().unwrap();
        let pairing = open.as_mut().ok_or_else(Refusal::unauthorized)?;
        let id = self.engine.lock().unwrap().device_id().to_string();
        let (response, paired) = pairing::accept(pairing, &request, now, &id, "Desk PC")
            .map_err(|e| Refusal::new(403, e.body()))?;
        *open = None;

        let mut peer = Peer::new(&paired.device_id, &paired.name, now);
        peer.token_hash = Some(paired.token_hash);
        if let Some(back) = paired.back {
            peer.token = Some(back.token);
            peer.fingerprint = Some(back.fingerprint);
            peer.addrs = back.addrs;
            peer.port = back.port;
        }
        self.peers.lock().unwrap().upsert(peer);
        Ok(response)
    }

    fn sync(&self, token: &str, request: SyncRequest) -> Result<SyncResponse, Refusal> {
        let now = *self.now.lock().unwrap();
        let Some(peer_id) = self
            .peers
            .lock()
            .unwrap()
            .by_token(token, pairing::token_matches)
            .map(|p| p.device_id.clone())
        else {
            return Err(Refusal::unauthorized());
        };
        let view = ServerView {
            attended: *self.attended.lock().unwrap(),
            addrs: vec!["127.0.0.1".into()],
            tz: &UTC,
            update: None,
        };
        let response = session::handle_sync(
            &mut self.engine.lock().unwrap(),
            &request,
            now,
            &view,
            &mut self.mesh.lock().unwrap(),
        )
        .map_err(|e| Refusal::new(400, e.body()))?;
        if let Some(peer) = self.peers.lock().unwrap().get_mut(&peer_id) {
            peer.note_served(&request, now);
        }
        Ok(response)
    }
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

/// The phone: engine, cursors and a pinned client.
struct Phone {
    engine: Engine,
    cursors: Cursors,
    client: SyncClient,
}

impl Phone {
    /// One sync round, looping while the PC says there's more.
    async fn sync(&mut self, now: Timestamp, due: Vec<DueItem>) -> SyncResponse {
        loop {
            let request = session::build_request(
                &self.engine,
                "phone",
                "pc",
                self.cursors.pulled_upto,
                self.cursors.pushed_upto,
                due.clone(),
                now,
            )
            .unwrap();
            let response = self
                .client
                .sync(&request)
                .await
                .expect("sync should succeed");
            self.cursors =
                session::apply_response(&mut self.engine, &request, &response, now).unwrap();
            if !self.cursors.more {
                return response;
            }
        }
    }
}

async fn paired(now: Timestamp) -> (Arc<Pc>, Phone, Server) {
    let pc = Pc::new(now);
    let identity = Identity::generate().unwrap();
    let server = Server::start(pc.clone(), &identity, 0).unwrap();

    let response = client::pair(
        &identity.fingerprint(),
        "127.0.0.1",
        server.port(),
        &PairRequest {
            code: pc.code(),
            device_id: "phone".into(),
            name: "Pixel".into(),
            back: None,
        },
    )
    .await
    .unwrap();

    let phone = Phone {
        engine: Engine::open_in_memory_as("phone").unwrap(),
        cursors: Cursors {
            pulled_upto: 0,
            pushed_upto: 0,
            more: false,
        },
        client: SyncClient::new(
            &identity.fingerprint(),
            response.token,
            vec!["127.0.0.1".into()],
            server.port(),
        )
        .unwrap(),
    };
    (pc, phone, server)
}

#[tokio::test]
async fn items_flow_both_ways_over_the_network() {
    let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &UTC);
    let (pc, mut phone, _server) = paired(t0).await;

    let on_pc = pc
        .engine
        .lock()
        .unwrap()
        .create_item(t0, reminder("Pay rent", t0.plus(MINUTE)))
        .unwrap();
    let on_phone = phone
        .engine
        .create_item(t0, reminder("Buy milk", t0.plus(2 * MINUTE)))
        .unwrap();

    phone.sync(t0, vec![]).await;

    assert!(phone.engine.state().items.contains_key(&on_pc));
    assert!(pc
        .engine
        .lock()
        .unwrap()
        .state()
        .items
        .contains_key(&on_phone));
}

#[tokio::test]
async fn done_on_the_phone_stops_the_pc_ringing() {
    let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &UTC);
    let (pc, mut phone, _server) = paired(t0.minus(MINUTE)).await;

    let id = pc
        .engine
        .lock()
        .unwrap()
        .create_item(t0.minus(MINUTE), reminder("Meds", t0))
        .unwrap();
    phone.sync(t0.minus(MINUTE), vec![]).await;

    // The PC starts ringing it.
    let ringing = pc
        .engine
        .lock()
        .unwrap()
        .evaluate(t0, &UTC, Role::Solo)
        .unwrap();
    assert!(ringing
        .iter()
        .any(|e| matches!(e, Effect::ShowAlert { item_id, .. } if *item_id == id)));

    // Done on the phone, pushed on its next sync.
    phone
        .engine
        .done(t0.plus(5 * SECOND), &UTC, &id, Some(t0))
        .unwrap();
    pc.set_now(t0.plus(6 * SECOND));
    phone.sync(t0.plus(6 * SECOND), vec![]).await;

    let after = pc
        .engine
        .lock()
        .unwrap()
        .evaluate(t0.plus(7 * SECOND), &UTC, Role::Solo)
        .unwrap();
    assert!(after
        .iter()
        .any(|e| matches!(e, Effect::ClearAlert { item_id } if *item_id == id)));
}

#[tokio::test]
async fn the_phone_covers_a_ring_while_the_pc_is_unattended() {
    let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &UTC);
    let (pc, mut phone, _server) = paired(t0.minus(MINUTE)).await;

    let id = pc
        .engine
        .lock()
        .unwrap()
        .create_item(t0.minus(MINUTE), reminder("Stretch", t0))
        .unwrap();
    phone.sync(t0.minus(MINUTE), vec![]).await;

    // The phone checks in just before it alerts.
    pc.set_now(t0);
    let response = phone
        .sync(
            t0,
            vec![DueItem {
                item: id.clone(),
                occurrence: t0,
                at: None,
            }],
        )
        .await;
    assert!(!response.attended, "nobody is at the PC");
    assert_eq!(
        response.ringing_soon,
        [DueItem {
            item: id.clone(),
            occurrence: t0,
            at: None,
        }]
    );

    // The check-in landed as a handoff acknowledgement, stamped with the PC's
    // clock rather than the phone's.
    let acks = pc.mesh.lock().unwrap().acks.clone();
    assert_eq!(
        acks.get(&id),
        Some(&Ack {
            occurrence: t0,
            at: t0
        })
    );

    // Holding it, the PC stays quiet past the point where it would otherwise
    // fail loud (that branch is covered in the scheduler's own tests).
    let late = t0.plus(150 * SECOND);
    let effects = pc
        .engine
        .lock()
        .unwrap()
        .evaluate(
            late,
            &UTC,
            Role::Hub {
                attended: false,
                acks: &acks,
                peer_attended_at: None,
            },
        )
        .unwrap();
    let full = effects.iter().any(|e| {
        matches!(e, Effect::ShowAlert { level, .. } if *level == dun_core::scheduler::AlertLevel::Full)
    });
    assert!(
        !full,
        "a covering phone should keep the PC quiet, got {effects:?}"
    );
}

impl Pc {
    /// One sync with another PC as its client, as the desktop's PC loop does.
    async fn sync_with(&self, other: &str, now: Timestamp) -> SyncResponse {
        let peer = self.peers.lock().unwrap().get(other).cloned().unwrap();
        let client = SyncClient::new(
            peer.fingerprint.as_deref().unwrap(),
            peer.token.clone().unwrap(),
            peer.addrs.clone(),
            peer.port,
        )
        .unwrap();
        let attended = *self.attended.lock().unwrap();
        let request = session::build_pc_request(
            &self.engine.lock().unwrap(),
            &self.mesh.lock().unwrap(),
            attended,
            other,
            peer.pulled_upto,
            peer.pushed_upto,
            now,
        )
        .unwrap();
        let response = client.sync(&request).await.expect("sync should succeed");
        let mut engine = self.engine.lock().unwrap();
        let cursors = session::apply_response(&mut engine, &request, &response, now).unwrap();
        session::apply_mesh(&engine, &mut self.mesh.lock().unwrap(), &response, now);
        let mut peers = self.peers.lock().unwrap();
        let stored = peers.get_mut(other).unwrap();
        stored.pulled_upto = stored.pulled_upto.max(cursors.pulled_upto);
        stored.pushed_upto = stored.pushed_upto.max(cursors.pushed_upto);
        response
    }
}

#[tokio::test]
async fn two_pcs_pair_once_and_then_either_can_start_a_sync() {
    let t0 = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &UTC);
    let desk = Pc::named("desk", t0);
    let desk_id = Identity::generate().unwrap();
    let desk_server = Server::start(desk.clone(), &desk_id, 0).unwrap();
    let laptop = Pc::named("laptop", t0);
    let laptop_id = Identity::generate().unwrap();
    let laptop_server = Server::start(laptop.clone(), &laptop_id, 0).unwrap();

    // The laptop joins the desk's invite, leaving a way back.
    let back_token = pairing::new_token();
    let mut request = pairing::PairingInvite {
        pc_device_id: "desk".into(),
        pc_name: "Desk PC".into(),
        port: desk_server.port(),
        addrs: vec!["127.0.0.1".into()],
        fingerprint: desk_id.fingerprint(),
        code: desk.code(),
    }
    .request("laptop", "Laptop");
    request.back = Some(PairBack {
        token: back_token.clone(),
        fingerprint: laptop_id.fingerprint(),
        addrs: vec!["127.0.0.1".into()],
        port: laptop_server.port(),
    });
    let response = client::pair(
        &desk_id.fingerprint(),
        "127.0.0.1",
        desk_server.port(),
        &request,
    )
    .await
    .unwrap();
    let mut desk_on_laptop = Peer::new("desk", "Desk PC", t0);
    desk_on_laptop.token = Some(response.token);
    desk_on_laptop.token_hash = Some(pairing::token_hash(&back_token));
    desk_on_laptop.fingerprint = Some(desk_id.fingerprint());
    desk_on_laptop.addrs = vec!["127.0.0.1".into()];
    desk_on_laptop.port = desk_server.port();
    laptop.peers.lock().unwrap().upsert(desk_on_laptop);

    let on_desk = desk
        .engine
        .lock()
        .unwrap()
        .create_item(t0, reminder("From the desk", t0.plus(MINUTE)))
        .unwrap();
    let on_laptop = laptop
        .engine
        .lock()
        .unwrap()
        .create_item(t0, reminder("From the laptop", t0.plus(MINUTE)))
        .unwrap();

    // The desk starts this one, which it could not do with a phone.
    *desk.attended.lock().unwrap() = true;
    desk.sync_with("laptop", t0.plus(SECOND)).await;
    assert!(laptop
        .engine
        .lock()
        .unwrap()
        .state()
        .items
        .contains_key(&on_desk));
    assert!(desk
        .engine
        .lock()
        .unwrap()
        .state()
        .items
        .contains_key(&on_laptop));
    assert_eq!(
        laptop.mesh.lock().unwrap().peer_attended_at(),
        Some(t0.plus(SECOND)),
        "the laptop knows someone is at the desk"
    );

    // And the other way round, with nothing left to send.
    let quiet = laptop.sync_with("desk", t0.plus(2 * SECOND)).await;
    assert!(quiet.pull.registers.is_empty() && quiet.pull.history.is_empty());
}
