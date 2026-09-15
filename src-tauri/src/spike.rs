//! M1 spike commands: drive the Android test ring from the app window. Removed
//! once the real scheduler owns Android alarms (M10).

use tauri::{AppHandle, Runtime};

#[cfg(target_os = "android")]
mod imp {
    use tauri::{AppHandle, Runtime};
    use tauri_plugin_dun_android::DunAndroidExt;

    use crate::mobile::bridge;

    fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    /// Runs one event through the same bridge the receivers use, then asks
    /// Kotlin to apply the plan.
    pub fn dispatch<R: Runtime>(
        app: &AppHandle<R>,
        event: serde_json::Value,
    ) -> Result<String, String> {
        let plugin = app.dun_android();
        let info = plugin.device_info().map_err(|e| e.to_string())?;
        let raw =
            bridge::handle_event_json(&info.data_dir, &info.tz_id, now_ms(), &event.to_string());
        let response: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        if response["ok"] != true {
            return Err(response["error"]
                .as_str()
                .unwrap_or("unknown error")
                .to_string());
        }
        plugin
            .apply_plan(&response["plan"])
            .map_err(|e| e.to_string())?;
        Ok(response["plan"]["log"]
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    pub fn status<R: Runtime>(app: &AppHandle<R>) -> Result<serde_json::Value, String> {
        let plugin = app.dun_android();
        let info = plugin.device_info().map_err(|e| e.to_string())?;
        let setup = plugin.setup_status().map_err(|e| e.to_string())?;
        let log = bridge::recent_log(&info.data_dir, 20)?;
        Ok(serde_json::json!({ "device": info, "setup": setup, "log": log }))
    }
}

#[cfg(not(target_os = "android"))]
mod imp {
    use tauri::{AppHandle, Runtime};

    const ANDROID_ONLY: &str = "the alarm spike runs on Android only";

    pub fn dispatch<R: Runtime>(_: &AppHandle<R>, _: serde_json::Value) -> Result<String, String> {
        Err(ANDROID_ONLY.into())
    }

    pub fn status<R: Runtime>(_: &AppHandle<R>) -> Result<serde_json::Value, String> {
        Err(ANDROID_ONLY.into())
    }
}

#[tauri::command]
pub async fn spike_app_started<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    imp::dispatch(&app, serde_json::json!({ "type": "appStarted" }))
}

#[tauri::command]
pub async fn spike_start<R: Runtime>(
    app: AppHandle<R>,
    delay_seconds: u32,
) -> Result<String, String> {
    imp::dispatch(
        &app,
        serde_json::json!({ "type": "spikeStart", "delayMs": i64::from(delay_seconds) * 1000 }),
    )
}

#[tauri::command]
pub async fn spike_status<R: Runtime>(app: AppHandle<R>) -> Result<serde_json::Value, String> {
    imp::status(&app)
}
