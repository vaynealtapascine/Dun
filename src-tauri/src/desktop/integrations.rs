//! Local, authenticated API for app-to-app reminder and timer creation.
use std::net::TcpListener;
use std::sync::{Arc, Mutex, RwLock, Weak};

use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use dun_core::actions::ItemDraft;
use dun_core::model::Schedule;
use dun_core::time::Timestamp;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::app_core::AppCore;

const KEY: &str = "integrations-v1";
pub const PORT: u16 = 48475;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    enabled: bool,
    token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    enabled: bool,
    running: bool,
    url: String,
    token: String,
    error: Option<String>,
}

pub struct IntegrationHub {
    core: Arc<AppCore>,
    config: RwLock<Config>,
    server: Mutex<Option<LocalServer>>,
    error: Mutex<Option<String>>,
    changed: Box<dyn Fn() + Send + Sync>,
    port: u16,
}

impl IntegrationHub {
    fn items(&self, archived: bool) -> serde_json::Value {
        let mut snapshot = self.core.snapshot_json();
        let mut items = snapshot["items"]
            .take()
            .as_array()
            .cloned()
            .unwrap_or_default();
        if archived {
            items.extend(
                snapshot["archived"]
                    .take()
                    .as_array()
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        items.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        serde_json::json!({ "items": items })
    }

    fn item(&self, id: &str) -> ApiResult<Json<serde_json::Value>> {
        self.items(true)["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"].as_str() == Some(id))
            .cloned()
            .map(Json)
            .ok_or_else(|| fail(StatusCode::NOT_FOUND, "Item not found"))
    }

    fn act(&self, id: &str, action: ItemAction) -> ApiResult<Json<serde_json::Value>> {
        let (now, tz) = (self.core.now(), self.core.tz());
        let mut engine = self.core.engine();
        let result = match action {
            ItemAction::Done { occurrence } => engine.done(now, &tz, id, Some(occurrence)),
            ItemAction::Snooze {
                occurrence,
                minutes,
            } => engine.snooze(now, &tz, id, Some(occurrence), minutes),
            ItemAction::Archive {} => engine.set_deleted(now, id, true),
            ItemAction::Restore {} => engine.set_deleted(now, id, false),
            ItemAction::Start {} => engine.start_timer(now, id),
            ItemAction::Pause {} => engine.pause_timer(now, id),
            ItemAction::Resume {} => engine.resume_timer(now, id),
            ItemAction::Reset {} => engine.reset_timer(now, id),
        };
        result.map_err(|error| {
            use dun_core::{actions::ActionError, engine::EngineError};
            match error {
                EngineError::Action(ActionError::NotFound) => {
                    fail(StatusCode::NOT_FOUND, "Item not found")
                }
                EngineError::Action(e) => fail(StatusCode::UNPROCESSABLE_ENTITY, &e.to_string()),
                _ => fail(StatusCode::INTERNAL_SERVER_ERROR, "Couldn't save the item"),
            }
        })?;
        drop(engine);
        (self.changed)();
        self.item(id)
    }

    pub fn new(
        core: Arc<AppCore>,
        changed: impl Fn() + Send + Sync + 'static,
    ) -> Result<Arc<Self>, String> {
        Self::with_port(core, changed, PORT)
    }

    fn with_port(
        core: Arc<AppCore>,
        changed: impl Fn() + Send + Sync + 'static,
        port: u16,
    ) -> Result<Arc<Self>, String> {
        let config = core
            .engine()
            .store()
            .local_get::<Config>(KEY)
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| Config {
                enabled: false,
                token: new_token(),
            });
        save_config(&core, &config)?;
        let hub = Arc::new(Self {
            core,
            config: RwLock::new(config),
            server: Mutex::new(None),
            error: Mutex::new(None),
            changed: Box::new(changed),
            port,
        });
        let enabled = hub.config.read().unwrap().enabled;
        if enabled {
            if let Err(e) = hub.set_enabled(true) {
                *hub.error.lock().unwrap() = Some(e);
            }
        }
        Ok(hub)
    }

    pub fn status(&self) -> Status {
        let server = self.server.lock().unwrap();
        let config = self.config.read().unwrap();
        Status {
            enabled: config.enabled,
            running: server.is_some(),
            url: format!(
                "http://127.0.0.1:{}",
                server.as_ref().map_or(self.port, |s| s.port)
            ),
            token: config.token.clone(),
            error: self.error.lock().unwrap().clone(),
        }
    }

    pub fn set_enabled(self: &Arc<Self>, enabled: bool) -> Result<Status, String> {
        let mut server = self.server.lock().unwrap();
        let mut next = self.config.read().unwrap().clone();
        next.enabled = enabled;
        if enabled && server.is_none() {
            match LocalServer::start(self.clone()) {
                Ok(started) => *server = Some(started),
                Err(e) => {
                    let e = format!(
                        "Couldn't start the app connection on port {}: {e}",
                        self.port
                    );
                    *self.error.lock().unwrap() = Some(e.clone());
                    return Err(e);
                }
            }
        }
        save_config(&self.core, &next)?;
        *self.config.write().unwrap() = next;
        if !enabled {
            server.take();
        }
        *self.error.lock().unwrap() = None;
        drop(server);
        Ok(self.status())
    }

    pub fn rotate_token(&self) -> Result<Status, String> {
        let server = self.server.lock().unwrap();
        let mut config = self.config.write().unwrap();
        let mut next = config.clone();
        next.token = new_token();
        save_config(&self.core, &next)?;
        *config = next;
        drop(config);
        drop(server);
        Ok(self.status())
    }

    fn authorize(&self, headers: &HeaderMap) -> ApiResult<()> {
        let config = self.config.read().unwrap();
        let given = headers
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .unwrap_or("");
        // Hash fixed-size values before comparison. No token is ever returned over HTTP.
        let a = Sha256::digest(given.as_bytes());
        let b = Sha256::digest(config.token.as_bytes());
        let mismatch = a
            .iter()
            .zip(b.iter())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b));
        if !config.enabled || mismatch != 0 {
            return Err(fail(StatusCode::UNAUTHORIZED, "Invalid app connection key"));
        }
        // Browser requests aren't an app integration; don't allow cookie/Origin based access.
        if headers.contains_key("origin") {
            return Err(fail(
                StatusCode::FORBIDDEN,
                "Send requests from your app's server",
            ));
        }
        Ok(())
    }

    fn create(&self, request: Request, timer: bool) -> ApiResult<Json<Created>> {
        let draft = request.draft(timer)?;
        let id = format!(
            "api-{:x}",
            Sha256::digest(serde_json::to_vec(&(&request.source, &request.external_id)).unwrap())
        );
        let mut engine = self.core.engine();
        if let Some(item) = engine.state().items.get(&id) {
            // Includes completed/deleted items: retrying a delivery never resurrects it.
            return Ok(Json(Created {
                id,
                created: false,
                archived: item.deleted,
            }));
        }
        let now = self.core.now();
        let change =
            dun_core::actions::create_item(engine.state(), engine.device_id(), now, &id, draft)
                .map_err(|e| fail(StatusCode::UNPROCESSABLE_ENTITY, &e.to_string()))?;
        engine.apply(now, change).map_err(|_| {
            fail(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Couldn't save the reminder",
            )
        })?;
        drop(engine);
        (self.changed)();
        Ok(Json(Created {
            id,
            created: true,
            archived: false,
        }))
    }
}

fn new_token() -> String {
    dun_sync::pairing::new_token()
}

fn save_config(core: &AppCore, config: &Config) -> Result<(), String> {
    // Local services can use this explicit path, including after key rotation.
    // This file never travels through sync, backups, or HTTP.
    let file = core.data_dir().join("integration-connection.json");
    let temporary = core.data_dir().join("integration-connection.json.tmp");
    let descriptor = serde_json::json!({
        "enabled": config.enabled, "url": format!("http://127.0.0.1:{PORT}"), "token": config.token,
    });
    std::fs::write(&temporary, serde_json::to_vec(&descriptor).unwrap())
        .map_err(|e| e.to_string())?;
    std::fs::rename(temporary, file).map_err(|e| e.to_string())?;
    core.engine()
        .store_mut()
        .local_set(KEY, config)
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    source: String,
    external_id: String,
    title: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    url: Option<String>,
    due_at: Option<String>,
    duration_seconds: Option<i64>,
}

impl Request {
    fn draft(&self, timer: bool) -> ApiResult<ItemDraft> {
        for (name, value, max) in [
            ("source", &self.source, 80),
            ("externalId", &self.external_id, 512),
            ("title", &self.title, 300),
        ] {
            if value.trim().is_empty() || value.len() > max {
                return Err(fail(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    &format!("{name} must contain 1–{max} bytes"),
                ));
            }
        }
        if self.notes.len() > 16_000 {
            return Err(fail(StatusCode::UNPROCESSABLE_ENTITY, "Notes are too long"));
        }
        let schedule = if timer {
            if self.due_at.is_some() {
                return Err(fail(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Timers use durationSeconds, not dueAt",
                ));
            }
            let seconds = self
                .duration_seconds
                .filter(|s| (1..=31_536_000).contains(s))
                .ok_or_else(|| {
                    fail(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "durationSeconds must be 1–31536000",
                    )
                })?;
            Schedule::Timer {
                duration_ms: seconds * 1000,
            }
        } else {
            if self.duration_seconds.is_some() {
                return Err(fail(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Reminders use dueAt, not durationSeconds",
                ));
            }
            let at = self
                .due_at
                .as_deref()
                .unwrap_or("")
                .parse::<jiff::Timestamp>()
                .map_err(|_| {
                    fail(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "dueAt must be an RFC 3339 time with a timezone offset",
                    )
                })?;
            Schedule::OneOff {
                due: Timestamp::from_jiff(at),
            }
        };
        let mut notes = format!("From {}", self.source.trim());
        if let Some(url) = &self.url {
            if url.len() > 2048
                || !(url.starts_with("https://") || url.starts_with("http://"))
                || url.chars().any(char::is_control)
            {
                return Err(fail(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "url must be an http or https link",
                ));
            }
            notes.push_str(&format!("\n{url}"));
        }
        if !self.notes.is_empty() {
            notes.push_str(&format!("\n\n{}", self.notes));
        }
        Ok(ItemDraft {
            title: self.title.trim().into(),
            notes,
            tag: None,
            schedule,
            nag: None,
            chime: None,
            quiet_exempt: None,
            start_timer: timer,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct Created {
    id: String,
    created: bool,
    archived: bool,
}
type ApiResult<T> = Result<T, (StatusCode, Json<serde_json::Value>)>;

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
enum ItemAction {
    Done { occurrence: Timestamp },
    Snooze { occurrence: Timestamp, minutes: u32 },
    Archive {},
    Restore {},
    Start {},
    Pause {},
    Resume {},
    Reset {},
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ListOptions {
    #[serde(default)]
    include_archived: bool,
}

async fn items(
    State(hub): State<Weak<IntegrationHub>>,
    headers: HeaderMap,
    Query(options): Query<ListOptions>,
) -> ApiResult<Json<serde_json::Value>> {
    let hub = authorized(&hub, &headers)?;
    Ok(Json(hub.items(options.include_archived)))
}

async fn item(
    State(hub): State<Weak<IntegrationHub>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    authorized(&hub, &headers)?.item(&id)
}

async fn action(
    State(hub): State<Weak<IntegrationHub>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(action): Json<ItemAction>,
) -> ApiResult<Json<serde_json::Value>> {
    authorized(&hub, &headers)?.act(&id, action)
}

fn authorized(hub: &Weak<IntegrationHub>, headers: &HeaderMap) -> ApiResult<Arc<IntegrationHub>> {
    let hub = hub
        .upgrade()
        .ok_or_else(|| fail(StatusCode::SERVICE_UNAVAILABLE, "Dun is stopping"))?;
    hub.authorize(headers)?;
    Ok(hub)
}

fn fail(status: StatusCode, message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": message })))
}

async fn health(
    State(hub): State<Weak<IntegrationHub>>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    let hub = hub
        .upgrade()
        .ok_or_else(|| fail(StatusCode::SERVICE_UNAVAILABLE, "Dun is stopping"))?;
    hub.authorize(&headers)?;
    Ok(Json(serde_json::json!({ "ok": true, "apiVersion": 1 })))
}
async fn reminder(
    State(hub): State<Weak<IntegrationHub>>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> ApiResult<Json<Created>> {
    let hub = hub
        .upgrade()
        .ok_or_else(|| fail(StatusCode::SERVICE_UNAVAILABLE, "Dun is stopping"))?;
    hub.authorize(&headers)?;
    hub.create(request, false)
}
async fn timer(
    State(hub): State<Weak<IntegrationHub>>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> ApiResult<Json<Created>> {
    let hub = hub
        .upgrade()
        .ok_or_else(|| fail(StatusCode::SERVICE_UNAVAILABLE, "Dun is stopping"))?;
    hub.authorize(&headers)?;
    hub.create(request, true)
}

struct LocalServer {
    runtime: Option<tokio::runtime::Runtime>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    port: u16,
}
impl LocalServer {
    fn start(hub: Arc<IntegrationHub>) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", hub.port))?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("dun-app-api")
            .build()?;
        let router = Router::new()
            .route("/v1/health", get(health))
            .route("/v1/reminders", post(reminder))
            .route("/v1/timers", post(timer))
            .route("/v1/items", get(items))
            .route("/v1/items/{id}", get(item))
            .route("/v1/items/{id}/actions", post(action))
            .layer(DefaultBodyLimit::max(24 * 1024))
            .with_state(Arc::downgrade(&hub));
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let listener = {
            let _guard = runtime.enter();
            tokio::net::TcpListener::from_std(listener)?
        };
        runtime.spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await;
        });
        Ok(Self {
            runtime: Some(runtime),
            stop: Some(stop),
            port,
        })
    }
}
impl Drop for LocalServer {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_schema_is_strict() {
        for body in [
            serde_json::json!({"action":"done"}),
            serde_json::json!({"action":"pause", "minutes":2}),
            serde_json::json!({"action":"snooze", "occurrence":123, "minutes":1, "extra":true}),
        ] {
            assert!(
                serde_json::from_value::<ItemAction>(body.clone()).is_err(),
                "{body}"
            );
        }
    }

    #[test]
    fn snooze_changes_only_the_requested_due_occurrence() {
        let dir = tempfile::tempdir().unwrap();
        let core = Arc::new(AppCore::open(dir.path()).unwrap());
        let hub = IntegrationHub::with_port(core.clone(), || {}, 0).unwrap();
        let due = core.now().minus(60_000);
        let Json(created) = hub
            .create(
                Request {
                    source: "Test".into(),
                    external_id: "due".into(),
                    title: "Test snooze".into(),
                    notes: String::new(),
                    url: None,
                    due_at: Some(due.to_jiff().to_string()),
                    duration_seconds: None,
                },
                false,
            )
            .unwrap();
        let Json(item) = hub
            .act(
                &created.id,
                ItemAction::Snooze {
                    occurrence: due,
                    minutes: 5,
                },
            )
            .unwrap();
        assert_eq!(item["status"]["snoozed"], true);
        let until = item["status"]["ringsAt"].clone();
        let Json(item) = hub
            .act(
                &created.id,
                ItemAction::Snooze {
                    occurrence: due.minus(1),
                    minutes: 15,
                },
            )
            .unwrap();
        assert_eq!(item["status"]["ringsAt"], until);
    }

    #[test]
    fn http_reads_and_actions_use_the_engine_and_require_auth() {
        dun_sync::pin::install_crypto_provider();
        let dir = tempfile::tempdir().unwrap();
        let core = Arc::new(AppCore::open(dir.path()).unwrap());
        let changes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = changes.clone();
        let hub = IntegrationHub::with_port(
            core.clone(),
            move || {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            },
            0,
        )
        .unwrap();
        let status = hub.set_enabled(true).unwrap();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let client = reqwest::Client::new();
            let base = &status.url;
            let created: Created = client.post(format!("{base}/v1/timers")).bearer_auth(&status.token)
                .json(&serde_json::json!({"source":"Test", "externalId":"tea", "title":"Tea", "durationSeconds":300}))
                .send().await.unwrap().json().await.unwrap();
            let item_url = format!("{base}/v1/items/{}", created.id);
            let actions_url = format!("{item_url}/actions");
            for path in [format!("{base}/v1/items"), item_url.clone()] {
                assert_eq!(client.get(&path).send().await.unwrap().status(), 401);
                assert_eq!(client.get(&path).bearer_auth(&status.token).header("Origin", "null").send().await.unwrap().status(), 403);
            }
            assert_eq!(client.post(&actions_url).json(&serde_json::json!({"action":"archive"})).send().await.unwrap().status(), 401);
            assert_eq!(client.post(&actions_url).bearer_auth(&status.token).header("Origin", "null").json(&serde_json::json!({"action":"archive"})).send().await.unwrap().status(), 403);
            for (action, state) in [("pause", "paused"), ("resume", "running"), ("reset", "idle"), ("start", "running")] {
                let response = client.post(&actions_url).bearer_auth(&status.token).json(&serde_json::json!({"action": action})).send().await.unwrap();
                assert_eq!(response.status(), 200);
                let item: serde_json::Value = response.json().await.unwrap();
                assert_eq!(item["timer"]["state"], state);
            }
            let item: serde_json::Value = client.get(&item_url).bearer_auth(&status.token).send().await.unwrap().json().await.unwrap();
            let occurrence = item["status"]["at"].as_i64().unwrap();
            for _ in 0..2 {
                assert_eq!(client.post(&actions_url).bearer_auth(&status.token).json(&serde_json::json!({"action":"done", "occurrence":occurrence})).send().await.unwrap().status(), 200);
            }
            assert_eq!(core.engine().history(Some(&created.id), None, 100).unwrap().iter().filter(|h| h.kind == dun_core::sync::HistoryKind::Done).count(), 1);
            for body in [serde_json::json!({"action":"done"}), serde_json::json!({"action":"snooze","occurrence":occurrence,"minutes":0}), serde_json::json!({"action":"launch"})] {
                assert_eq!(client.post(&actions_url).bearer_auth(&status.token).json(&body).send().await.unwrap().status(), 422);
            }
            assert_eq!(client.get(format!("{base}/v1/items/missing")).bearer_auth(&status.token).send().await.unwrap().status(), 404);
            assert_eq!(client.post(format!("{base}/v1/items/missing/actions")).bearer_auth(&status.token).json(&serde_json::json!({"action":"archive"})).send().await.unwrap().status(), 404);
            for action in ["archive", "restore", "archive"] {
                assert_eq!(client.post(&actions_url).bearer_auth(&status.token).json(&serde_json::json!({"action":action})).send().await.unwrap().status(), 200);
            }
            let live: serde_json::Value = client.get(format!("{base}/v1/items")).bearer_auth(&status.token).send().await.unwrap().json().await.unwrap();
            assert!(live["items"].as_array().unwrap().is_empty());
            let all: serde_json::Value = client.get(format!("{base}/v1/items?includeArchived=true")).bearer_auth(&status.token).send().await.unwrap().json().await.unwrap();
            assert_eq!(all["items"][0]["deleted"], true);
            assert!(all.get("token").is_none());
        });
        assert_eq!(changes.load(std::sync::atomic::Ordering::SeqCst), 10);
        hub.set_enabled(false).unwrap();
    }

    #[test]
    fn http_auth_validation_retries_rotation_and_restart() {
        dun_sync::pin::install_crypto_provider();
        let dir = tempfile::tempdir().unwrap();
        let core = Arc::new(AppCore::open(dir.path()).unwrap());
        let hub = IntegrationHub::with_port(core.clone(), || {}, 0).unwrap();
        let status = hub.set_enabled(true).unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let client = reqwest::Client::new();
            let url = format!("{}/v1/reminders", status.url);
            let body = serde_json::json!({"source":"Memos","externalId":"memo/1/2026-09-30","title":"Read memo","dueAt":"2026-09-30T09:00:00+08:00"});
            assert_eq!(client.post(&url).json(&body).send().await.unwrap().status(), 401);
            assert_eq!(client.post(&url).bearer_auth(&status.token).header("Origin", "https://example.com").json(&body).send().await.unwrap().status(), 403);
            let first: Created = client.post(&url).bearer_auth(&status.token).json(&body).send().await.unwrap().json().await.unwrap();
            assert!(first.created);
            let repeat: Created = client.post(&url).bearer_auth(&status.token).json(&body).send().await.unwrap().json().await.unwrap();
            assert!(!repeat.created);
            assert_eq!(repeat.id, first.id);
            core.engine().set_deleted(core.now(), &first.id, true).unwrap();
            let archived: Created = client.post(&url).bearer_auth(&status.token).json(&body).send().await.unwrap().json().await.unwrap();
            assert!(archived.archived);
            let bad = serde_json::json!({"source":"Arbor","externalId":"1","title":"Bad","dueAt":"2026-09-30T09:00:00"});
            assert_eq!(client.post(&url).bearer_auth(&status.token).json(&bad).send().await.unwrap().status(), 422);
            let timer_url = format!("{}/v1/timers", status.url);
            for seconds in [0, i64::MAX] {
                let body = serde_json::json!({"source":"Arbor","externalId":"timer","title":"Tea","durationSeconds":seconds});
                assert_eq!(client.post(&timer_url).bearer_auth(&status.token).json(&body).send().await.unwrap().status(), 422);
            }
            let body = serde_json::json!({"source":"Arbor","externalId":"timer","title":"Tea","durationSeconds":60});
            let timer: Created = client.post(&timer_url).bearer_auth(&status.token).json(&body).send().await.unwrap().json().await.unwrap();
            assert!(matches!(core.engine().state().items[&timer.id].timer, dun_core::model::TimerState::Running { .. }));
            let next = hub.rotate_token().unwrap();
            assert_ne!(next.token, status.token);
            assert_eq!(client.get(format!("{}/v1/health", status.url)).bearer_auth(&status.token).send().await.unwrap().status(), 401);
            assert_eq!(client.get(format!("{}/v1/health", status.url)).bearer_auth(&next.token).send().await.unwrap().status(), 200);
        });
        hub.set_enabled(false).unwrap();
        let token = hub.status().token;
        drop(hub);
        drop(core);
        let core = Arc::new(AppCore::open(dir.path()).unwrap());
        let hub = IntegrationHub::with_port(core.clone(), || {}, 0).unwrap();
        assert_eq!(hub.status().token, token);
        assert_eq!(core.engine().state().items.len(), 2);
    }
}
