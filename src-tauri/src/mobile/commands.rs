//! Commands for the phone's UI.
//!
//! Every change goes through the same engine the receivers use, then re-plans
//! alarms and notifications so what's on screen and what will ring can't drift
//! apart. Pushing to the PC happens just after, off the UI thread.

use dun_core::actions::{ItemDraft, PresetDraft};
use dun_core::engine::{Engine, EngineError};
use dun_core::model::item::setting_key;
use dun_core::snapshot;
use dun_core::sync::peers::Peer;
use dun_core::sync::HistoryRow;
use dun_core::time::{TimeZone, Timestamp};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_dun_android::DunAndroidExt;

use super::{bridge, core, sync};

pub const STATE_CHANGED: &str = "state-changed";

type CmdResult<T> = Result<T, String>;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// Where the database lives and which zone this phone is in, from Android.
fn device<R: Runtime>(app: &AppHandle<R>) -> CmdResult<(String, String)> {
    let info = app.dun_android().device_info().map_err(|e| e.to_string())?;
    Ok((info.data_dir, info.tz_id))
}

/// Re-plans after a change and hands the plan to Kotlin.
fn replan<R: Runtime>(app: &AppHandle<R>, dir: &str, tz_id: &str) -> CmdResult<()> {
    let plan = bridge::plan_now(dir, tz_id, now_ms())?;
    let value = serde_json::to_value(&plan).map_err(|e| e.to_string())?;
    app.dun_android()
        .apply_plan(&value)
        .map_err(|e| e.to_string())
}

/// Pushes to the PC shortly after a change, without making the UI wait.
fn push_soon<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    std::thread::spawn(move || {
        let Ok((dir, tz_id)) = device(&app) else {
            return;
        };
        let raw = bridge::handle_event_json(&dir, &tz_id, now_ms(), r#"{"type":"syncOnly"}"#);
        // A failed push is not an error here: the next alarm or app start retries.
        if let Ok(response) = serde_json::from_str::<serde_json::Value>(&raw) {
            if response["ok"] == true {
                let _ = app.dun_android().apply_plan(&response["plan"]);
                let _ = snapshot_of(&app).map(|s| app.emit(STATE_CHANGED, s));
            }
        }
    });
}

fn snapshot_of<R: Runtime>(app: &AppHandle<R>) -> CmdResult<serde_json::Value> {
    let (dir, tz_id) = device(app)?;
    let tz = core::zone(&tz_id);
    let now = Timestamp(now_ms());
    core::with_engine(&dir, |engine| {
        serde_json::to_value(snapshot::build(
            engine.state(),
            engine.scheduler(),
            engine.device_id(),
            now,
            &tz,
        ))
        .unwrap_or(serde_json::Value::Null)
    })
}

/// Runs a change, re-plans, tells the UI, and starts a push.
fn mutate<R: Runtime, T>(
    app: &AppHandle<R>,
    f: impl FnOnce(&mut Engine, Timestamp, &TimeZone) -> Result<T, EngineError>,
) -> CmdResult<T> {
    let (dir, tz_id) = device(app)?;
    let tz = core::zone(&tz_id);
    let now = Timestamp(now_ms());
    let out = core::with_engine(&dir, |engine| f(engine, now, &tz))?.map_err(|e| e.to_string())?;
    replan(app, &dir, &tz_id)?;
    let _ = snapshot_of(app).map(|s| app.emit(STATE_CHANGED, s));
    push_soon(app);
    Ok(out)
}

#[tauri::command]
pub fn get_snapshot<R: Runtime>(app: AppHandle<R>) -> CmdResult<serde_json::Value> {
    snapshot_of(&app)
}

#[tauri::command]
pub fn create_item<R: Runtime>(app: AppHandle<R>, draft: ItemDraft) -> CmdResult<String> {
    mutate(&app, |e, now, _| e.create_item(now, draft))
}

#[tauri::command]
pub fn preview_schedule<R: Runtime>(
    app: AppHandle<R>,
    draft: ItemDraft,
) -> CmdResult<Vec<Timestamp>> {
    let (_, tz_id) = device(&app)?;
    dun_core::actions::preview(&draft, Timestamp(now_ms()), &core::zone(&tz_id), 3)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_item<R: Runtime>(app: AppHandle<R>, id: String, draft: ItemDraft) -> CmdResult<()> {
    mutate(&app, |e, now, _| e.update_item(now, &id, draft))
}

#[tauri::command]
pub fn delete_item<R: Runtime>(app: AppHandle<R>, id: String, deleted: bool) -> CmdResult<()> {
    mutate(&app, |e, now, _| e.set_deleted(now, &id, deleted))
}

#[tauri::command]
pub fn mark_done<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    occurrence: Option<Timestamp>,
) -> CmdResult<()> {
    mutate(&app, |e, now, tz| e.done(now, tz, &id, occurrence))
}

#[tauri::command]
pub fn snooze<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    occurrence: Option<Timestamp>,
    minutes: u32,
) -> CmdResult<()> {
    mutate(&app, |e, now, tz| {
        e.snooze(now, tz, &id, occurrence, minutes)
    })
}

#[tauri::command]
pub fn snooze_all<R: Runtime>(app: AppHandle<R>, ids: Vec<String>, minutes: u32) -> CmdResult<()> {
    mutate(&app, |e, now, tz| e.snooze_many(now, tz, &ids, minutes))
}

#[tauri::command]
pub fn undo<R: Runtime>(app: AppHandle<R>, history_id: String) -> CmdResult<()> {
    mutate(&app, |e, now, _| e.undo(now, &history_id))
}

#[tauri::command]
pub fn get_history<R: Runtime>(
    app: AppHandle<R>,
    item_id: Option<String>,
    before: Option<Timestamp>,
    limit: Option<usize>,
) -> CmdResult<Vec<HistoryRow>> {
    let (dir, _) = device(&app)?;
    core::with_engine(&dir, |engine| {
        engine
            .history(item_id.as_deref(), before, limit.unwrap_or(100).min(1_000))
            .map_err(|e| e.to_string())
    })?
}

#[tauri::command]
pub fn timer_action<R: Runtime>(app: AppHandle<R>, id: String, action: String) -> CmdResult<()> {
    mutate(&app, |e, now, _| match action.as_str() {
        "start" => e.start_timer(now, &id),
        "pause" => e.pause_timer(now, &id),
        "resume" => e.resume_timer(now, &id),
        "reset" => e.reset_timer(now, &id),
        _ => Err(dun_core::actions::ActionError::NotATimer.into()),
    })
}

#[tauri::command]
pub fn save_preset<R: Runtime>(
    app: AppHandle<R>,
    id: Option<String>,
    draft: PresetDraft,
) -> CmdResult<String> {
    mutate(&app, |e, now, _| e.save_preset(now, id.as_deref(), draft))
}

#[tauri::command]
pub fn delete_preset<R: Runtime>(app: AppHandle<R>, id: String) -> CmdResult<()> {
    mutate(&app, |e, now, _| e.delete_preset(now, &id))
}

#[tauri::command]
pub fn start_preset<R: Runtime>(app: AppHandle<R>, id: String) -> CmdResult<String> {
    mutate(&app, |e, now, _| e.start_preset(now, &id))
}

#[tauri::command]
pub fn save_tag<R: Runtime>(
    app: AppHandle<R>,
    id: Option<String>,
    name: String,
    color: String,
    order: i64,
) -> CmdResult<String> {
    mutate(&app, |e, now, _| {
        e.save_tag(now, id.as_deref(), &name, &color, order)
    })
}

#[tauri::command]
pub fn delete_tag<R: Runtime>(app: AppHandle<R>, id: String) -> CmdResult<()> {
    mutate(&app, |e, now, _| e.delete_tag(now, &id))
}

#[tauri::command]
pub fn set_setting<R: Runtime>(
    app: AppHandle<R>,
    key: String,
    value: serde_json::Value,
) -> CmdResult<()> {
    let known = [
        setting_key::QUIET_HOURS,
        setting_key::NAG_DEFAULT,
        setting_key::DATE_ONLY_TIME,
        setting_key::MISSED_SUMMARY_THRESHOLD,
        setting_key::HANDOFF,
    ];
    if !known.contains(&key.as_str()) {
        return Err(format!("unknown setting '{key}'"));
    }
    mutate(&app, |e, now, _| e.set_setting(now, &key, value))
}

#[tauri::command]
pub fn mute<R: Runtime>(
    app: AppHandle<R>,
    minutes: Option<u32>,
    until_tomorrow: bool,
) -> CmdResult<()> {
    mutate(&app, |e, now, tz| match (minutes, until_tomorrow) {
        (_, true) => e.mute_until_tomorrow(now, tz),
        (Some(m), false) => e.mute_for(now, m),
        (None, false) => e.unmute(now),
    })
}

/// This phone's own chime, used when an item doesn't name one.
#[tauri::command]
pub fn set_chime<R: Runtime>(app: AppHandle<R>, chime: dun_core::model::ChimeRef) -> CmdResult<()> {
    let (dir, tz_id) = device(&app)?;
    core::with_engine(&dir, |engine| {
        engine
            .store_mut()
            .local_set("chime", &chime)
            .map_err(|e| e.to_string())
    })??;
    replan(&app, &dir, &tz_id)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneSyncStatus {
    pub paired: bool,
    pub pc_name: Option<String>,
    pub last_seen_at: Option<Timestamp>,
    pub addrs: Vec<String>,
    pub chime: dun_core::model::ChimeRef,
    /// How far this phone's clock was from the PC's at the last check-in, when
    /// they disagreed enough to matter.
    pub skew_ms: Option<i64>,
}

#[tauri::command]
pub fn sync_status<R: Runtime>(app: AppHandle<R>) -> CmdResult<PhoneSyncStatus> {
    let (dir, _) = device(&app)?;
    core::with_engine(&dir, |engine| {
        let peer = sync::paired_pc(&engine.peers());
        PhoneSyncStatus {
            paired: peer.is_some(),
            pc_name: peer.as_ref().map(|p| p.name.clone()),
            last_seen_at: peer.as_ref().and_then(|p| p.last_seen_at),
            addrs: peer.map(|p| p.addrs).unwrap_or_default(),
            chime: engine
                .store()
                .local_get("chime")
                .ok()
                .flatten()
                .unwrap_or(dun_core::model::ChimeRef::Bundled { id: "bell".into() }),
            skew_ms: engine
                .store()
                .local_get(sync::SKEW_KEY)
                .ok()
                .flatten()
                .flatten(),
        }
    })
}

/// Syncs now (the pull-to-refresh path), returning what the PC said.
#[tauri::command]
pub fn sync_now<R: Runtime>(app: AppHandle<R>) -> CmdResult<PhoneSyncStatus> {
    let (dir, tz_id) = device(&app)?;
    let raw = bridge::handle_event_json(&dir, &tz_id, now_ms(), r#"{"type":"syncOnly"}"#);
    let response: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if response["ok"] != true {
        return Err(response["error"]
            .as_str()
            .unwrap_or("sync failed")
            .to_string());
    }
    let _ = app.dun_android().apply_plan(&response["plan"]);
    let _ = snapshot_of(&app).map(|s| app.emit(STATE_CHANGED, s));
    sync_status(app)
}

/// Pairs with a PC from a scanned or pasted `dun://pair?…` invite.
#[tauri::command]
pub fn pair_with_pc<R: Runtime>(
    app: AppHandle<R>,
    invite: String,
    name: String,
) -> CmdResult<String> {
    let invite = dun_sync::pairing::PairingInvite::parse(&invite)
        .ok_or("That doesn't look like a Dun pairing code")?;
    let (dir, tz_id) = device(&app)?;
    let device_id = core::with_engine(&dir, |engine| engine.device_id().to_string())?;
    let request = invite.request(&device_id, &name);

    // Try the PC's addresses in turn; the first that answers wins.
    let mut last_error = "no addresses to try".to_string();
    let mut paired = None;
    for addr in &invite.addrs {
        let attempt = sync::runtime().block_on(dun_sync::client::pair(
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
    let Some((addr, response)) = paired else {
        return Err(last_error);
    };

    let now = Timestamp(now_ms());
    core::with_engine(&dir, |engine| {
        let mut peers = engine.peers();
        let mut peer = Peer::new(&response.pc_device_id, &response.pc_name, now);
        peer.token = Some(response.token.clone());
        peer.fingerprint = Some(invite.fingerprint.clone());
        peer.addrs = invite.addrs.clone();
        peer.port = invite.port;
        peer.last_ok_addr = Some(addr);
        peers.upsert(peer);
        engine.save_peers(&peers).map_err(|e| e.to_string())
    })??;

    // First sync straight away, so the phone fills up with the PC's items.
    let raw = bridge::handle_event_json(&dir, &tz_id, now_ms(), r#"{"type":"syncOnly"}"#);
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
        let _ = app.dun_android().apply_plan(&value["plan"]);
    }
    let _ = snapshot_of(&app).map(|s| app.emit(STATE_CHANGED, s));
    Ok(response.pc_name)
}

#[tauri::command]
pub fn forget_pc<R: Runtime>(app: AppHandle<R>) -> CmdResult<()> {
    let (dir, tz_id) = device(&app)?;
    core::with_engine(&dir, |engine| {
        let mut peers = engine.peers();
        let ids: Vec<String> = peers.iter().map(|p| p.device_id.clone()).collect();
        for id in ids {
            peers.remove(&id);
        }
        engine.save_peers(&peers).map_err(|e| e.to_string())
    })??;
    replan(&app, &dir, &tz_id)
}

/// Permissions and battery settings the phone needs, from Android.
#[tauri::command]
pub fn setup_status<R: Runtime>(app: AppHandle<R>) -> CmdResult<serde_json::Value> {
    app.dun_android().setup_status().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_setting<R: Runtime>(app: AppHandle<R>, key: String) -> CmdResult<()> {
    app.dun_android()
        .open_setting(&key)
        .map_err(|e| e.to_string())
}
