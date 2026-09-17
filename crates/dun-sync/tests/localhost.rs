//! The client and server talking to each other over real TLS on localhost.

use std::sync::{Arc, Mutex};

use dun_core::sync::protocol::{error, PairRequest, Pull, SyncRequest, SyncResponse};
use dun_core::time::Timestamp;
use dun_sync::cert::Identity;
use dun_sync::client::{self, ClientError, SyncClient};
use dun_sync::pairing::{self, PairError, Pairing};
use dun_sync::server::{Backend, Refusal, Server};

const NOW: Timestamp = Timestamp(1_789_500_000_000);

/// A PC stand-in: real pairing and token checks, canned sync answers.
struct TestPc {
    pairing: Mutex<Option<Pairing>>,
    peers: Mutex<Vec<pairing::PairedPeer>>,
    last_request: Mutex<Option<SyncRequest>>,
    /// A package to hand out, as the real PC does from its updates folder.
    apk: Mutex<Option<std::path::PathBuf>>,
}

impl TestPc {
    fn new(open_pairing: bool) -> Arc<TestPc> {
        Arc::new(TestPc {
            pairing: Mutex::new(open_pairing.then(|| Pairing::open(NOW))),
            peers: Mutex::new(Vec::new()),
            last_request: Mutex::new(None),
            apk: Mutex::new(None),
        })
    }

    fn knows(&self, token: &str) -> bool {
        self.peers
            .lock()
            .unwrap()
            .iter()
            .any(|p| pairing::token_matches(token, &p.token_hash))
    }

    fn code(&self) -> String {
        self.pairing
            .lock()
            .unwrap()
            .as_ref()
            .expect("pairing open")
            .code()
            .to_string()
    }
}

impl Backend for TestPc {
    fn pair(
        &self,
        request: PairRequest,
    ) -> Result<dun_core::sync::protocol::PairResponse, Refusal> {
        let mut open = self.pairing.lock().unwrap();
        let Some(pairing) = open.as_mut() else {
            return Err(Refusal::new(403, PairError::NotPairing.body()));
        };
        match pairing::accept(pairing, &request, NOW, "pc-device", "Desk PC") {
            Ok((response, peer)) => {
                self.peers.lock().unwrap().push(peer);
                Ok(response)
            }
            Err(e) => Err(Refusal::new(403, e.body())),
        }
    }

    fn apk(&self, token: &str) -> Result<std::path::PathBuf, Refusal> {
        if !self.knows(token) {
            return Err(Refusal::unauthorized());
        }
        self.apk
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| Refusal::new(404, PairError::NotPairing.body()))
    }

    fn sync(&self, token: &str, request: SyncRequest) -> Result<SyncResponse, Refusal> {
        if !self.knows(token) {
            return Err(Refusal::unauthorized());
        }
        *self.last_request.lock().unwrap() = Some(request.clone());
        Ok(SyncResponse {
            pc_now: NOW,
            acked_upto: request.push.max_seq,
            pull: Pull::default(),
            attended: true,
            ringing_soon: vec![],
            addrs: vec!["127.0.0.1".into()],
            skew_warning: None,
            update: None,
        })
    }
}

fn request() -> SyncRequest {
    SyncRequest {
        device_id: "phone".into(),
        now: NOW,
        pulled_upto: 0,
        push: Default::default(),
        due_items: vec![],
        app_ver: "test".into(),
        schema_ver: 1,
    }
}

struct Running {
    pc: Arc<TestPc>,
    identity: Identity,
    server: Server,
}

fn start(open_pairing: bool) -> Running {
    let pc = TestPc::new(open_pairing);
    let identity = Identity::generate().unwrap();
    let server = Server::start(pc.clone(), &identity, 0).unwrap();
    Running {
        pc,
        identity,
        server,
    }
}

async fn pair_ok(running: &Running) -> String {
    client::pair(
        &running.identity.fingerprint(),
        "127.0.0.1",
        running.server.port(),
        &PairRequest {
            code: running.pc.code(),
            device_id: "phone".into(),
            name: "Pixel".into(),
        },
    )
    .await
    .expect("pairing should succeed")
    .token
}

#[tokio::test]
async fn pairs_then_syncs_over_pinned_tls() {
    let running = start(true);
    let token = pair_ok(&running).await;

    let client = SyncClient::new(
        &running.identity.fingerprint(),
        token,
        vec!["127.0.0.1".into()],
        running.server.port(),
    )
    .unwrap();
    let response = client.sync(&request()).await.unwrap();

    assert!(response.attended);
    assert_eq!(
        running
            .pc
            .last_request
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .device_id,
        "phone"
    );
    assert_eq!(client.last_ok_addr().as_deref(), Some("127.0.0.1"));
}

#[tokio::test]
async fn a_wrong_code_is_refused_and_the_right_one_works_only_once() {
    let running = start(true);
    let wrong = client::pair(
        &running.identity.fingerprint(),
        "127.0.0.1",
        running.server.port(),
        &PairRequest {
            code: "000000".into(),
            device_id: "phone".into(),
            name: "Pixel".into(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(wrong.code(), Some(error::BAD_CODE));

    let code = running.pc.code();
    let request = PairRequest {
        code,
        device_id: "phone".into(),
        name: "Pixel".into(),
    };
    assert!(client::pair(
        &running.identity.fingerprint(),
        "127.0.0.1",
        running.server.port(),
        &request
    )
    .await
    .is_ok());

    // Replaying the same code gets nothing.
    let replay = client::pair(
        &running.identity.fingerprint(),
        "127.0.0.1",
        running.server.port(),
        &request,
    )
    .await
    .unwrap_err();
    assert_eq!(replay.code(), Some(error::BAD_CODE));
}

#[tokio::test]
async fn pairing_is_closed_unless_the_dialog_is_open() {
    let running = start(false);
    let refused = client::pair(
        &running.identity.fingerprint(),
        "127.0.0.1",
        running.server.port(),
        &PairRequest {
            code: "123456".into(),
            device_id: "phone".into(),
            name: "Pixel".into(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(refused.code(), Some(error::NOT_PAIRING));
}

#[tokio::test]
async fn an_unknown_token_is_unauthorized() {
    let running = start(true);
    pair_ok(&running).await;

    let client = SyncClient::new(
        &running.identity.fingerprint(),
        "not-the-token".into(),
        vec!["127.0.0.1".into()],
        running.server.port(),
    )
    .unwrap();
    let error = client.sync(&request()).await.unwrap_err();
    assert_eq!(error.code(), Some(error::UNAUTHORIZED));
}

#[tokio::test]
async fn a_different_certificate_is_refused() {
    let running = start(true);
    let token = pair_ok(&running).await;
    let impostor = Identity::generate().unwrap().fingerprint();

    let client = SyncClient::new(
        &impostor,
        token,
        vec!["127.0.0.1".into()],
        running.server.port(),
    )
    .unwrap();
    let error = client.sync(&request()).await.unwrap_err();
    assert!(
        matches!(error, ClientError::Unreachable { .. }),
        "a pin mismatch must fail the handshake, got {error:?}"
    );
    assert_eq!(error.code(), None, "the PC never got to answer");
}

#[tokio::test]
async fn a_dead_address_falls_through_to_a_live_one() {
    let running = start(true);
    let token = pair_ok(&running).await;

    // 192.0.2.1 is reserved for documentation and never answers.
    let client = SyncClient::new(
        &running.identity.fingerprint(),
        token,
        vec!["192.0.2.1".into(), "127.0.0.1".into()],
        running.server.port(),
    )
    .unwrap();
    let response = client.sync(&request()).await.unwrap();
    assert!(response.attended);
    assert_eq!(client.last_ok_addr().as_deref(), Some("127.0.0.1"));

    // The next sync tries the address that worked first.
    assert_eq!(
        client.candidates().first().map(String::as_str),
        Some("127.0.0.1")
    );
}

/// The desktop starts the server from its setup thread, where no runtime is
/// running — unlike every test above, which tokio has already entered. That
/// difference used to panic ("no reactor running") on the real app's first
/// start, so this proves a bare thread can start it *and* be reached.
#[test]
fn starts_and_serves_without_a_runtime_in_the_caller() {
    let running = start(true);
    let port = running.server.port();
    assert_ne!(port, 0, "the server should have picked a port");

    let paired = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(pair_ok(&running));
    assert!(!paired.is_empty(), "the server should have answered");
}

/// The package itself: fetched over the same pinned connection, and refused
/// unless it arrives exactly as promised.
mod updates {
    use super::*;

    fn sha256(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    #[tokio::test]
    async fn a_paired_phone_downloads_the_package_it_was_promised() {
        let running = start(true);
        let token = pair_ok(&running).await;

        let dir = tempfile::tempdir().unwrap();
        let served = dir.path().join("dun-0.2.0.apk");
        let bytes = b"not really an APK, but the bytes are the bytes".repeat(500);
        std::fs::write(&served, &bytes).unwrap();
        *running.pc.apk.lock().unwrap() = Some(served);

        let into = dir.path().join("downloaded.apk");
        let written = client::download_apk(
            &running.identity.fingerprint(),
            "127.0.0.1",
            running.server.port(),
            &token,
            &sha256(&bytes),
            &into,
        )
        .await
        .expect("the download should succeed");

        assert_eq!(written as usize, bytes.len());
        assert_eq!(std::fs::read(&into).unwrap(), bytes);
    }

    #[tokio::test]
    async fn a_package_that_arrives_wrong_is_thrown_away() {
        let running = start(true);
        let token = pair_ok(&running).await;

        let dir = tempfile::tempdir().unwrap();
        let served = dir.path().join("dun-0.2.0.apk");
        std::fs::write(&served, b"the bytes the PC actually has").unwrap();
        *running.pc.apk.lock().unwrap() = Some(served);

        let into = dir.path().join("downloaded.apk");
        let error = client::download_apk(
            &running.identity.fingerprint(),
            "127.0.0.1",
            running.server.port(),
            &token,
            &sha256(b"what the PC said it had"),
            &into,
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("whole"), "got {error}");
        assert!(
            !into.exists(),
            "a package that failed its hash must not be left for the installer"
        );
        assert!(
            !into.with_extension("part").exists(),
            "and neither must the half-written one"
        );
    }

    #[tokio::test]
    async fn an_unknown_token_gets_nothing() {
        let running = start(true);
        pair_ok(&running).await;

        let error = client::download_apk(
            &running.identity.fingerprint(),
            "127.0.0.1",
            running.server.port(),
            "not-the-token",
            &sha256(b""),
            &tempfile::tempdir().unwrap().path().join("nope.apk"),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code(), Some(error::UNAUTHORIZED));
    }
}
