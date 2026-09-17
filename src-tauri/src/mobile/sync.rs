//! The phone's side of sync: one check-in, on its own runtime, with a hard
//! deadline.
//!
//! Every alert the phone is about to raise is announced first, so the PC can
//! stay quiet while the phone covers it — and anything done on the phone
//! reaches the PC in the same request. The engine lock is never held across
//! the network call, and one gate keeps concurrent receivers from stacking up
//! sessions.

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use dun_core::engine::Engine;
use dun_core::scheduler::PcView;
use dun_core::sync::peers::{Peer, Peers};
use dun_core::sync::protocol::DueItem;
use dun_core::sync::session::{self, Cursors};
use dun_core::time::{TimeZone, Timestamp};
use dun_sync::client::SyncClient;

/// Total budget for a check-in, comfortably inside a receiver's 10 s.
const BUDGET: Duration = Duration::from_millis(4_000);

/// Device-local setting: how far this phone's clock was from the PC's at the
/// last check-in, in ms, or null when they agreed.
pub const SKEW_KEY: &str = "lastSkewMs";

/// Device-local setting: the newer build the PC last said it was holding.
pub const UPDATE_KEY: &str = "updateOffer";

/// Serializes sessions: two receivers firing at once shouldn't both sync.
static GATE: Mutex<()> = Mutex::new(());

pub fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .thread_name("dun-sync")
            .build()
            .expect("sync runtime")
    })
}

/// What the check-in learned. `pc` is `None` when the PC couldn't be reached,
/// which makes the phone ring on its own.
#[derive(Debug, Default)]
pub struct CheckIn {
    pub pc: Option<PcView>,
    /// Local changes still waiting to be pushed.
    pub pending_push: bool,
    /// For logs: what happened, in a few words.
    pub note: String,
}

/// Whether there is anything worth a sync right now.
pub fn should_sync(engine: &Engine, peer: &Peer, due: &[DueItem]) -> bool {
    !due.is_empty() || has_unpushed(engine, peer)
}

fn has_unpushed(engine: &Engine, peer: &Peer) -> bool {
    engine.store().seq() > peer.pushed_upto
}

/// The paired PC, if this phone has one.
pub fn paired_pc(peers: &Peers) -> Option<Peer> {
    peers
        .iter()
        .find(|p| p.token.is_some() && p.fingerprint.is_some())
        .cloned()
}

/// Announces `due`, pushes local changes, merges whatever comes back, and
/// stores the new cursors. Never holds the engine lock across the network.
pub fn check_in(
    data_dir: &str,
    device_id: String,
    peer: Peer,
    due: Vec<DueItem>,
    now: Timestamp,
    tz: &TimeZone,
) -> CheckIn {
    let _gate = GATE.lock().unwrap_or_else(|p| p.into_inner());

    let (Some(token), Some(fingerprint)) = (peer.token.clone(), peer.fingerprint.clone()) else {
        return CheckIn {
            note: "not paired".into(),
            ..Default::default()
        };
    };

    // Build the request with the lock, release it for the round trip.
    let request = crate::mobile::core::with_engine(data_dir, |engine| {
        session::build_request(
            engine,
            &device_id,
            &peer.device_id,
            peer.pulled_upto,
            peer.pushed_upto,
            due.clone(),
            now,
        )
        .ok()
    })
    .ok()
    .flatten();
    let Some(request) = request else {
        return CheckIn {
            pending_push: true,
            note: "couldn't read local changes".into(),
            ..Default::default()
        };
    };

    let mut addrs = peer.addrs.clone();
    if let Some(last) = peer.last_ok_addr.clone() {
        addrs.retain(|a| *a != last);
        addrs.insert(0, last);
    }
    let client = match SyncClient::new(&fingerprint, token, addrs, peer.port) {
        Ok(client) => client,
        Err(e) => {
            return CheckIn {
                pending_push: true,
                note: format!("client: {e}"),
                ..Default::default()
            }
        }
    };

    let response = runtime().block_on(async {
        tokio::time::timeout(BUDGET, client.sync(&request))
            .await
            .unwrap_or_else(|_| {
                Err(dun_sync::client::ClientError::Unreachable {
                    tried: "timed out".into(),
                })
            })
    });

    let response = match response {
        Ok(response) => response,
        Err(e) => {
            return CheckIn {
                pending_push: true,
                note: format!("no PC: {e}"),
                ..Default::default()
            }
        }
    };

    let mut cursors = Cursors {
        pulled_upto: peer.pulled_upto,
        pushed_upto: peer.pushed_upto,
        more: false,
    };
    let _ = crate::mobile::core::with_engine(data_dir, |engine| {
        if let Ok(next) = session::apply_response(engine, &request, &response, now) {
            cursors = next;
        }
        let mut peers = engine.peers();
        if let Some(stored) = peers.get_mut(&peer.device_id) {
            stored.pulled_upto = cursors.pulled_upto;
            stored.pushed_upto = cursors.pushed_upto;
            stored.last_seen_at = Some(now);
            stored.last_ok_addr = client.last_ok_addr();
            if !response.addrs.is_empty() {
                stored.addrs = response.addrs.clone();
            }
        }
        let _ = engine.save_peers(&peers);
        // Kept so Settings can say so: clocks far apart mean reminders ring at
        // the wrong moment, and nothing else would ever explain that.
        let _ = engine
            .store_mut()
            .local_set(SKEW_KEY, &response.skew_warning);
        // Remembered rather than acted on: installing is the user's call, and
        // the offer has to survive until they open Settings and see it.
        let _ = engine.store_mut().local_set(UPDATE_KEY, &response.update);
    });

    let _ = tz;
    CheckIn {
        pc: Some(PcView {
            attended: response.attended,
            ringing: response
                .ringing_soon
                .iter()
                .map(|d| (d.item.clone(), d.occurrence))
                .collect(),
        }),
        pending_push: cursors.more,
        note: format!(
            "synced{}{}",
            if response.attended {
                ", PC attended"
            } else {
                ""
            },
            response
                .skew_warning
                .map(|s| format!(", clock off by {} s", s / 1000))
                .unwrap_or_default()
        ),
    }
}
