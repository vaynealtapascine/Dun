//! Tray icon: menu (open, quick add, mute, quit), a tooltip with what's next, and a red
//! badge on the icon while anything is ringing.

use dun_core::scheduler::when;
use dun_core::time::Timestamp;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use super::scheduler_loop::TrayStatus;
use super::window;
use crate::app_core::AppCore;

const TRAY_ID: &str = "main";
const ICON: &[u8] = include_bytes!("../../icons/32x32.png");
/// Windows truncates tray tooltips beyond this.
const TOOLTIP_MAX: usize = 127;

pub fn create<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Dun", true, None::<&str>)?;
    let quick = MenuItem::with_id(app, "quickadd", "Quick add…", true, None::<&str>)?;
    let mute_15 = MenuItem::with_id(app, "mute:15", "15 minutes", true, None::<&str>)?;
    let mute_60 = MenuItem::with_id(app, "mute:60", "1 hour", true, None::<&str>)?;
    let mute_tomorrow =
        MenuItem::with_id(app, "mute:tomorrow", "Until tomorrow", true, None::<&str>)?;
    let unmute = MenuItem::with_id(app, "mute:off", "Unmute", true, None::<&str>)?;
    let mute = Submenu::with_items(
        app,
        "Mute",
        true,
        &[
            &mute_15,
            &mute_60,
            &mute_tomorrow,
            &PredefinedMenuItem::separator(app)?,
            &unmute,
        ],
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit Dun", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &quick,
            &PredefinedMenuItem::separator(app)?,
            &mute,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(ICON)?)
        .tooltip("Dun")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id.as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                window::show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn on_menu<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        "open" => window::show_main(app),
        "quickadd" => super::quickadd::show(app),
        "quit" => app.exit(0),
        mute if mute.starts_with("mute:") => {
            let Some(core) = app.try_state::<std::sync::Arc<AppCore>>() else {
                return;
            };
            let (now, tz) = (core.now(), core.tz());
            let result = {
                let mut engine = core.engine();
                match &mute[5..] {
                    "15" => engine.mute_for(now, 15),
                    "60" => engine.mute_for(now, 60),
                    "tomorrow" => engine.mute_until_tomorrow(now, &tz),
                    _ => engine.unmute(now),
                }
            };
            match result {
                Ok(()) => crate::commands::changed(app, &core),
                Err(e) => eprintln!("tray: mute failed: {e}"),
            }
        }
        _ => {}
    }
}

/// Applies the scheduler's latest tray status.
pub fn update<R: Runtime>(
    app: &AppHandle<R>,
    status: &TrayStatus,
    now: Timestamp,
    tz: &dun_core::time::TimeZone,
) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let _ = tray.set_tooltip(Some(tooltip(status, now, tz)));
    let icon = Image::from_bytes(ICON).map(|img| {
        if status.ringing > 0 {
            Image::new_owned(
                badge(img.rgba(), img.width(), img.height()),
                img.width(),
                img.height(),
            )
        } else {
            img.to_owned()
        }
    });
    if let Ok(icon) = icon {
        let _ = tray.set_icon(Some(icon));
    }
}

pub fn tooltip(status: &TrayStatus, now: Timestamp, tz: &dun_core::time::TimeZone) -> String {
    let mut parts = Vec::new();
    if status.ringing > 0 {
        parts.push(format!("{} ringing", status.ringing));
    }
    if let Some(until) = status.muted_until {
        parts.push(format!("muted until {}", when(until, now, tz)));
    }
    if let Some((title, at)) = &status.next {
        parts.push(format!("next: {title} {}", when(*at, now, tz)));
    }
    let text = if parts.is_empty() {
        "Dun · nothing scheduled".to_string()
    } else {
        format!("Dun · {}", parts.join(" · "))
    };
    if text.chars().count() > TOOLTIP_MAX {
        let cut: String = text.chars().take(TOOLTIP_MAX - 1).collect();
        format!("{cut}…")
    } else {
        text
    }
}

/// Draws a red dot with a white ring in the bottom-right corner.
pub fn badge(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut out = rgba.to_vec();
    let (w, h) = (width as f32, height as f32);
    let r = w.min(h) * 0.26;
    let (cx, cy) = (w - r - 0.5, h - r - 0.5);
    for y in 0..height {
        for x in 0..width {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let color = if d <= r - 1.5 {
                [0xE0, 0x2E, 0x2E, 0xFF]
            } else if d <= r {
                [0xFF, 0xFF, 0xFF, 0xFF]
            } else {
                continue;
            };
            let i = ((y * width + x) * 4) as usize;
            out[i..i + 4].copy_from_slice(&color);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use dun_core::time::{resolve, TimeZone, MINUTE};
    use jiff::civil::date;

    #[test]
    fn tooltip_summarises_and_truncates() {
        let tz = TimeZone::UTC;
        let now = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &tz);
        assert_eq!(
            tooltip(&TrayStatus::default(), now, &tz),
            "Dun · nothing scheduled"
        );
        let status = TrayStatus {
            ringing: 2,
            next: Some(("Pay rent".into(), now.plus(90 * MINUTE))),
            muted_until: Some(now.plus(15 * MINUTE)),
        };
        assert_eq!(
            tooltip(&status, now, &tz),
            "Dun · 2 ringing · muted until 9:15 AM · next: Pay rent 10:30 AM"
        );
        let long = TrayStatus {
            next: Some(("x".repeat(300), now)),
            ..TrayStatus::default()
        };
        assert_eq!(tooltip(&long, now, &tz).chars().count(), TOOLTIP_MAX);
    }

    #[test]
    fn badge_paints_only_the_corner() {
        let icon = Image::from_bytes(ICON).unwrap();
        let (w, h) = (icon.width(), icon.height());
        let badged = badge(icon.rgba(), w, h);
        let px = |buf: &[u8], x: u32, y: u32| buf[((y * w + x) * 4) as usize..][..4].to_vec();
        assert_eq!(
            px(&badged, 0, 0),
            px(icon.rgba(), 0, 0),
            "top-left untouched"
        );
        assert_eq!(
            px(&badged, w - 5, h - 5),
            vec![0xE0, 0x2E, 0x2E, 0xFF],
            "red in the corner"
        );
    }
}
