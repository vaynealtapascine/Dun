//! Turns scheduler effects into Windows toasts, and toast clicks into messages
//! for the scheduler loop.

use std::path::PathBuf;
use std::sync::mpsc::Sender;

use dun_core::scheduler::{AlertLabel, AlertLevel};
use dun_core::time::{TimeZone, Timestamp};

use super::scheduler_loop::Msg;
use super::toast::xml::{Button, Toast, ToastAction};
use super::toast::Identity;

pub const RING_GROUP: &str = "ring";
pub const SUMMARY_TAG: &str = "summary";

/// Snooze lengths on the PC toast, in minutes.
pub const SNOOZE_BUTTONS: [u32; 3] = [1, 5, 15];

/// `%APPDATA%\app.dun`, where Tauri's app_data_dir points on Windows.
/// Computed without Tauri so registration can happen before the app starts.
fn early_data_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("app.dun")
}

/// Registers the toast identity and COM activator. Must run before the Tauri
/// builder: when a click launched this process, the activation is delivered
/// as soon as the class object is registered.
#[cfg(windows)]
pub fn init(tx: Sender<Msg>) {
    let dir = early_data_dir();
    let icon = dir.join("toast-icon.png");
    let _ = std::fs::create_dir_all(&dir);
    if !icon.exists() {
        let _ = std::fs::write(&icon, include_bytes!("../../icons/128x128@2x.png"));
    }
    let tx = std::sync::Mutex::new(tx);
    let sink = std::sync::Arc::new(move |args: String| {
        let _ = tx
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .send(Msg::Toast(args));
    });
    if let Err(e) = super::toast::init(Identity::current(), &icon, sink) {
        eprintln!("toast registration failed: {e}");
    }
}

pub fn ring_toast(
    item_id: &str,
    occurrence: Timestamp,
    title: &str,
    notes: &str,
    label: &AlertLabel,
    now: Timestamp,
    tz: &TimeZone,
) -> Toast {
    let mut buttons = vec![Button {
        label: "Done".into(),
        action: ToastAction::Done {
            item: item_id.into(),
            occ: occurrence.0,
        },
    }];
    buttons.extend(SNOOZE_BUTTONS.iter().map(|m| Button {
        label: format!("+{m}m"),
        action: ToastAction::Snooze {
            item: item_id.into(),
            occ: occurrence.0,
            minutes: *m,
        },
    }));
    let first_note_line: String = notes
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(140)
        .collect();
    Toast {
        tag: item_id.into(),
        group: RING_GROUP.into(),
        title: title.into(),
        lines: vec![label.describe(now, tz), first_note_line],
        launch: ToastAction::Open {
            item: Some(item_id.into()),
        },
        buttons,
        reminder: true,
    }
}

pub fn summary_toast(items: &[(String, String)]) -> Toast {
    let names: Vec<&str> = items.iter().map(|(_, title)| title.as_str()).collect();
    let mut list = names.iter().take(4).copied().collect::<Vec<_>>().join(", ");
    if names.len() > 4 {
        list.push_str(&format!(" and {} more", names.len() - 4));
    }
    Toast {
        tag: SUMMARY_TAG.into(),
        group: RING_GROUP.into(),
        title: format!("{} missed reminders", items.len()),
        lines: vec![list],
        launch: ToastAction::Open { item: None },
        buttons: vec![
            Button {
                label: "Open Dun".into(),
                action: ToastAction::Open { item: None },
            },
            Button {
                label: "Snooze all 15m".into(),
                action: ToastAction::SnoozeAll { minutes: 15 },
            },
        ],
        reminder: true,
    }
}

/// Shows an alert. A full alert removes any previous toast first so Windows
/// pops a fresh banner even if the user dismissed the last one.
#[cfg(windows)]
pub fn show(toast: &Toast, level: AlertLevel) -> Result<(), String> {
    let id = Identity::current();
    if level == AlertLevel::Full {
        let _ = super::toast::remove(id, &toast.tag, &toast.group);
    }
    super::toast::show(id, toast, level == AlertLevel::Full)
}

#[cfg(windows)]
pub fn clear(tag: &str) {
    let _ = super::toast::remove(Identity::current(), tag, RING_GROUP);
}

#[cfg(not(windows))]
pub fn show(_toast: &Toast, _level: AlertLevel) -> Result<(), String> {
    Ok(())
}

#[cfg(not(windows))]
pub fn clear(_tag: &str) {}

#[cfg(test)]
mod tests {
    use super::*;
    use dun_core::time::{resolve, MINUTE};
    use jiff::civil::date;

    #[test]
    fn ring_toast_has_done_and_three_snoozes_for_the_occurrence() {
        let tz = TimeZone::UTC;
        let due = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &tz);
        let label = AlertLabel {
            due,
            missed_first: None,
            missed_count: 1,
            quiet_held: false,
            snooze_count: 0,
        };
        let t = ring_toast(
            "item-1",
            due,
            "Pay rent",
            "Landlord\nsecond line",
            &label,
            due.plus(MINUTE),
            &tz,
        );
        let labels: Vec<_> = t.buttons.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, ["Done", "+1m", "+5m", "+15m"]);
        assert_eq!(t.lines, ["Due 9:00 AM", "Landlord"]);
        assert!(t.buttons.iter().all(|b| match &b.action {
            ToastAction::Done { occ, .. } | ToastAction::Snooze { occ, .. } => *occ == due.0,
            _ => false,
        }));
        assert!(t.to_xml().contains(r#"scenario="reminder""#));
    }

    #[test]
    fn summary_lists_a_few_titles() {
        let items: Vec<_> = (1..=6)
            .map(|i| (format!("i{i}"), format!("Item {i}")))
            .collect();
        let t = summary_toast(&items);
        assert_eq!(t.title, "6 missed reminders");
        assert_eq!(t.lines[0], "Item 1, Item 2, Item 3, Item 4 and 2 more");
    }
}
