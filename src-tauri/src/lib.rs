pub mod mobile;

#[cfg(target_os = "android")]
mod android_jni;

#[cfg(desktop)]
pub mod desktop;

mod spike;

#[tauri::command]
fn app_version() -> String {
    format!("v{}", env!("CARGO_PKG_VERSION"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(windows)]
    desktop::toast_spike::init(
        std::env::args()
            .any(|a| a.eq_ignore_ascii_case(desktop::toast::registration::TOAST_ACTIVATED_ARG)),
    );

    let builder = tauri::Builder::default().plugin(tauri_plugin_dun_android::init());

    #[cfg(desktop)]
    let builder = builder
        .setup(|app| {
            desktop::toast_spike::set_app(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_version,
            spike::spike_app_started,
            spike::spike_start,
            spike::spike_status,
            desktop::toast_spike::toast_spike_show,
            desktop::toast_spike::toast_spike_remove,
            desktop::toast_spike::toast_spike_status,
        ]);

    #[cfg(mobile)]
    let builder = builder.invoke_handler(tauri::generate_handler![
        app_version,
        spike::spike_app_started,
        spike::spike_start,
        spike::spike_status,
    ]);

    builder
        .run(tauri::generate_context!())
        .expect("error while running Dun");
}
