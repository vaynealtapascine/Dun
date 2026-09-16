//! The PC's HTTPS server: two routes, its own runtime, and no knowledge of
//! what a sync means — that is the [`Backend`]'s job.

use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use axum_server::tls_rustls::RustlsConfig;
use axum_server::Handle;
use dun_core::sync::protocol::{
    error, ErrorBody, PairRequest, PairResponse, SyncRequest, SyncResponse, PAIR_PATH, SYNC_PATH,
};

use crate::cert::Identity;
use crate::pin::install_crypto_provider;

/// How long in-flight requests get when the server is asked to stop.
const SHUTDOWN_GRACE: Duration = Duration::from_millis(500);

/// A refusal to send back, with the status code the phone should see.
#[derive(Debug, Clone)]
pub struct Refusal {
    pub status: u16,
    pub body: ErrorBody,
}

impl Refusal {
    pub fn new(status: u16, body: ErrorBody) -> Self {
        Refusal { status, body }
    }

    pub fn unauthorized() -> Self {
        Refusal::new(
            401,
            ErrorBody::new(error::UNAUTHORIZED, "This device isn't paired with that PC"),
        )
    }
}

/// What the app plugs in: pairing and syncing against the real store.
pub trait Backend: Send + Sync + 'static {
    fn pair(&self, request: PairRequest) -> Result<PairResponse, Refusal>;
    /// `token` is the bearer token as sent; the backend decides if it's known.
    fn sync(&self, token: &str, request: SyncRequest) -> Result<SyncResponse, Refusal>;
}

/// A running server. Dropping it stops listening.
pub struct Server {
    handle: Handle<SocketAddr>,
    runtime: Option<tokio::runtime::Runtime>,
    addr: SocketAddr,
}

impl Server {
    /// Starts listening on `port` (0 picks a free one, which tests use).
    pub fn start(
        backend: Arc<dyn Backend>,
        identity: &Identity,
        port: u16,
    ) -> std::io::Result<Server> {
        install_crypto_provider();
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        let addr = listener.local_addr()?;
        // Tokio adopts this listener; a blocking one would never accept.
        listener.set_nonblocking(true)?;

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("dun-sync")
            .build()?;

        // Built synchronously: `RustlsConfig::from_pem` is async, and blocking on
        // it would panic when the caller already has a runtime (as tests do).
        let tls = RustlsConfig::from_config(Arc::new(server_config(identity)?));

        let router = Router::new()
            .route(PAIR_PATH, post(pair))
            .route(SYNC_PATH, post(sync))
            .with_state(backend);

        let handle = Handle::new();
        // Adopting the listener registers it with a reactor, so this has to run
        // inside our runtime. The desktop starts the server from a plain thread,
        // where `block_on` would be fine but panics when a caller already has a
        // runtime (as the tests do); entering panics in neither case.
        let server = {
            let _guard = runtime.enter();
            axum_server::from_tcp_rustls(listener, tls)?
        }
        .handle(handle.clone());
        runtime.spawn(async move {
            if let Err(e) = server.serve(router.into_make_service()).await {
                eprintln!("sync server stopped: {e}");
            }
        });

        Ok(Server {
            handle,
            runtime: Some(runtime),
            addr,
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.handle.graceful_shutdown(Some(SHUTDOWN_GRACE));
        if let Some(runtime) = self.runtime.take() {
            // Shutting a runtime down blocks, which isn't allowed inside another
            // runtime, and the caller shouldn't wait for stragglers anyway.
            std::thread::spawn(move || runtime.shutdown_timeout(SHUTDOWN_GRACE));
        }
    }
}

/// The server's TLS configuration: our own certificate, no client auth.
fn server_config(identity: &Identity) -> std::io::Result<rustls::ServerConfig> {
    let certs = rustls_pemfile::certs(&mut identity.cert_pem.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| std::io::Error::other(format!("unreadable certificate: {e}")))?;
    let key = rustls_pemfile::private_key(&mut identity.key_pem.as_bytes())
        .map_err(|e| std::io::Error::other(format!("unreadable key: {e}")))?
        .ok_or_else(|| std::io::Error::other("the certificate has no private key"))?;

    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| std::io::Error::other(e.to_string()))?
    .with_no_client_auth()
    .with_single_cert(certs, key)
    .map_err(|e| std::io::Error::other(format!("TLS setup failed: {e}")))?;
    // Clients offer ALPN; without this the handshake has nothing to agree on.
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(config)
}

type Reply<T> = Result<Json<T>, (StatusCode, Json<ErrorBody>)>;

fn refuse<T>(refusal: Refusal) -> Reply<T> {
    let status = StatusCode::from_u16(refusal.status).unwrap_or(StatusCode::BAD_REQUEST);
    Err((status, Json(refusal.body)))
}

async fn pair(
    State(backend): State<Arc<dyn Backend>>,
    Json(request): Json<PairRequest>,
) -> Reply<PairResponse> {
    match backend.pair(request) {
        Ok(response) => Ok(Json(response)),
        Err(refusal) => refuse(refusal),
    }
}

async fn sync(
    State(backend): State<Arc<dyn Backend>>,
    headers: HeaderMap,
    Json(request): Json<SyncRequest>,
) -> Reply<SyncResponse> {
    let Some(token) = bearer(&headers) else {
        return refuse(Refusal::unauthorized());
    };
    match backend.sync(token, request) {
        Ok(response) => Ok(Json(response)),
        Err(refusal) => refuse(refusal),
    }
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_bearer_tokens_and_ignores_anything_else() {
        let mut headers = HeaderMap::new();
        assert_eq!(bearer(&headers), None);
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer abc123".parse().unwrap(),
        );
        assert_eq!(bearer(&headers), Some("abc123"));
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Basic abc123".parse().unwrap(),
        );
        assert_eq!(bearer(&headers), None);
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer ".parse().unwrap(),
        );
        assert_eq!(bearer(&headers), None);
    }
}
