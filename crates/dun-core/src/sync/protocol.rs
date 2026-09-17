//! The wire protocol between the phone (client) and the PC (server).
//!
//! `POST /v1/pair` runs once, while the Pairing dialog is open, and trades a
//! short code for a token. `POST /v1/sync` then carries everything else: the
//! phone pushes what it changed, pulls what it hasn't seen, tells the PC which
//! occurrences it is about to alert, and learns whether the PC is attended.
//!
//! Both sides are the same version of Dun in practice, but every response
//! carries the PC's clock so the phone can warn about drift, and unknown
//! fields are ignored so a newer peer doesn't break an older one.

use serde::{Deserialize, Serialize};

use super::{HistoryRow, Reg};
use crate::time::Timestamp;

/// Port the PC listens on.
pub const PORT: u16 = 47823;
pub const PAIR_PATH: &str = "/v1/pair";
pub const SYNC_PATH: &str = "/v1/sync";
/// Where the PC serves the Android package it is offering.
pub const APK_PATH: &str = "/v1/apk";

/// Rows per page; the phone loops while `more` is set.
pub const PAGE_ROWS: usize = 1000;

/// Clocks further apart than this are reported to the user.
pub const SKEW_WARN_MS: i64 = 30_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairRequest {
    /// The code shown in the PC's Pairing dialog.
    pub code: String,
    pub device_id: String,
    /// Shown in the PC's device list, e.g. "Pixel 8".
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairResponse {
    /// Bearer token for every later sync. The PC keeps only its hash.
    pub token: String,
    pub pc_device_id: String,
    pub pc_name: String,
}

/// An occurrence the phone is about to alert (or suppress), so the PC can
/// stay quiet while the phone covers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DueItem {
    pub item: String,
    pub occurrence: Timestamp,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Push {
    #[serde(default)]
    pub registers: Vec<Reg>,
    #[serde(default)]
    pub history: Vec<HistoryRow>,
    /// The sender's own `seq` for the last row in this push; it comes back as
    /// `acked_upto` once the rows are stored.
    pub max_seq: i64,
}

impl Push {
    pub fn is_empty(&self) -> bool {
        self.registers.is_empty() && self.history.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRequest {
    pub device_id: String,
    /// The phone's clock, for the drift check.
    pub now: Timestamp,
    /// Highest PC `seq` the phone has stored.
    pub pulled_upto: i64,
    #[serde(default)]
    pub push: Push,
    #[serde(default)]
    pub due_items: Vec<DueItem>,
    pub app_ver: String,
    pub schema_ver: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pull {
    #[serde(default)]
    pub registers: Vec<Reg>,
    #[serde(default)]
    pub history: Vec<HistoryRow>,
    /// Cursor to send as `pulled_upto` next time.
    pub new_pulled_upto: i64,
    /// More rows are waiting; sync again straight away.
    pub more: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResponse {
    pub pc_now: Timestamp,
    /// The phone's `push.max_seq` that the PC has now stored.
    pub acked_upto: i64,
    #[serde(default)]
    pub pull: Pull,
    /// Someone is at the PC, so the phone can stay quiet.
    pub attended: bool,
    /// What the PC is ringing (or about to).
    #[serde(default)]
    pub ringing_soon: Vec<DueItem>,
    /// Addresses to try next time, freshest first.
    #[serde(default)]
    pub addrs: Vec<String>,
    /// Set when the two clocks disagree by more than [`SKEW_WARN_MS`].
    #[serde(default)]
    pub skew_warning: Option<i64>,
    /// A newer Android build this PC is holding, when the phone is behind.
    #[serde(default)]
    pub update: Option<UpdateOffer>,
}

/// An Android package the PC will serve, offered to a phone running something
/// older. Nothing is downloaded until the user says so.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOffer {
    pub version: String,
    pub size: i64,
    /// Of the file, so a half-finished download can't reach the installer.
    pub sha256: String,
}

/// What the server says when it refuses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    /// Stable machine-readable reason, e.g. "badCode".
    pub error: String,
    /// One sentence for the user.
    pub message: String,
}

impl ErrorBody {
    pub fn new(error: &str, message: impl Into<String>) -> Self {
        ErrorBody {
            error: error.into(),
            message: message.into(),
        }
    }
}

/// Reasons the server refuses, as `error` strings.
pub mod error {
    /// The pairing code was wrong, expired, or already used.
    pub const BAD_CODE: &str = "badCode";
    /// Pairing isn't open right now.
    pub const NOT_PAIRING: &str = "notPairing";
    /// Too many wrong codes; the dialog must be reopened.
    pub const TOO_MANY_TRIES: &str = "tooManyTries";
    /// Missing or unknown bearer token.
    pub const UNAUTHORIZED: &str = "unauthorized";
    /// The pushed rows are stamped too far from the PC's clock.
    pub const CLOCK_DRIFT: &str = "clockDrift";
    /// The request was malformed.
    pub const BAD_REQUEST: &str = "badRequest";
    /// Asked for something this PC doesn't have, such as an Android package.
    pub const NOT_FOUND: &str = "notFound";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hlc::Hlc;
    use crate::sync::Entity;

    #[test]
    fn requests_round_trip_as_camel_case() {
        let request = SyncRequest {
            device_id: "phone".into(),
            now: Timestamp(1_789_500_000_000),
            pulled_upto: 42,
            push: Push {
                registers: vec![Reg {
                    entity: Entity::Item,
                    id: "a".into(),
                    field: "title".into(),
                    value: serde_json::json!("Pay rent"),
                    hlc: Hlc::new(1, 0),
                    device: "phone".into(),
                }],
                history: vec![],
                max_seq: 7,
            },
            due_items: vec![DueItem {
                item: "a".into(),
                occurrence: Timestamp(1_789_500_060_000),
            }],
            app_ver: "0.1.0".into(),
            schema_ver: 1,
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["deviceId"], "phone");
        assert_eq!(json["pulledUpto"], 42);
        assert_eq!(json["push"]["maxSeq"], 7);
        assert_eq!(json["dueItems"][0]["occurrence"], 1_789_500_060_000i64);
        assert_eq!(
            serde_json::from_value::<SyncRequest>(json).unwrap(),
            request
        );
    }

    #[test]
    fn missing_optional_fields_default() {
        // A minimal request from an older or leaner client still parses.
        let request: SyncRequest = serde_json::from_str(
            r#"{"deviceId":"phone","now":1,"pulledUpto":0,"appVer":"0.1.0","schemaVer":1}"#,
        )
        .unwrap();
        assert!(request.push.is_empty());
        assert!(request.due_items.is_empty());

        let response: SyncResponse =
            serde_json::from_str(r#"{"pcNow":2,"ackedUpto":0,"attended":true,"unknownField":5}"#)
                .unwrap();
        assert!(response.attended);
        assert_eq!(response.pull, Pull::default());
        assert_eq!(response.skew_warning, None);
    }
}

/// Whether `candidate` is a later release than `current`.
///
/// Versions are `major.minor.patch` compared number by number, because
/// "0.10.0" sorts before "0.9.0" as text and an update that appears to go
/// backwards would be offered forever. Anything unparseable loses: refusing to
/// offer an update is the harmless way to be wrong.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    fn parts(v: &str) -> Option<[u32; 3]> {
        let mut out = [0u32; 3];
        // A trailing "-beta.2" or "+build" isn't part of the ordering here.
        let core = v.trim().split(['-', '+']).next()?;
        for (i, piece) in core.split('.').enumerate() {
            if i >= 3 {
                return None;
            }
            out[i] = piece.parse().ok()?;
        }
        Some(out)
    }
    match (parts(candidate), parts(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

#[cfg(test)]
mod version_tests {
    use super::is_newer;

    #[test]
    fn compares_numerically_not_alphabetically() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("0.10.0", "0.9.0"), "ten comes after nine");
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(is_newer("0.1.1", "0.1.0"));
    }

    #[test]
    fn is_not_newer_when_the_same_or_older() {
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("0.9.0", "0.10.0"));
    }

    #[test]
    fn short_and_decorated_versions_still_order() {
        assert!(is_newer("0.2", "0.1.9"));
        assert!(is_newer("0.2.0-beta.1", "0.1.0"));
    }

    #[test]
    fn nonsense_never_offers_an_update() {
        assert!(!is_newer("", "0.1.0"));
        assert!(!is_newer("next", "0.1.0"));
        assert!(!is_newer("0.1.0", "unknown"));
        assert!(!is_newer("1.2.3.4", "0.1.0"));
    }
}
