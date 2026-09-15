pub mod mobile;

#[cfg(target_os = "android")]
mod android_jni;

#[cfg(desktop)]
pub mod app_core;
#[cfg(desktop)]
mod commands;
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
            use tauri::Manager;
            let dir = app.path().app_data_dir()?;
            let core = std::sync::Arc::new(app_core::AppCore::open(&dir)?);
            app.manage(core);
            desktop::toast_spike::set_app(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_version,
            commands::get_snapshot,
            commands::create_item,
            commands::update_item,
            commands::delete_item,
            commands::mark_done,
            commands::snooze,
            commands::snooze_all,
            commands::undo,
            commands::get_history,
            commands::timer_action,
            commands::save_preset,
            commands::delete_preset,
            commands::start_preset,
            commands::save_tag,
            commands::delete_tag,
            commands::set_setting,
            commands::mute,
            commands::get_local_settings,
            commands::set_local_settings,
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
