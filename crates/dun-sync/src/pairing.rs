//! Pairing: a short code shown on the PC, traded once for a long-lived token.
//!
//! The code is only accepted while the Pairing dialog is open, for five
//! minutes, five tries, and once. The token is random and the PC stores only
//! its hash, so a stolen database can't be used to sync.

use dun_core::sync::protocol::{error, ErrorBody, PairRequest, PairResponse, PORT};
use dun_core::time::Timestamp;
use sha2::{Digest, Sha256};

use crate::pin::constant_time_eq;

pub const CODE_TTL_MS: i64 = 5 * 60_000;
pub const MAX_TRIES: u8 = 5;
const CODE_DIGITS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PairError {
    #[error("Dun isn't waiting to pair; open Pairing on the PC first")]
    NotPairing,
    #[error("that code doesn't match the one on the PC")]
    BadCode,
    #[error("that code has expired; get a new one on the PC")]
    Expired,
    #[error("too many wrong codes; open Pairing on the PC again")]
    TooManyTries,
    #[error("that code has already been used")]
    AlreadyUsed,
}

impl PairError {
    pub fn body(&self) -> ErrorBody {
        let code = match self {
            PairError::NotPairing => error::NOT_PAIRING,
            PairError::TooManyTries => error::TOO_MANY_TRIES,
            _ => error::BAD_CODE,
        };
        ErrorBody::new(code, self.to_string())
    }
}

/// An open Pairing dialog: one code, ticking down.
#[derive(Debug, Clone)]
pub struct Pairing {
    code: String,
    opened_at: Timestamp,
    tries: u8,
    used: bool,
}

impl Pairing {
    pub fn open(now: Timestamp) -> Self {
        Pairing {
            code: new_code(),
            opened_at: now,
            tries: 0,
            used: false,
        }
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn expires_at(&self) -> Timestamp {
        self.opened_at.plus(CODE_TTL_MS)
    }

    pub fn tries_left(&self) -> u8 {
        MAX_TRIES.saturating_sub(self.tries)
    }

    /// Checks a code, counting the attempt. A correct code can only be used once.
    pub fn check(&mut self, code: &str, now: Timestamp) -> Result<(), PairError> {
        if self.used {
            return Err(PairError::AlreadyUsed);
        }
        if self.tries >= MAX_TRIES {
            return Err(PairError::TooManyTries);
        }
        if now >= self.expires_at() {
            return Err(PairError::Expired);
        }
        self.tries += 1;
        if !constant_time_eq(code.trim(), &self.code) {
            return Err(PairError::BadCode);
        }
        self.used = true;
        Ok(())
    }
}

/// Six digits, easy to read off a screen and type on a phone.
pub fn new_code() -> String {
    let mut code = String::with_capacity(CODE_DIGITS);
    for byte in random_bytes::<CODE_DIGITS>() {
        // Bias is negligible for a five-minute, five-try code.
        code.push(char::from(b'0' + byte % 10));
    }
    code
}

/// 256 bits of randomness, hex encoded.
pub fn new_token() -> String {
    hex(&random_bytes::<32>())
}

pub fn token_hash(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

pub fn token_matches(token: &str, stored_hash: &str) -> bool {
    constant_time_eq(&token_hash(token), stored_hash)
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buffer = [0u8; N];
    getrandom::fill(&mut buffer).expect("the OS random generator is available");
    buffer
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// What the QR code (and the paste-able text) carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingInvite {
    pub pc_device_id: String,
    pub pc_name: String,
    pub port: u16,
    /// Addresses to try, freshest first.
    pub addrs: Vec<String>,
    pub fingerprint: String,
    pub code: String,
}

impl PairingInvite {
    /// `dun://pair?v=1&id=…&n=…&p=47823&a=192.168.1.10,100.64.0.2&fp=…&c=123456`
    pub fn to_uri(&self) -> String {
        let escape = |s: &str| {
            s.bytes()
                .map(|b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        char::from(b).to_string()
                    }
                    other => format!("%{other:02X}"),
                })
                .collect::<String>()
        };
        format!(
            "dun://pair?v=1&id={}&n={}&p={}&a={}&fp={}&c={}",
            escape(&self.pc_device_id),
            escape(&self.pc_name),
            self.port,
            escape(&self.addrs.join(",")),
            escape(&self.fingerprint),
            escape(&self.code),
        )
    }

    pub fn parse(uri: &str) -> Option<PairingInvite> {
        let query = uri.trim().strip_prefix("dun://pair?")?;
        let mut fields: Vec<(String, String)> = Vec::new();
        for pair in query.split('&') {
            let (key, value) = pair.split_once('=')?;
            fields.push((key.to_string(), unescape(value)?));
        }
        let get = |key: &str| {
            fields
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
        };
        if get("v").as_deref() != Some("1") {
            return None;
        }
        let invite = PairingInvite {
            pc_device_id: get("id")?,
            pc_name: get("n").unwrap_or_else(|| "PC".into()),
            port: get("p").and_then(|p| p.parse().ok()).unwrap_or(PORT),
            addrs: get("a")
                .unwrap_or_default()
                .split(',')
                .filter(|a| !a.is_empty())
                .map(str::to_string)
                .collect(),
            fingerprint: get("fp")?.to_ascii_lowercase(),
            code: get("c")?,
        };
        (invite.fingerprint.len() == 64 && !invite.addrs.is_empty()).then_some(invite)
    }

    pub fn request(&self, device_id: &str, name: &str) -> PairRequest {
        PairRequest {
            code: self.code.clone(),
            device_id: device_id.to_string(),
            name: name.to_string(),
        }
    }
}

/// The PC's side of a successful pairing: what to remember about the phone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedPeer {
    pub device_id: String,
    pub name: String,
    pub token_hash: String,
    pub paired_at: Timestamp,
}

/// Completes pairing: checks the code, mints a token, and returns what each
/// side keeps.
pub fn accept(
    pairing: &mut Pairing,
    request: &PairRequest,
    now: Timestamp,
    pc_device_id: &str,
    pc_name: &str,
) -> Result<(PairResponse, PairedPeer), PairError> {
    pairing.check(&request.code, now)?;
    let token = new_token();
    let peer = PairedPeer {
        device_id: request.device_id.clone(),
        name: request.name.clone(),
        token_hash: token_hash(&token),
        paired_at: now,
    };
    Ok((
        PairResponse {
            token,
            pc_device_id: pc_device_id.to_string(),
            pc_name: pc_name.to_string(),
        },
        peer,
    ))
}

fn unescape(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Timestamp = Timestamp(1_789_500_000_000);

    #[test]
    fn codes_are_six_digits_and_tokens_are_unique() {
        let code = new_code();
        assert_eq!(code.len(), CODE_DIGITS);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
        assert_ne!(new_token(), new_token());
        let token = new_token();
        assert!(token_matches(&token, &token_hash(&token)));
        assert!(!token_matches("guess", &token_hash(&token)));
    }

    #[test]
    fn a_code_works_once() {
        let mut pairing = Pairing::open(T);
        let code = pairing.code().to_string();
        assert!(pairing.check(&code, T).is_ok());
        assert_eq!(pairing.check(&code, T), Err(PairError::AlreadyUsed));
    }

    #[test]
    fn wrong_codes_run_out_of_tries() {
        let mut pairing = Pairing::open(T);
        for _ in 0..MAX_TRIES {
            assert_eq!(pairing.check("000000", T), Err(PairError::BadCode));
        }
        assert_eq!(pairing.tries_left(), 0);
        // Even the right code is refused now.
        let code = pairing.code().to_string();
        assert_eq!(pairing.check(&code, T), Err(PairError::TooManyTries));
    }

    #[test]
    fn codes_expire_after_five_minutes() {
        let mut pairing = Pairing::open(T);
        let code = pairing.code().to_string();
        assert_eq!(pairing.expires_at(), T.plus(CODE_TTL_MS));
        assert_eq!(
            pairing.check(&code, T.plus(CODE_TTL_MS)),
            Err(PairError::Expired)
        );
        assert!(pairing.check(&code, T.plus(CODE_TTL_MS - 1)).is_ok());
    }

    #[test]
    fn accept_mints_a_token_the_pc_only_stores_hashed() {
        let mut pairing = Pairing::open(T);
        let request = PairRequest {
            code: pairing.code().to_string(),
            device_id: "phone".into(),
            name: "Pixel 8".into(),
        };
        let (response, peer) = accept(&mut pairing, &request, T, "pc", "Desk PC").unwrap();
        assert_eq!(peer.device_id, "phone");
        assert_eq!(response.pc_name, "Desk PC");
        assert!(token_matches(&response.token, &peer.token_hash));
        assert!(!peer.token_hash.contains(&response.token));
    }

    #[test]
    fn invites_round_trip_through_the_qr_payload() {
        let invite = PairingInvite {
            pc_device_id: "0192-abc".into(),
            pc_name: "Alex's PC".into(),
            port: PORT,
            addrs: vec!["192.168.1.10".into(), "100.64.0.2".into()],
            fingerprint: "a".repeat(64),
            code: "123456".into(),
        };
        let uri = invite.to_uri();
        assert!(uri.starts_with("dun://pair?v=1&"));
        assert!(uri.contains("n=Alex%27s%20PC"));
        assert_eq!(PairingInvite::parse(&uri), Some(invite));
    }

    #[test]
    fn nonsense_invites_are_refused() {
        assert_eq!(PairingInvite::parse("https://example.com"), None);
        assert_eq!(
            PairingInvite::parse("dun://pair?v=2&id=x&a=1.2.3.4&fp=aa&c=1"),
            None,
            "unknown version"
        );
        assert_eq!(
            PairingInvite::parse(&format!("dun://pair?v=1&id=x&a=&fp={}&c=1", "a".repeat(64))),
            None,
            "no addresses to try"
        );
        assert_eq!(
            PairingInvite::parse("dun://pair?v=1&id=x&a=1.2.3.4&fp=short&c=1"),
            None,
            "fingerprint isn't a SHA-256"
        );
    }
}
