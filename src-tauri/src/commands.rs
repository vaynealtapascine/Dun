//! Tauri commands for the UI. Every mutating command wakes the scheduler and
//! pushes a fresh snapshot to all windows via the `state-changed` event.

use std::sync::Arc;

use dun_core::actions::{ItemDraft, PresetDraft};
use dun_core::engine::Engine;
use dun_core::model::item::setting_key;
use dun_core::model::{HandoffSettings, Nag};
use dun_core::quiet::QuietHours;
use dun_core::sync::HistoryRow;
use dun_core::time::Timestamp;
use serde::de::DeserializeOwned;
use tauri::{AppHandle, Emitter, Runtime, State};

use crate::app_core::{AppCore, LocalSettings};

pub const STATE_CHANGED: &str = "state-changed";

type Core<'a> = State<'a, Arc<AppCore>>;
type CmdResult<T> = Result<T, String>;

/// Runs a mutation with the engine, then notifies the scheduler and the UI.
fn mutate<R: Runtime, T>(
    app: &AppHandle<R>,
    core: &AppCore,
    f: impl FnOnce(
        &mut Engine,
        Timestamp,
        &dun_core::time::TimeZone,
    ) -> Result<T, dun_core::engine::EngineError>,
) -> CmdResult<T> {
    let (now, tz) = (core.now(), core.tz());
    let out = {
        let mut engine = core.engine();
        f(&mut engine, now, &tz).map_err(|e| e.to_string())?
    };
    changed(app, core);
    Ok(out)
}

pub fn changed<R: Runtime>(app: &AppHandle<R>, core: &AppCore) {
    core.wake_scheduler();
    let _ = app.emit(STATE_CHANGED, core.snapshot_json());
}

#[tauri::command]
pub fn get_snapshot(core: Core<'_>) -> serde_json::Value {
    core.snapshot_json()
}

#[tauri::command]
pub fn create_item<R: Runtime>(
    app: AppHandle<R>,
    core: Core<'_>,
    draft: ItemDraft,
) -> CmdResult<String> {
    mutate(&app, &core, |e, now, _| e.create_item(now, draft))
}

#[tauri::command]
pub fn update_item<R: Runtime>(
    app: AppHandle<R>,
    core: Core<'_>,
    id: String,
    draft: ItemDraft,
) -> CmdResult<()> {
    mutate(&app, &core, |e, now, _| e.update_item(now, &id, draft))
}

#[tauri::command]
pub fn delete_item<R: Runtime>(
    app: AppHandle<R>,
    core: Core<'_>,
    id: String,
    deleted: bool,
) -> CmdResult<()> {
    mutate(&app, &core, |e, now, _| e.set_deleted(now, &id, deleted))
}

#[tauri::command]
pub fn mark_done<R: Runtime>(
    app: AppHandle<R>,
    core: Core<'_>,
    id: String,
    occurrence: Option<Timestamp>,
) -> CmdResult<()> {
    mutate(&app, &core, |e, now, tz| e.done(now, tz, &id, occurrence))
}

#[tauri::command]
pub fn snooze<R: Runtime>(
    app: AppHandle<R>,
    core: Core<'_>,
    id: String,
    occurrence: Option<Timestamp>,
    minutes: u32,
) -> CmdResult<()> {
    mutate(&app, &core, |e, now, tz| {
        e.snooze(now, tz, &id, occurrence, minutes)
    })
}

#[tauri::command]
pub fn snooze_all<R: Runtime>(
    app: AppHandle<R>,
    core: Core<'_>,
    ids: Vec<String>,
    minutes: u32,
) -> CmdResult<()> {
    mutate(&app, &core, |e, now, tz| {
        e.snooze_many(now, tz, &ids, minutes)
    })
}

#[tauri::command]
pub fn undo<R: Runtime>(app: AppHandle<R>, core: Core<'_>, history_id: String) -> CmdResult<()> {
    mutate(&app, &core, |e, now, _| e.undo(now, &history_id))
}

#[tauri::command]
pub fn get_history(
    core: Core<'_>,
    item_id: Option<String>,
    before: Option<Timestamp>,
    limit: Option<usize>,
) -> CmdResult<Vec<HistoryRow>> {
    core.engine()
        .history(item_id.as_deref(), before, limit.unwrap_or(100).min(1_000))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn timer_action<R: Runtime>(
    app: AppHandle<R>,
    core: Core<'_>,
    id: String,
    action: String,
) -> CmdResult<()> {
    mutate(&app, &core, |e, now, _| match action.as_str() {
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
    core: Core<'_>,
    id: Option<String>,
    draft: PresetDraft,
) -> CmdResult<String> {
    mutate(&app, &core, |e, now, _| {
        e.save_preset(now, id.as_deref(), draft)
    })
}

#[tauri::command]
pub fn delete_preset<R: Runtime>(app: AppHandle<R>, core: Core<'_>, id: String) -> CmdResult<()> {
    mutate(&app, &core, |e, now, _| e.delete_preset(now, &id))
}

#[tauri::command]
pub fn start_preset<R: Runtime>(
    app: AppHandle<R>,
    core: Core<'_>,
    id: String,
) -> CmdResult<String> {
    mutate(&app, &core, |e, now, _| e.start_preset(now, &id))
}

#[tauri::command]
pub fn save_tag<R: Runtime>(
    app: AppHandle<R>,
    core: Core<'_>,
    id: Option<String>,
    name: String,
    color: String,
    order: i64,
) -> CmdResult<String> {
    mutate(&app, &core, |e, now, _| {
        e.save_tag(now, id.as_deref(), &name, &color, order)
    })
}

#[tauri::command]
pub fn delete_tag<R: Runtime>(app: AppHandle<R>, core: Core<'_>, id: String) -> CmdResult<()> {
    mutate(&app, &core, |e, now, _| e.delete_tag(now, &id))
}

fn parse<T: DeserializeOwned>(value: serde_json::Value) -> CmdResult<T> {
    serde_json::from_value(value).map_err(|e| format!("invalid setting value: {e}"))
}

/// Updates one synced setting. Values are validated by parsing them into the
/// setting's real type before anything is written.
#[tauri::command]
pub fn set_setting<R: Runtime>(
    app: AppHandle<R>,
    core: Core<'_>,
    key: String,
    value: serde_json::Value,
) -> CmdResult<()> {
    let value = match key.as_str() {
        setting_key::QUIET_HOURS => serde_json::to_value(parse::<QuietHours>(value)?),
        setting_key::NAG_DEFAULT => serde_json::to_value(parse::<Nag>(value)?),
        setting_key::DATE_ONLY_TIME => serde_json::to_value(parse::<jiff::civil::Time>(value)?),
        setting_key::MISSED_SUMMARY_THRESHOLD => serde_json::to_value(parse::<u32>(value)?),
        setting_key::HANDOFF => serde_json::to_value(parse::<HandoffSettings>(value)?),
        other => return Err(format!("unknown setting '{other}'")),
    }
    .map_err(|e| e.to_string())?;
    mutate(&app, &core, |e, now, _| e.set_setting(now, &key, value))
}

#[tauri::command]
pub fn mute<R: Runtime>(
    app: AppHandle<R>,
    core: Core<'_>,
    minutes: Option<u32>,
    until_tomorrow: bool,
) -> CmdResult<()> {
    mutate(&app, &core, |e, now, tz| match (minutes, until_tomorrow) {
        (_, true) => e.mute_until_tomorrow(now, tz),
        (Some(m), false) => e.mute_for(now, m),
        (None, false) => e.unmute(now),
    })
}

#[tauri::command]
pub fn get_local_settings(core: Core<'_>) -> LocalSettings {
    core.local_settings()
}

#[tauri::command]
pub fn set_local_settings<R: Runtime>(
    app: AppHandle<R>,
    core: Core<'_>,
    settings: LocalSettings,
) -> CmdResult<()> {
    if !(0.0..=1.0).contains(&settings.volume) {
        return Err("volume must be between 0 and 1".into());
    }
    if !["system", "light", "dark"].contains(&settings.theme.as_str()) {
        return Err("theme must be system, light or dark".into());
    }
    core.set_local_settings(&settings)?;
    let _ = app.emit("local-settings-changed", &settings);
    core.wake_scheduler();
    Ok(())
}
