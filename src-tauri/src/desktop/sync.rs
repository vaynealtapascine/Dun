//! Sync on the PC: the server's lifecycle, pairing, the bridge between an
//! HTTPS request and the engine, and the loop that keeps this PC in step with
//! the other PCs it is paired with.
//!
//! Every PC listens; a PC paired with another PC also syncs with it as a
//! client, so either can start an exchange and rows reach PCs that are only
//! paired through a third. Phones only ever connect in.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dun_core::sync::mesh::Mesh;
use dun_core::sync::peers::Peer;
use dun_core::sync::protocol::{
    PairBack, PairRequest, PairResponse, SyncRequest, SyncResponse, PORT,
};
use dun_core::sync::session::{self, ServerView};
use dun_core::time::Timestamp;
use dun_sync::cert::Identity;
use dun_sync::client::SyncClient;
use dun_sync::pairing::{self, PairError, Pairing, PairingInvite};
use dun_sync::server::{Backend, Refusal, Server};

use super::updates::Updates;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

use crate::app_core::AppCore;

/// Device-local setting: whether the server should be listening.
const ENABLED_KEY: &str = "syncEnabled";

/// How often this PC syncs with each paired PC when nothing has changed, so
/// presence stays fresh (see `PRESENCE_FRESH_MS`).
const PC_SYNC_EVERY: Duration = Duration::from_secs(10);
/// How long to leave a PC that didn't answer before trying it again.
const PC_RETRY_AFTER: Duration = Duration::from_secs(30);
/// Pages taken from one PC in one go before giving the others a turn.
const MAX_PAGES: usize = 20;

pub struct SyncHub<R: Runtime> {
    app: AppHandle<R>,
    core: Arc<AppCore>,
    identity: Identity,
    server: Mutex<Option<Server>>,
    pairing: Mutex<Option<Pairing>>,
    /// Handoff acknowledgements and other PCs' presence, read by the
    /// scheduler loop.
    mesh: Arc<Mutex<Mesh>>,
    attended: AtomicBool,
    /// Mirrors how many peers are stored, so the scheduler can ask without
    /// taking the engine lock it already holds.
    paired_count: AtomicUsize,
    /// The Android build, if any, waiting in the `updates` folder.
    updates: Updates,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerView {
    pub device_id: String,
    pub name: String,
    /// A PC this one syncs with in both directions, rather than a phone.
    pub is_pc: bool,
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
    /// Connected networks Windows has marked Public, where it blocks the phone
    /// however well everything else is set up.
    pub public_networks: Vec<String>,
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
            mesh: Arc::new(Mutex::new(Mesh::default())),
            attended: AtomicBool::new(true),
            paired_count: AtomicUsize::new(0),
            updates: Updates::default(),
        });
        hub.refresh_paired_count();
        Ok(hub)
    }

    pub fn mesh(&self) -> Arc<Mutex<Mesh>> {
        self.mesh.clone()
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
        self.mesh
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .forget(device_id);
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
                    is_pc: is_pc(p),
                    paired_at: p.paired_at,
                    last_seen_at: p.last_seen_at,
                })
                .collect(),
            pairing: pairing.as_ref().and_then(|p| self.view_of(p).ok()),
            public_networks: if self.listening() {
                super::network::public_networks()
            } else {
                Vec::new()
            },
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

    /// The package this PC is holding for phones, if there is one.
    fn offered(&self) -> Option<super::updates::Available> {
        self.updates.current(&self.core.data_dir().join("updates"))
    }

    fn after_merge(&self) {
        self.core.wake_scheduler();
        let _ = self
            .app
            .emit(crate::commands::STATE_CHANGED, self.core.snapshot_json());
    }

    /// Joins another PC from its pairing invite. This PC sends a token of its
    /// own along, so from then on either PC can start a sync.
    pub fn join(self: &Arc<Self>, invite: &str) -> Result<String, String> {
        let invite =
            PairingInvite::parse(invite).ok_or("That doesn't look like a Dun pairing code")?;
        if !self.enabled() {
            return Err("Turn on sync first, so the other PC can reach this one too".into());
        }
        let device_id = self.core.engine().device_id().to_string();
        if invite.pc_device_id == device_id {
            return Err("That's this PC's own pairing code".into());
        }
        self.start()?;

        let back_token = pairing::new_token();
        let mut request = invite.request(&device_id, &pc_name());
        request.back = Some(PairBack {
            token: back_token.clone(),
            fingerprint: self.identity.fingerprint(),
            addrs: local_addrs(),
            port: PORT,
        });

        let runtime = client_runtime().map_err(|e| e.to_string())?;
        let mut last_error = "no addresses to try".to_string();
        let mut paired = None;
        for addr in &invite.addrs {
            let attempt = runtime.block_on(dun_sync::client::pair(
                &invite.fingerprint,
                addr,
                invite.port,
                &request,
            ));
            match attempt {
                Ok(response) => {
                    paired = Some((addr.clone(), response));
                    break;
                }
                Err(e) => last_error = e.to_string(),
            }
        }
        let (addr, response) = paired.ok_or(last_error)?;

        let now = self.core.now();
        let mut engine = self.core.engine();
        let mut peers = engine.peers();
        let mut peer = Peer::new(&response.pc_device_id, &response.pc_name, now);
        peer.token = Some(response.token);
        peer.token_hash = Some(pairing::token_hash(&back_token));
        peer.fingerprint = Some(invite.fingerprint);
        peer.addrs = invite.addrs;
        peer.port = invite.port;
        peer.last_ok_addr = Some(addr);
        peers.upsert(peer);
        engine.save_peers(&peers).map_err(|e| e.to_string())?;
        drop(engine);

        self.refresh_paired_count();
        self.notify();
        Ok(response.pc_name)
    }

    /// Starts the loop that syncs with every paired PC.
    pub fn spawn_pc_sync(self: &Arc<Self>) {
        let hub = self.clone();
        let spawned = std::thread::Builder::new()
            .name("dun-pc-sync".into())
            .spawn(move || hub.run_pc_sync());
        if let Err(e) = spawned {
            eprintln!("sync: couldn't start syncing with other PCs: {e}");
        }
    }

    fn run_pc_sync(&self) {
        let runtime = match client_runtime() {
            Ok(runtime) => runtime,
            Err(e) => {
                eprintln!("sync: no runtime for syncing with other PCs: {e}");
                return;
            }
        };

        #[derive(Default)]
        struct Round {
            next_at: Option<Instant>,
            /// The local `seq` at the last successful sync. Compared with
            /// the store rather than the peer's cursor, which can stay just
            /// behind it for good when the newest rows came from that peer.
            synced_seq: i64,
            sent_attended: Option<bool>,
            failing: bool,
        }
        let mut rounds: HashMap<String, Round> = HashMap::new();

        loop {
            std::thread::sleep(Duration::from_secs(1));
            if !self.listening() {
                continue;
            }
            let attended = self.attended.load(Ordering::Relaxed);
            let (pcs, seq) = {
                let engine = self.core.engine();
                let pcs: Vec<Peer> = engine
                    .peers()
                    .iter()
                    .filter(|p| is_pc(p))
                    .cloned()
                    .collect();
                (pcs, engine.store().seq())
            };
            rounds.retain(|id, _| pcs.iter().any(|p| &p.device_id == id));

            for peer in pcs {
                let round = rounds.entry(peer.device_id.clone()).or_default();
                let now = Instant::now();
                let scheduled = round.next_at.is_none_or(|at| now >= at);
                // Changes, and someone arriving or leaving, go out straight
                // away, unless the PC has stopped answering.
                let urgent = !round.failing
                    && (seq > round.synced_seq || round.sent_attended != Some(attended));
                if !(scheduled || urgent) {
                    continue;
                }

                match self.sync_with_pc(&runtime, &peer.device_id, attended) {
                    Ok(()) => {
                        if round.failing {
                            eprintln!("sync: {} is reachable again", peer.name);
                        }
                        round.failing = false;
                        round.synced_seq = seq;
                        round.sent_attended = Some(attended);
                        round.next_at = Some(now + PC_SYNC_EVERY);
                    }
                    Err(e) => {
                        if !round.failing {
                            eprintln!("sync: couldn't sync with {}: {e}", peer.name);
                        }
                        round.failing = true;
                        round.next_at = Some(now + PC_RETRY_AFTER);
                    }
                }
            }
        }
    }

    /// One full exchange with another PC, page after page until it has
    /// nothing more. The engine lock is never held across the network.
    fn sync_with_pc(
        &self,
        runtime: &tokio::runtime::Runtime,
        device_id: &str,
        attended: bool,
    ) -> Result<(), String> {
        for _ in 0..MAX_PAGES {
            // Lock order everywhere: engine first, then mesh.
            let (peer, request) = {
                let engine = self.core.engine();
                let peer = engine
                    .peers()
                    .get(device_id)
                    .cloned()
                    .ok_or("no longer paired")?;
                let mesh = self.mesh.lock().unwrap_or_else(|p| p.into_inner());
                let request = session::build_pc_request(
                    &engine,
                    &mesh,
                    attended,
                    device_id,
                    peer.pulled_upto,
                    peer.pushed_upto,
                    self.core.now(),
                )
                .map_err(|e| e.to_string())?;
                (peer, request)
            };
            let (Some(token), Some(fingerprint)) = (peer.token.clone(), peer.fingerprint.clone())
            else {
                return Err("paired one way only".into());
            };

            let mut addrs = peer.addrs.clone();
            if let Some(last) = peer.last_ok_addr.clone() {
                addrs.retain(|a| *a != last);
                addrs.insert(0, last);
            }
            let client = SyncClient::new(&fingerprint, token, addrs, peer.port)
                .map_err(|e| e.to_string())?;
            let response = runtime
                .block_on(client.sync(&request))
                .map_err(|e| e.to_string())?;

            let now = self.core.now();
            let mut engine = self.core.engine();
            let cursors = session::apply_response(&mut engine, &request, &response, now)
                .map_err(|e| e.to_string())?;
            session::apply_mesh(
                &engine,
                &mut self.mesh.lock().unwrap_or_else(|p| p.into_inner()),
                &response,
                now,
            );
            let mut peers = engine.peers();
            if let Some(stored) = peers.get_mut(device_id) {
                // An exchange the other way may have moved these on while
                // this request was in flight.
                stored.pulled_upto = stored.pulled_upto.max(cursors.pulled_upto);
                stored.pushed_upto = stored.pushed_upto.max(cursors.pushed_upto);
                stored.last_seen_at = Some(now);
                stored.last_ok_addr = client.last_ok_addr();
                if !response.addrs.is_empty() {
                    stored.addrs = response.addrs.clone();
                }
            }
            let _ = engine.save_peers(&peers);
            drop(engine);

            if !response.pull.registers.is_empty() || !response.pull.history.is_empty() {
                self.after_merge();
            }
            if !cursors.more {
                return Ok(());
            }
        }
        Ok(())
    }
}

/// A PC this one can sync with, as opposed to a phone that only connects in.
fn is_pc(peer: &Peer) -> bool {
    peer.token.is_some() && peer.fingerprint.is_some()
}

fn client_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
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
        // One code, one device: close the dialog as soon as it's used.
        *open = None;
        drop(open);

        let mut engine = self.core.engine();
        let mut peers = engine.peers();
        let mut peer = Peer::new(&paired.device_id, &paired.name, now);
        peer.token_hash = Some(paired.token_hash);
        peer.last_seen_at = Some(now);
        // Another PC: keep what it sent, so this PC can sync with it in turn.
        if let Some(back) = paired.back {
            peer.token = Some(back.token);
            peer.fingerprint = Some(back.fingerprint);
            peer.addrs = back.addrs;
            peer.port = back.port;
        }
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
            update: self.offered().map(|a| a.offer),
        };
        let mut mesh = self.mesh.lock().unwrap_or_else(|p| p.into_inner());
        let response = session::handle_sync(&mut engine, &request, now, &view, &mut mesh)
            .map_err(|e| Refusal::new(400, e.body()))?;
        drop(mesh);

        let mut peers = engine.peers();
        if let Some(peer) = peers.get_mut(&peer_id) {
            peer.note_served(&request, now);
        }
        let _ = engine.save_peers(&peers);
        drop(engine);

        // A Done elsewhere should stop this PC nagging within seconds.
        self.after_merge();
        Ok(response)
    }

    fn apk(&self, token: &str) -> Result<std::path::PathBuf, Refusal> {
        // Paired phones only: the package is served over the same trust the
        // rest of sync runs on, not to anything that can reach the port.
        let peers = self.core.engine().peers();
        if peers.by_token(token, pairing::token_matches).is_none() {
            return Err(Refusal::unauthorized());
        }
        self.offered().map(|a| a.path).ok_or_else(|| {
            Refusal::new(
                404,
                dun_core::sync::protocol::ErrorBody::new(
                    dun_core::sync::protocol::error::NOT_FOUND,
                    "This PC has no Android build to hand out",
                ),
            )
        })
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
