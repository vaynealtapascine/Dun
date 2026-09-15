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
        }
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
