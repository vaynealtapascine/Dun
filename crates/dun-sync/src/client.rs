//! The phone's side: reach the PC over whichever address answers first.
//!
//! A PC moves between addresses (Wi-Fi, a second NIC, Tailscale), so every
//! response refreshes the list and the address that worked is tried first
//! next time. The rest are tried in parallel, staggered, so a dead address
//! costs a moment rather than the whole attempt.

use std::sync::Mutex;
use std::time::Duration;

use dun_core::sync::protocol::{
    ErrorBody, PairRequest, PairResponse, SyncRequest, SyncResponse, APK_PATH, PAIR_PATH, SYNC_PATH,
};

use crate::pin::pinned_client_config;

/// Gap between starting attempts on different addresses.
pub const STAGGER: Duration = Duration::from_millis(300);
pub const CONNECT_TIMEOUT: Duration = Duration::from_millis(1500);
/// Everything, including retries, fits in this.
pub const TOTAL_TIMEOUT: Duration = Duration::from_millis(3500);

/// A package is tens of megabytes over Wi-Fi, so it gets its own budget
/// instead of the one sized for a check-in.
pub const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("couldn't reach the PC (tried {tried})")]
    Unreachable { tried: String },
    #[error("{}", .0.message)]
    Refused(ErrorBody),
    #[error("the PC answered with {status}")]
    Status { status: u16 },
    #[error("couldn't set up the connection: {0}")]
    Setup(String),
}

impl ClientError {
    /// The machine-readable reason, when the PC gave one.
    pub fn code(&self) -> Option<&str> {
        match self {
            ClientError::Refused(body) => Some(&body.error),
            _ => None,
        }
    }

    /// True when the PC answered and said no, rather than not answering.
    pub fn is_from_server(&self) -> bool {
        matches!(self, ClientError::Refused(_) | ClientError::Status { .. })
    }
}

pub struct SyncClient {
    http: reqwest::Client,
    token: String,
    port: u16,
    addrs: Mutex<Vec<String>>,
    last_ok: Mutex<Option<String>>,
}

impl SyncClient {
    pub fn new(
        fingerprint: &str,
        token: String,
        addrs: Vec<String>,
        port: u16,
    ) -> Result<Self, ClientError> {
        Ok(SyncClient {
            http: http_client(fingerprint)?,
            token,
            port,
            addrs: Mutex::new(addrs),
            last_ok: Mutex::new(None),
        })
    }

    /// Addresses to try, best first.
    pub fn candidates(&self) -> Vec<String> {
        let addrs = self.addrs.lock().unwrap_or_else(|p| p.into_inner());
        let last_ok = self
            .last_ok
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let mut order: Vec<String> = last_ok.iter().cloned().collect();
        order.extend(
            addrs
                .iter()
                .filter(|a| Some(*a) != last_ok.as_ref())
                .cloned(),
        );
        order
    }

    pub fn last_ok_addr(&self) -> Option<String> {
        self.last_ok
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// One sync. On success the address that answered is remembered and the
    /// address list is refreshed from the response.
    pub async fn sync(&self, request: &SyncRequest) -> Result<SyncResponse, ClientError> {
        let candidates = self.candidates();
        if candidates.is_empty() {
            return Err(ClientError::Unreachable {
                tried: "no addresses".into(),
            });
        }

        let (tx, mut rx) = tokio::sync::mpsc::channel(candidates.len());
        for (index, addr) in candidates.iter().enumerate() {
            let (http, token, port, addr, tx) = (
                self.http.clone(),
                self.token.clone(),
                self.port,
                addr.clone(),
                tx.clone(),
            );
            let body = request.clone();
            tokio::spawn(async move {
                tokio::time::sleep(STAGGER * index as u32).await;
                let result = post_json::<_, SyncResponse>(
                    &http,
                    &url(&addr, port, SYNC_PATH),
                    Some(&token),
                    &body,
                )
                .await;
                let _ = tx.send((addr, result)).await;
            });
        }
        drop(tx);

        enum Outcome {
            Answered(String, Box<SyncResponse>),
            /// The PC itself said no; another address won't do better.
            Refused(ClientError),
        }

        let mut last_error = None;
        let raced = tokio::time::timeout(TOTAL_TIMEOUT, async {
            while let Some((addr, result)) = rx.recv().await {
                match result {
                    Ok(response) => return Some(Outcome::Answered(addr, Box::new(response))),
                    Err(e) if e.is_from_server() => return Some(Outcome::Refused(e)),
                    Err(e) => last_error = Some(e),
                }
            }
            None
        })
        .await;

        match raced {
            Ok(Some(Outcome::Answered(addr, response))) => {
                *self.last_ok.lock().unwrap_or_else(|p| p.into_inner()) = Some(addr);
                if !response.addrs.is_empty() {
                    *self.addrs.lock().unwrap_or_else(|p| p.into_inner()) = response.addrs.clone();
                }
                Ok(*response)
            }
            Ok(Some(Outcome::Refused(e))) => Err(e),
            _ => Err(last_error.unwrap_or(ClientError::Unreachable {
                tried: candidates.join(", "),
            })),
        }
    }
}

/// Pairs with a PC: one address, the code from its screen.
pub async fn pair(
    fingerprint: &str,
    addr: &str,
    port: u16,
    request: &PairRequest,
) -> Result<PairResponse, ClientError> {
    let http = http_client(fingerprint)?;
    post_json(&http, &url(addr, port, PAIR_PATH), None, request).await
}

/// Fetches the Android package the PC offered, writing it to `into`.
///
/// The hash is checked while it arrives and a bad or short download leaves no
/// file behind: a half-written APK handed to Android's installer is a puzzle
/// nobody enjoys.
pub async fn download_apk(
    fingerprint: &str,
    addr: &str,
    port: u16,
    token: &str,
    sha256: &str,
    into: &std::path::Path,
) -> Result<u64, ClientError> {
    use sha2::{Digest, Sha256};

    let http = reqwest::Client::builder()
        .use_preconfigured_tls(pinned_client_config(fingerprint))
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|e| ClientError::Setup(e.to_string()))?;

    let mut response = http
        .get(url(addr, port, APK_PATH))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| ClientError::Unreachable { tried: chain(&e) })?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        return match response.json::<ErrorBody>().await {
            Ok(body) => Err(ClientError::Refused(body)),
            Err(_) => Err(ClientError::Status { status }),
        };
    }

    let temporary = into.with_extension("part");
    let mut file = std::fs::File::create(&temporary)
        .map_err(|e| ClientError::Setup(format!("couldn't write the download: {e}")))?;
    let mut hasher = Sha256::new();
    let mut written = 0u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| ClientError::Unreachable { tried: chain(&e) })?
    {
        use std::io::Write;
        hasher.update(&chunk);
        file.write_all(&chunk)
            .map_err(|e| ClientError::Setup(format!("couldn't write the download: {e}")))?;
        written += chunk.len() as u64;
    }
    drop(file);

    let got = hex(&hasher.finalize());
    if !got.eq_ignore_ascii_case(sha256) {
        let _ = std::fs::remove_file(&temporary);
        return Err(ClientError::Setup(
            "the download didn't arrive whole; try again".into(),
        ));
    }
    std::fs::rename(&temporary, into)
        .map_err(|e| ClientError::Setup(format!("couldn't finish the download: {e}")))?;
    Ok(written)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn http_client(fingerprint: &str) -> Result<reqwest::Client, ClientError> {
    reqwest::Client::builder()
        .use_preconfigured_tls(pinned_client_config(fingerprint))
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(TOTAL_TIMEOUT)
        .build()
        .map_err(|e| ClientError::Setup(e.to_string()))
}

/// The whole error chain: reqwest's own message rarely says why.
fn chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(e) = source {
        text.push_str(": ");
        text.push_str(&e.to_string());
        source = e.source();
    }
    text
}

fn url(addr: &str, port: u16, path: &str) -> String {
    // IPv6 literals need brackets.
    if addr.contains(':') && !addr.starts_with('[') {
        format!("https://[{addr}]:{port}{path}")
    } else {
        format!("https://{addr}:{port}{path}")
    }
}

async fn post_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
    http: &reqwest::Client,
    url: &str,
    token: Option<&str>,
    body: &B,
) -> Result<T, ClientError> {
    let mut request = http.post(url).json(body);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|e| ClientError::Unreachable { tried: chain(&e) })?;
    let status = response.status();
    if status.is_success() {
        return response
            .json::<T>()
            .await
            .map_err(|e| ClientError::Setup(format!("unreadable answer: {e}")));
    }
    match response.json::<ErrorBody>().await {
        Ok(body) => Err(ClientError::Refused(body)),
        Err(_) => Err(ClientError::Status {
            status: status.as_u16(),
        }),
    }
}
