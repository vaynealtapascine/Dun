//! The quick-add window: a small always-on-top bar shown by the global
//! shortcut or the tray, on the monitor under the cursor, and hidden again on
//! Esc, after adding, or when it loses focus.

use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Runtime, WebviewWindow, WindowEvent,
};

pub const LABEL: &str = "quickadd";
/// Logical width, matching `tauri.conf.json`.
const WIDTH: f64 = 560.0;
const MAX_HEIGHT: f64 = 400.0;

/// Hides the window when the user switches away from it.
pub fn setup<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let w = window.clone();
    window.on_window_event(move |event| match event {
        // On Windows `Focused(false)` comes from WebView2's LostFocus, which also
        // fires while focus moves between the webview's own child windows (seen
        // when text is entered). Only hide once another window is really in front.
        WindowEvent::Focused(false) => {
            let w = w.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(150));
                if w.is_visible().unwrap_or(false) && !is_foreground(&w) {
                    let _ = w.hide();
                }
            });
        }
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let _ = w.hide();
        }
        _ => {}
    });
}

pub fn toggle<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    // Not `is_focused()`: that tracks the top-level window's own keyboard focus,
    // which it gives up to the webview as soon as the bar is in use.
    if window.is_visible().unwrap_or(false) && is_foreground(&window) {
        let _ = window.hide();
    } else {
        show(app);
    }
}

pub fn show<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    place_near_cursor(app, &window);
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit_to(LABEL, "quickadd-shown", ());
}

pub fn hide<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

/// Sizes the window to its content (logical pixels, from the page).
pub fn fit<R: Runtime>(app: &AppHandle<R>, height: f64) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let height = height.clamp(40.0, MAX_HEIGHT);
        let _ = window.set_size(LogicalSize::new(WIDTH, height));
    }
}

/// Centres the bar horizontally, a quarter of the way down the work area of
/// the monitor the cursor is on.
fn place_near_cursor<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|c| app.monitor_from_point(c.x, c.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        let _ = window.center();
        return;
    };
    let area = monitor.work_area();
    let width = (WIDTH * monitor.scale_factor()).round() as i32;
    let (x, y) = centred(
        area.position.x,
        area.position.y,
        area.size.width as i32,
        area.size.height as i32,
        width,
    );
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

#[cfg(windows)]
fn is_foreground<R: Runtime>(window: &WebviewWindow<R>) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetAncestor, GetForegroundWindow, GA_ROOTOWNER};
    let Ok(hwnd) = window.hwnd() else {
        return false;
    };
    let foreground = unsafe { GetForegroundWindow() };
    !foreground.is_invalid()
        && (foreground == hwnd || unsafe { GetAncestor(foreground, GA_ROOTOWNER) } == hwnd)
}

#[cfg(not(windows))]
fn is_foreground<R: Runtime>(window: &WebviewWindow<R>) -> bool {
    window.is_focused().unwrap_or(false)
}

fn centred(area_x: i32, area_y: i32, area_w: i32, area_h: i32, width: i32) -> (i32, i32) {
    (area_x + (area_w - width).max(0) / 2, area_y + area_h / 4)
}

#[cfg(test)]
mod tests {
    #[test]
    fn centres_on_the_given_work_area() {
        // A 1920×1040 work area to the left of the primary monitor.
        assert_eq!(super::centred(-1920, 0, 1920, 1040, 1120), (-1520, 260));
        // Narrower than the bar: pin to the left edge.
        assert_eq!(super::centred(0, 40, 800, 600, 1120), (0, 190));
    }
}
