pub mod mobile;

#[cfg(target_os = "android")]
mod android_jni;

#[cfg(desktop)]
pub mod app_core;
#[cfg(desktop)]
mod commands;
#[cfg(desktop)]
pub mod desktop;

#[cfg(mobile)]
mod spike;

#[tauri::command]
fn app_version() -> String {
    format!("v{}", env!("CARGO_PKG_VERSION"))
}

#[cfg(desktop)]
pub fn run() {
    use std::sync::{Arc, Mutex};
    use tauri::Manager;

    let (tx, rx) = std::sync::mpsc::channel::<desktop::scheduler_loop::Msg>();
    // Register for toast clicks before anything else so a click that launched
    // this process is delivered.
    #[cfg(windows)]
    desktop::toasts::init(tx.clone());

    // `dun.exe --quick-add` from a shortcut usually just hands off to the
    // running Dun. This process was started by the user so it may take the
    // foreground; pass that right on, or the bar would open behind other windows.
    #[cfg(windows)]
    if desktop::window::wants_quick_add(&std::env::args().collect::<Vec<_>>()) {
        use windows::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY};
        let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
    }

    tauri::Builder::default()
        // Must be the first plugin: a second launch just brings Dun forward.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if desktop::window::wants_quick_add(&argv) {
                desktop::quickadd::show(app);
            } else {
                desktop::window::show_main(app);
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(desktop::hotkey::plugin())
        .plugin(tauri_plugin_dun_android::init())
        .manage(desktop::hotkey::HotkeyStatus::default())
        .setup(move |app| {
            let dir = app.path().app_data_dir()?;
            let core = Arc::new(app_core::AppCore::open(&dir)?);
            let wake_tx = Mutex::new(tx);
            core.set_wake(move || {
                let _ = wake_tx
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .send(desktop::scheduler_loop::Msg::Wake);
            });
            app.manage(core.clone());
            if std::env::args().any(|a| a == "--dev-seed")
                || std::env::var_os("DUN_DEV_SEED").is_some()
            {
                core.dev_seed();
            }

            let local = core.local_settings();
            desktop::window::apply_theme(app.handle(), &local.theme);
            desktop::sync_autostart(app.handle(), local.autostart);
            desktop::tray::create(app.handle())?;
            desktop::quickadd::setup(app.handle());
            desktop::hotkey::register_saved(app.handle(), &local.hotkey);
            if let Some(main) = app.get_webview_window("main") {
                desktop::window::keep_alive_on_close(&main);
                let args: Vec<String> = std::env::args().collect();
                if desktop::window::wants_quick_add(&args) {
                    desktop::quickadd::show(app.handle());
                } else if !desktop::window::launched_hidden(&args) {
                    main.show()?;
                }
            }

            let sync = desktop::sync::SyncHub::new(app.handle().clone(), core.clone())?;
            if sync.enabled() {
                if let Err(e) = sync.start() {
                    eprintln!("sync: {e}");
                }
            }
            app.manage(sync.clone());

            let audio = desktop::audio::Audio::start(dir.join("sounds"));
            app.manage(audio.clone());
            let tray = Arc::new(Mutex::new(desktop::scheduler_loop::TrayStatus::default()));
            let tray_core = core.clone();
            desktop::scheduler_loop::spawn(
                app.handle().clone(),
                core,
                audio,
                rx,
                tray,
                sync,
                move |app, status| {
                    desktop::tray::update(app, status, tray_core.now(), &tray_core.tz());
                },
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_version,
            commands::get_snapshot,
            commands::create_item,
            commands::preview_schedule,
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
            commands::list_chimes,
            commands::play_chime,
            commands::hotkey_status,
            commands::quickadd_hide,
            commands::quickadd_fit,
            commands::quickadd_open_form,
            commands::backup_export,
            commands::backup_import,
            commands::import_sound,
            commands::sync_status,
            commands::sync_set_enabled,
            commands::pairing_open,
            commands::pairing_close,
            commands::forget_peer,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Dun");
}

#[cfg(mobile)]
#[tauri::mobile_entry_point]
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
