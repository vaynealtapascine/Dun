//! Desktop-only integration.

pub mod attended;
pub mod audio;
pub mod firewall;
pub mod hotkey;
pub mod network;
pub mod quickadd;
pub mod scheduler_loop;
pub mod sync;
pub mod toast;
pub mod toasts;
pub mod tray;
pub mod updates;
pub mod window;

use tauri::{AppHandle, Runtime};

/// Makes the login-autostart entry match the setting. Debug builds never
/// register themselves, so a dev binary doesn't start at login.
pub fn sync_autostart<R: Runtime>(app: &AppHandle<R>, enabled: bool) {
    if cfg!(debug_assertions) {
        return;
    }
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    let result = match (enabled, launcher.is_enabled().unwrap_or(false)) {
        (true, false) => launcher.enable(),
        (false, true) => launcher.disable(),
        _ => Ok(()),
    };
    if let Err(e) = result {
        eprintln!("autostart: {e}");
    }
}
