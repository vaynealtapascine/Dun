pub mod mobile;

#[cfg(target_os = "android")]
mod android_jni;

mod spike;

#[tauri::command]
fn app_version() -> String {
    format!("v{}", env!("CARGO_PKG_VERSION"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dun_android::init())
        .invoke_handler(tauri::generate_handler![
            app_version,
            spike::spike_app_started,
            spike::spike_start,
            spike::spike_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Dun");
}
