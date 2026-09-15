//! Main window behaviour: hide instead of closing, start hidden when launched
//! at login or by a toast click, and follow the chosen theme.

use tauri::{AppHandle, Manager, Runtime, Theme, WebviewWindow, WindowEvent};

/// Launch arguments that mean "run in the background".
pub const HIDDEN_ARGS: &[&str] = &["--hidden", "-ToastActivated", "-Embedding"];

pub fn launched_hidden(args: &[String]) -> bool {
    args.iter()
        .any(|a| HIDDEN_ARGS.iter().any(|h| a.eq_ignore_ascii_case(h)))
}

/// `dun.exe --quick-add` opens the quick-add bar (in the running Dun if there is one),
/// so it can be bound to a Start-menu shortcut, AutoHotkey, a Stream Deck and so on.
pub fn wants_quick_add(args: &[String]) -> bool {
    args.iter().any(|a| a.eq_ignore_ascii_case("--quick-add"))
}

pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Closing the window hides it; Dun keeps running in the tray.
pub fn keep_alive_on_close<R: Runtime>(window: &WebviewWindow<R>) {
    let w = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = w.hide();
        }
    });
}

pub fn apply_theme<R: Runtime>(app: &AppHandle<R>, theme: &str) {
    app.set_theme(match theme {
        "light" => Some(Theme::Light),
        "dark" => Some(Theme::Dark),
        _ => None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_launch_detection() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(launched_hidden(&args(&["dun.exe", "--hidden"])));
        assert!(launched_hidden(&args(&[
            "dun.exe",
            "-ToastActivated",
            "-Embedding"
        ])));
        assert!(!launched_hidden(&args(&["dun.exe"])));
    }

    #[test]
    fn quick_add_launch_detection() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(wants_quick_add(&args(&["dun.exe", "--Quick-Add"])));
        assert!(!wants_quick_add(&args(&["dun.exe", "--hidden"])));
    }
}
