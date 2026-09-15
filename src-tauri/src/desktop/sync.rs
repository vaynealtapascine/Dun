//! Sync on the PC: the server's lifecycle, pairing, and the bridge between
//! an HTTPS request and the engine.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use dun_core::scheduler::Ack;
use dun_core::sync::peers::Peer;
use dun_core::sync::protocol::{PairRequest, PairResponse, SyncRequest, SyncResponse, PORT};
use dun_core::sync::session::{self, ServerView};
use dun_core::time::Timestamp;
use dun_sync::cert::Identity;
use dun_sync::pairing::{self, PairError, Pairing, PairingInvite};
use dun_sync::server::{Backend, Refusal, Server};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

use crate::app_core::AppCore;

/// Device-local setting: whether the server should be listening.
const ENABLED_KEY: &str = "syncEnabled";

pub struct SyncHub<R: Runtime> {
    app: AppHandle<R>,
    core: Arc<AppCore>,
    identity: Identity,
    server: Mutex<Option<Server>>,
    pairing: Mutex<Option<Pairing>>,
    /// Handoff acknowledgements from the phone, read by the scheduler loop.
    acks: Arc<Mutex<BTreeMap<String, Ack>>>,
    attended: AtomicBool,
    /// Mirrors how many peers are stored, so the scheduler can ask without
    /// taking the engine lock it already holds.
    paired_count: AtomicUsize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerView {
    pub device_id: String,
    pub name: String,
    pub paired_at: Timestamp,
    pub last_seen_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub enabled: bool,
    pub listening: bool,
    pub port: u16,
    pub addrs: Vec<String>,
    pub fingerprint: String,
    pub peers: Vec<PeerView>,
    /// Set while the Pairing dialog is open.
    pub pairing: Option<PairingView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingView {
    pub code: String,
    pub expires_at: Timestamp,
    pub tries_left: u8,
    /// The text behind the QR code, for pasting.
    pub uri: String,
    pub qr_svg: String,
}

impl<R: Runtime> SyncHub<R> {
    pub fn new(app: AppHandle<R>, core: Arc<AppCore>) -> Result<Arc<Self>, String> {
        let identity =
            Identity::load_or_create(&core.data_dir().join("sync")).map_err(|e| e.to_string())?;
        let hub = Arc::new(SyncHub {
            app,
            core,
            identity,
            server: Mutex::new(None),
            pairing: Mutex::new(None),
            acks: Arc::new(Mutex::new(BTreeMap::new())),
            attended: AtomicBool::new(true),
            paired_count: AtomicUsize::new(0),
        });
        hub.refresh_paired_count();
        Ok(hub)
    }

    pub fn acks(&self) -> Arc<Mutex<BTreeMap<String, Ack>>> {
        self.acks.clone()
    }

    pub fn set_attended(&self, attended: bool) {
        self.attended.store(attended, Ordering::Relaxed);
    }

    /// Whether anything is paired. Called with the engine lock already held by
    /// the scheduler, so it must not take it again.
    pub fn peers_empty(&self) -> bool {
        self.paired_count.load(Ordering::Relaxed) == 0
    }

    fn refresh_paired_count(&self) {
        let count = self.core.engine().peers().iter().count();
        self.paired_count.store(count, Ordering::Relaxed);
    }

    pub fn enabled(&self) -> bool {
        self.core
            .engine()
            .store()
            .local_get::<bool>(ENABLED_KEY)
            .ok()
            .flatten()
            .unwrap_or(false)
    }

    fn set_enabled_setting(&self, enabled: bool) -> Result<(), String> {
        self.core
            .engine()
            .store_mut()
            .local_set(ENABLED_KEY, &enabled)
            .map_err(|e| e.to_string())
    }

    pub fn listening(&self) -> bool {
        self.server
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_some()
    }

    /// Starts listening (idempotent). The firewall rule is the caller's job.
    pub fn start(self: &Arc<Self>) -> Result<u16, String> {
        let mut server = self.server.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(running) = server.as_ref() {
            return Ok(running.port());
        }
        let started = Server::start(self.clone(), &self.identity, PORT)
            .map_err(|e| format!("Couldn't listen on port {PORT}: {e}"))?;
        let port = started.port();
        *server = Some(started);
        Ok(port)
    }

    pub fn stop(&self) {
        *self.server.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }

    /// Turns sync on or off. Enabling adds the firewall rule first (one UAC
    /// prompt); skip_firewall is the "enable anyway" path after a refusal.
    pub fn set_enabled(self: &Arc<Self>, enabled: bool, skip_firewall: bool) -> Result<(), String> {
        if enabled && !skip_firewall {
            super::firewall::ensure(PORT)?;
        }
        self.set_enabled_setting(enabled)?;
        if enabled {
            self.start()?;
        } else {
            self.stop();
            self.close_pairing();
        }
        self.notify();
        Ok(())
    }

    /// Opens the Pairing dialog: a fresh code, good for five minutes.
    pub fn open_pairing(self: &Arc<Self>) -> Result<PairingView, String> {
        self.start()?;
        let pairing = Pairing::open(self.core.now());
        let view = self.view_of(&pairing);
        *self.pairing.lock().unwrap_or_else(|p| p.into_inner()) = Some(pairing);
        self.notify();
        view
    }

    pub fn close_pairing(&self) {
        *self.pairing.lock().unwrap_or_else(|p| p.into_inner()) = None;
        self.notify();
    }

    pub fn forget_peer(&self, device_id: &str) -> Result<(), String> {
        let mut engine = self.core.engine();
        let mut peers = engine.peers();
        if peers.remove(device_id) {
            engine.save_peers(&peers).map_err(|e| e.to_string())?;
        }
        drop(engine);
        self.refresh_paired_count();
        self.notify();
        Ok(())
    }

    pub fn status(&self) -> SyncStatus {
        let peers = self.core.engine().peers();
        let pairing = self.pairing.lock().unwrap_or_else(|p| p.into_inner());
        SyncStatus {
            enabled: self.enabled(),
            listening: self.listening(),
            port: PORT,
            addrs: local_addrs(),
            fingerprint: self.identity.fingerprint(),
            peers: peers
                .iter()
                .map(|p| PeerView {
                    device_id: p.device_id.clone(),
                    name: p.name.clone(),
                    paired_at: p.paired_at,
                    last_seen_at: p.last_seen_at,
                })
                .collect(),
            pairing: pairing.as_ref().and_then(|p| self.view_of(p).ok()),
        }
    }

    fn view_of(&self, pairing: &Pairing) -> Result<PairingView, String> {
        let invite = PairingInvite {
            pc_device_id: self.core.engine().device_id().to_string(),
            pc_name: pc_name(),
            port: PORT,
            addrs: local_addrs(),
            fingerprint: self.identity.fingerprint(),
            code: pairing.code().to_string(),
        };
        let uri = invite.to_uri();
        Ok(PairingView {
            code: pairing.code().to_string(),
            expires_at: pairing.expires_at(),
            tries_left: pairing.tries_left(),
            qr_svg: qr_svg(&uri)?,
            uri,
        })
    }

    /// Tells the UI (and the scheduler) that something changed.
    fn notify(&self) {
        let _ = self.app.emit("sync-changed", self.status());
    }

    fn after_merge(&self) {
        self.core.wake_scheduler();
        let _ = self
            .app
            .emit(crate::commands::STATE_CHANGED, self.core.snapshot_json());
    }
}

impl<R: Runtime> Backend for SyncHub<R> {
    fn pair(&self, request: PairRequest) -> Result<PairResponse, Refusal> {
        let now = self.core.now();
        let mut open = self.pairing.lock().unwrap_or_else(|p| p.into_inner());
        let Some(pairing) = open.as_mut() else {
            return Err(Refusal::new(403, PairError::NotPairing.body()));
        };

        let pc_device_id = self.core.engine().device_id().to_string();
        let (response, paired) = pairing::accept(pairing, &request, now, &pc_device_id, &pc_name())
            .map_err(|e| Refusal::new(403, e.body()))?;
        // One code, one phone: close the dialog as soon as it's used.
        *open = None;
        drop(open);

        let mut engine = self.core.engine();
        let mut peers = engine.peers();
        let mut peer = Peer::new(&paired.device_id, &paired.name, now);
        peer.token_hash = Some(paired.token_hash);
        peer.last_seen_at = Some(now);
        peers.upsert(peer);
        engine.save_peers(&peers).map_err(|e| {
            Refusal::new(
                500,
                dun_core::sync::protocol::ErrorBody::new("storeFailed", e.to_string()),
            )
        })?;
        drop(engine);

        self.refresh_paired_count();
        self.notify();
        Ok(response)
    }

    fn sync(&self, token: &str, request: SyncRequest) -> Result<SyncResponse, Refusal> {
        let (now, tz) = (self.core.now(), self.core.tz());
        let mut engine = self.core.engine();
        let peers = engine.peers();
        let Some(peer) = peers.by_token(token, pairing::token_matches) else {
            return Err(Refusal::unauthorized());
        };
        let peer_id = peer.device_id.clone();

        let view = ServerView {
            attended: self.attended.load(Ordering::Relaxed),
            addrs: local_addrs(),
            tz: &tz,
        };
        let mut acks = self.acks.lock().unwrap_or_else(|p| p.into_inner());
        let response = session::handle_sync(&mut engine, &request, now, &view, &mut acks)
            .map_err(|e| Refusal::new(400, e.body()))?;
        drop(acks);

        let mut peers = engine.peers();
        if let Some(peer) = peers.get_mut(&peer_id) {
            peer.pulled_upto = response.pull.new_pulled_upto.max(peer.pulled_upto);
            peer.pushed_upto = request.push.max_seq.max(peer.pushed_upto);
            peer.last_seen_at = Some(now);
        }
        let _ = engine.save_peers(&peers);
        drop(engine);

        // A phone's Done should stop this PC nagging within seconds.
        self.after_merge();
        Ok(response)
    }
}

/// This PC's addresses, best first: private LAN, then Tailscale-style, then
/// anything else routable.
pub fn local_addrs() -> Vec<String> {
    let mut addrs: Vec<(u8, String)> = Vec::new();
    for iface in if_addrs::get_if_addrs().unwrap_or_default() {
        if iface.is_loopback() {
            continue;
        }
        let ip = iface.ip();
        let rank = match ip {
            std::net::IpAddr::V4(v4) if v4.is_private() => 0,
            std::net::IpAddr::V4(v4)
                if v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1]) =>
            {
                1
            }
            std::net::IpAddr::V4(_) => 2,
            // Link-local IPv6 needs a scope id to be useful; skip it.
            std::net::IpAddr::V6(v6) if v6.segments()[0] & 0xffc0 == 0xfe80 => continue,
            std::net::IpAddr::V6(_) => 3,
        };
        addrs.push((rank, ip.to_string()));
    }
    addrs.sort();
    addrs.dedup();
    addrs.into_iter().map(|(_, ip)| ip).collect()
}

fn pc_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "This PC".to_string())
}

/// The invite as a scannable SVG.
fn qr_svg(text: &str) -> Result<String, String> {
    use qrcode::render::svg;
    let code = qrcode::QrCode::new(text.as_bytes())
        .map_err(|e| format!("couldn't make a QR code: {e}"))?;
    let svg = code
        .render::<svg::Color<'_>>()
        .min_dimensions(240, 240)
        .quiet_zone(true)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build();
    // Drop the XML declaration so the markup can be inlined in the page.
    Ok(svg
        .split_once("?>")
        .map_or(svg.clone(), |(_, rest)| rest.trim_start().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_addresses_are_usable_and_ordered() {
        let addrs = local_addrs();
        for addr in &addrs {
            assert!(
                !addr.starts_with("127."),
                "loopback shouldn't be advertised"
            );
            assert!(
                !addr.starts_with("fe80"),
                "link-local IPv6 needs a scope id"
            );
            assert!(
                addr.parse::<std::net::IpAddr>().is_ok(),
                "{addr} should parse"
            );
        }
    }

    #[test]
    fn the_invite_renders_as_a_qr_code() {
        let svg =
            qr_svg("dun://pair?v=1&id=abc&n=PC&p=47823&a=192.168.1.10&fp=aa&c=123456").unwrap();
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("</svg>"));
    }
}
