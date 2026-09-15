//! Bridge from the Dun app to its Kotlin Android plugin.
//!
//! Background work (alarms firing, notification buttons, boot) never goes
//! through this crate: Kotlin receivers call the app's JNI exports directly.
//! This crate covers the in-app direction, where Rust asks Kotlin to apply a
//! plan (post/cancel notifications, set the next alarm) or report setup state.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tauri::{
    plugin::{Builder, PluginApi, TauriPlugin},
    AppHandle, Manager, Runtime,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[cfg(mobile)]
    #[error(transparent)]
    PluginInvoke(#[from] tauri::plugin::mobile::PluginInvokeError),
    #[error("the Dun Android plugin is only available on Android")]
    Unsupported,
}

impl Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Paths and identity the Kotlin side owns. `data_dir` is `Context.dataDir`,
/// the same directory the background receiver passes to JNI, so the app and
/// the receiver always open the same database.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub data_dir: String,
    pub tz_id: String,
    pub sdk_int: i32,
    pub manufacturer: String,
    pub model: String,
}

pub struct DunAndroid<R: Runtime> {
    #[cfg(target_os = "android")]
    handle: tauri::plugin::PluginHandle<R>,
    #[cfg(not(target_os = "android"))]
    _marker: std::marker::PhantomData<fn() -> R>,
}

impl<R: Runtime> DunAndroid<R> {
    /// Hands a plan (JSON produced by the app's bridge) to Kotlin's PlanExecutor.
    pub fn apply_plan(&self, plan: &serde_json::Value) -> Result<()> {
        self.run::<serde_json::Value>("applyPlan", plan).map(|_| ())
    }

    pub fn device_info(&self) -> Result<DeviceInfo> {
        self.run("deviceInfo", &serde_json::json!({}))
    }

    /// Live status for the setup checklist (notification permission, exact
    /// alarms, battery optimisation...), as a JSON object.
    pub fn setup_status(&self) -> Result<serde_json::Value> {
        self.run("setupStatus", &serde_json::json!({}))
    }

    /// Opens the system settings screen for one checklist entry.
    pub fn open_setting(&self, key: &str) -> Result<()> {
        self.run::<serde_json::Value>("openSetting", &serde_json::json!({ "key": key }))
            .map(|_| ())
    }

    #[cfg(target_os = "android")]
    fn run<T: DeserializeOwned>(&self, command: &str, payload: &serde_json::Value) -> Result<T> {
        Ok(self.handle.run_mobile_plugin(command, payload.clone())?)
    }

    #[cfg(not(target_os = "android"))]
    fn run<T: DeserializeOwned>(&self, _command: &str, _payload: &serde_json::Value) -> Result<T> {
        Err(Error::Unsupported)
    }
}

pub trait DunAndroidExt<R: Runtime> {
    fn dun_android(&self) -> &DunAndroid<R>;
}

impl<R: Runtime, T: Manager<R>> DunAndroidExt<R> for T {
    fn dun_android(&self) -> &DunAndroid<R> {
        self.state::<DunAndroid<R>>().inner()
    }
}

fn register<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> Result<DunAndroid<R>> {
    #[cfg(target_os = "android")]
    {
        let handle = _api.register_android_plugin("app.dun.android", "DunPlugin")?;
        Ok(DunAndroid { handle })
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(DunAndroid {
            _marker: std::marker::PhantomData,
        })
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("dun-android")
        .setup(|app, api| {
            let plugin = register(app, api)?;
            app.manage(plugin);
            Ok(())
        })
        .build()
}
