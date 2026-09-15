//! The global quick-add shortcut (default Ctrl+Alt+N, rebindable).

use std::sync::Mutex;

use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use super::quickadd;

/// Why the configured shortcut isn't active, if it isn't (shown in Settings).
#[derive(Default)]
pub struct HotkeyStatus(pub Mutex<Option<String>>);

pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                quickadd::toggle(app);
            }
        })
        .build()
}

/// Parses an accelerator such as `CommandOrControl+Alt+N`. Empty means "no
/// shortcut". A shortcut without a modifier is refused: it would swallow
/// that key in every app.
pub fn parse(accelerator: &str) -> Result<Option<Shortcut>, String> {
    let a = accelerator.trim();
    if a.is_empty() {
        return Ok(None);
    }
    let shortcut: Shortcut = a
        .parse()
        .map_err(|e| format!("“{a}” isn't a shortcut Dun understands ({e})"))?;
    if shortcut.mods.is_empty() {
        return Err("A shortcut needs Ctrl, Alt, Shift or Win as well as a key".into());
    }
    Ok(Some(shortcut))
}

/// Swaps the registered shortcut from `previous` to `next`. If `next` can't be
/// registered (usually another app owns it) the previous one is restored and
/// the error returned, so a failed change never leaves Dun without one.
pub fn apply<R: Runtime>(app: &AppHandle<R>, previous: &str, next: &str) -> Result<(), String> {
    let next_shortcut = parse(next)?;
    let previous_shortcut = parse(previous).ok().flatten();
    let gs = app.global_shortcut();

    if next_shortcut.is_some() && next_shortcut == previous_shortcut {
        if next_shortcut.is_some_and(|s| gs.is_registered(s)) {
            return Ok(());
        }
    } else if let Some(p) = previous_shortcut {
        let _ = gs.unregister(p);
    }

    if let Some(n) = next_shortcut {
        if let Err(e) = gs.register(n) {
            if let Some(p) = previous_shortcut.filter(|p| *p != n) {
                let _ = gs.register(p);
            }
            return Err(format!(
                "{} is already used by another app ({e})",
                next.trim()
            ));
        }
    }
    Ok(())
}

/// Registers the saved shortcut at startup, remembering any failure.
pub fn register_saved<R: Runtime>(app: &AppHandle<R>, accelerator: &str) {
    let result = apply(app, "", accelerator);
    if let Err(e) = &result {
        eprintln!("hotkey: {e}");
    }
    set_status(app, result.err());
}

pub fn set_status<R: Runtime>(app: &AppHandle<R>, error: Option<String>) {
    if let Some(status) = app.try_state::<HotkeyStatus>() {
        *status.0.lock().unwrap_or_else(|p| p.into_inner()) = error;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_accelerators() {
        let default = parse("CommandOrControl+Alt+N").unwrap().unwrap();
        assert_eq!(default, "Ctrl+Alt+KeyN".parse::<Shortcut>().unwrap());
        assert!(parse("Ctrl+Shift+Space").unwrap().is_some());
        assert!(parse("Alt+F1").unwrap().is_some());
        assert_eq!(parse("  ").unwrap(), None);
    }

    #[test]
    fn refuses_bare_keys_and_nonsense() {
        assert!(parse("N").unwrap_err().contains("needs Ctrl"));
        assert!(parse("Ctrl+Banana").is_err());
    }
}
