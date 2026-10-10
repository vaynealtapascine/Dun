//! Who this device is paired with.
//!
//! Peers are device-local (never synced): the PC keeps the hash of the token
//! it issued, the phone keeps the token itself plus the certificate it pins
//! and its sync cursors.

use serde::{Deserialize, Serialize};

use crate::time::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub device_id: String,
    pub name: String,
    pub paired_at: Timestamp,

    /// PC side: SHA-256 of the token given to this peer.
    #[serde(default)]
    pub token_hash: Option<String>,
    /// Phone side: the bearer token to send.
    #[serde(default)]
    pub token: Option<String>,
    /// Phone side: the certificate fingerprint to pin.
    #[serde(default)]
    pub fingerprint: Option<String>,
    #[serde(default)]
    pub addrs: Vec<String>,
    #[serde(default)]
    pub port: u16,

    /// Highest peer `seq` stored here.
    #[serde(default)]
    pub pulled_upto: i64,
    /// Highest local `seq` the peer has acknowledged.
    #[serde(default)]
    pub pushed_upto: i64,
    #[serde(default)]
    pub last_ok_addr: Option<String>,
    #[serde(default)]
    pub last_seen_at: Option<Timestamp>,
    /// Phone side: the last time this PC didn't answer, so a phone paired
    /// with several PCs tries another one next.
    #[serde(default)]
    pub last_failed_at: Option<Timestamp>,
}

impl Peer {
    pub fn new(
        device_id: impl Into<String>,
        name: impl Into<String>,
        paired_at: Timestamp,
    ) -> Self {
        Peer {
            device_id: device_id.into(),
            name: name.into(),
            paired_at,
            token_hash: None,
            token: None,
            fingerprint: None,
            addrs: Vec::new(),
            port: crate::sync::protocol::PORT,
            pulled_upto: 0,
            pushed_upto: 0,
            last_ok_addr: None,
            last_seen_at: None,
            last_failed_at: None,
        }
    }
}

impl Peer {
    /// Moves the cursors on after serving this peer's sync request, to the
    /// same meaning they have when this device is the client, so a PC that
    /// syncs both ways picks up where either direction left off: everything
    /// it pushed is now merged here, and it says it has stored ours up to
    /// its `pulled_upto`.
    pub fn note_served(&mut self, request: &crate::sync::protocol::SyncRequest, now: Timestamp) {
        self.pulled_upto = self.pulled_upto.max(request.push.max_seq);
        self.pushed_upto = self.pushed_upto.max(request.pulled_upto);
        self.last_seen_at = Some(now);
    }
}

/// The device-local setting key the list is stored under.
pub const PEERS_KEY: &str = "peers";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Peers(pub Vec<Peer>);

impl Peers {
    pub fn get(&self, device_id: &str) -> Option<&Peer> {
        self.0.iter().find(|p| p.device_id == device_id)
    }

    pub fn get_mut(&mut self, device_id: &str) -> Option<&mut Peer> {
        self.0.iter_mut().find(|p| p.device_id == device_id)
    }

    /// Adds a peer, replacing any earlier pairing with the same device.
    pub fn upsert(&mut self, peer: Peer) {
        match self.get_mut(&peer.device_id) {
            Some(existing) => *existing = peer,
            None => self.0.push(peer),
        }
    }

    pub fn remove(&mut self, device_id: &str) -> bool {
        let before = self.0.len();
        self.0.retain(|p| p.device_id != device_id);
        before != self.0.len()
    }

    /// The peer whose token this is (the PC checking a bearer token).
    pub fn by_token(&self, token: &str, matches: impl Fn(&str, &str) -> bool) -> Option<&Peer> {
        self.0
            .iter()
            .find(|p| p.token_hash.as_deref().is_some_and(|h| matches(token, h)))
    }

    /// The PC a phone should check in with: one that answered last time,
    /// most recently heard from first. When every PC failed last time, the
    /// one that failed longest ago, so the phone works its way round them.
    pub fn best_pc(&self) -> Option<&Peer> {
        self.0
            .iter()
            .filter(|p| p.token.is_some() && p.fingerprint.is_some())
            .max_by_key(|p| {
                let failed = p.last_failed_at.is_some_and(|f| Some(f) >= p.last_seen_at);
                (
                    !failed,
                    if failed {
                        p.last_failed_at.map(|f| -f.0)
                    } else {
                        p.last_seen_at.map(|s| s.0)
                    },
                )
            })
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Peer> {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Timestamp = Timestamp(1_789_500_000_000);

    #[test]
    fn upsert_replaces_a_re_paired_device() {
        let mut peers = Peers::default();
        peers.upsert(Peer::new("phone", "Old phone", T));
        let mut again = Peer::new("phone", "Pixel 8", T.plus(1000));
        again.token_hash = Some("hash".into());
        peers.upsert(again);

        assert_eq!(peers.0.len(), 1);
        assert_eq!(peers.get("phone").unwrap().name, "Pixel 8");
        assert_eq!(
            peers.get("phone").unwrap().token_hash.as_deref(),
            Some("hash")
        );
    }

    #[test]
    fn finds_a_peer_by_its_token_and_forgets_it_on_request() {
        let mut peers = Peers::default();
        let mut peer = Peer::new("phone", "Pixel", T);
        peer.token_hash = Some("stored-hash".into());
        peers.upsert(peer);

        let matches = |token: &str, hash: &str| format!("{token}-hash") == hash;
        assert_eq!(
            peers
                .by_token("stored", matches)
                .map(|p| p.device_id.as_str()),
            Some("phone")
        );
        assert!(peers.by_token("other", matches).is_none());

        assert!(peers.remove("phone"));
        assert!(!peers.remove("phone"));
        assert!(peers.is_empty());
    }

    fn pc(id: &str, seen: Option<i64>, failed: Option<i64>) -> Peer {
        let mut peer = Peer::new(id, id, T);
        peer.token = Some("token".into());
        peer.fingerprint = Some("fp".into());
        peer.last_seen_at = seen.map(|ms| T.plus(ms));
        peer.last_failed_at = failed.map(|ms| T.plus(ms));
        peer
    }

    #[test]
    fn a_phone_checks_in_with_the_pc_it_heard_from_last_and_moves_on_when_it_fails() {
        let best = |peers: &Peers| peers.best_pc().map(|p| p.device_id.clone());
        let mut peers = Peers::default();
        assert_eq!(best(&peers), None);

        peers.upsert(Peer::new("phone-only", "Not a PC", T));
        assert_eq!(best(&peers), None, "a record without a token is no PC");

        peers.upsert(pc("desk", Some(10), None));
        peers.upsert(pc("laptop", Some(20), None));
        assert_eq!(best(&peers).as_deref(), Some("laptop"));

        peers.upsert(pc("laptop", Some(20), Some(30)));
        assert_eq!(best(&peers).as_deref(), Some("desk"));

        // Both failing: try the one that failed longest ago.
        peers.upsert(pc("desk", Some(10), Some(40)));
        assert_eq!(best(&peers).as_deref(), Some("laptop"));
    }

    #[test]
    fn older_records_gain_defaults() {
        let peer: Peer =
            serde_json::from_str(r#"{"deviceId":"phone","name":"Pixel","pairedAt":1789500000000}"#)
                .unwrap();
        assert_eq!(peer.pulled_upto, 0);
        assert_eq!(
            peer.port, 0,
            "port defaults to 0 when the record predates it"
        );
        assert!(peer.addrs.is_empty());
    }
}
