//! M2 spike: prove toast buttons reach Dun while it runs, after it quits and
//! from Action Center. Activations are appended to a log file (not just held in
//! memory) so a cold start caused by a click leaves evidence. Replaced by the
//! real scheduler's toast effects in M6.

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;

use tauri::{AppHandle, Emitter, Runtime, Wry};

use super::toast::xml::{Button, Toast, ToastAction};
use super::toast::Identity;

const SPIKE_TAG: &str = "spike";
const GROUP: &str = "ring";

static APP: OnceLock<AppHandle<Wry>> = OnceLock::new();
static SHOWN: AtomicU32 = AtomicU32::new(0);

/// `%APPDATA%\app.dun`, the same folder Tauri's app_data_dir resolves to on
/// Windows. Computed without Tauri so it works before the builder runs.
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("app.dun")
}

fn log_path() -> PathBuf {
    data_dir().join("toast-spike.log")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn append_log(line: &str) {
    let _ = std::fs::create_dir_all(data_dir());
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
    {
        let _ = writeln!(f, "{}\t{line}", now_ms());
    }
}

/// Registers the toast identity and activator. Called at the very start of
/// `run()` so an activation that launched the process is not lost.
#[cfg(windows)]
pub fn init(launched_by_toast: bool) {
    let id = Identity::current();
    let icon = data_dir().join("toast-icon.png");
    let _ = std::fs::create_dir_all(data_dir());
    if !icon.exists() {
        let _ = std::fs::write(&icon, include_bytes!("../../icons/128x128@2x.png"));
    }
    append_log(&format!(
        "start pid={} aumid={} launched_by_toast={launched_by_toast}",
        std::process::id(),
        id.aumid
    ));
    let sink = std::sync::Arc::new(|args: String| {
        let parsed = ToastAction::parse(&args);
        let line = format!(
            "activated pid={} args={args:?} parsed={parsed:?}",
            std::process::id()
        );
        append_log(&line);
        if let Some(app) = APP.get() {
            let _ = app.emit("toast-spike", line);
        }
    });
    if let Err(e) = super::toast::init(id, &icon, sink) {
        append_log(&format!("init failed: {e}"));
    }
}

pub fn set_app(app: AppHandle<Wry>) {
    let _ = APP.set(app);
}

fn spike_toast(kind: &str) -> Toast {
    let n = SHOWN.fetch_add(1, Ordering::SeqCst) + 1;
    let occ = 1_000; // fixed occurrence so stale-vs-current can be reasoned about in the log
    let item = SPIKE_TAG.to_string();
    let buttons = vec![
        Button {
            label: "Done".into(),
            action: ToastAction::Done {
                item: item.clone(),
                occ,
            },
        },
        Button {
            label: "+1m".into(),
            action: ToastAction::Snooze {
                item: item.clone(),
                occ,
                minutes: 1,
            },
        },
        Button {
            label: "+5m".into(),
            action: ToastAction::Snooze {
                item: item.clone(),
                occ,
                minutes: 5,
            },
        },
        Button {
            label: "+15m".into(),
            action: ToastAction::Snooze {
                item: item.clone(),
                occ,
                minutes: 15,
            },
        },
    ];
    Toast {
        tag: item.clone(),
        group: GROUP.into(),
        title: "Dun test reminder".into(),
        lines: vec![
            format!("{kind} · show #{n}"),
            "Notes line: buttons should reach Dun even after Quit.".into(),
        ],
        launch: ToastAction::Open { item: Some(item) },
        buttons,
        reminder: true,
    }
}

#[tauri::command]
pub fn toast_spike_show<R: Runtime>(_app: AppHandle<R>, kind: String) -> Result<String, String> {
    #[cfg(windows)]
    {
        let toast = spike_toast(&kind);
        super::toast::show(Identity::current(), &toast)?;
        append_log(&format!("shown {kind} #{}", SHOWN.load(Ordering::SeqCst)));
        Ok(format!("shown ({kind})"))
    }
    #[cfg(not(windows))]
    {
        let _ = spike_toast(&kind);
        Err("toast spike is Windows-only".into())
    }
}

#[tauri::command]
pub fn toast_spike_remove() -> Result<String, String> {
    #[cfg(windows)]
    {
        super::toast::remove(Identity::current(), SPIKE_TAG, GROUP)?;
        append_log("removed");
        Ok("removed".into())
    }
    #[cfg(not(windows))]
    Err("toast spike is Windows-only".into())
}

#[tauri::command]
pub fn toast_spike_status() -> Result<serde_json::Value, String> {
    #[cfg(windows)]
    let blocked = super::toast::blocked_reason(Identity::current())?;
    #[cfg(not(windows))]
    let blocked: Option<&str> = Some("not Windows");
    let log = std::fs::read_to_string(log_path()).unwrap_or_default();
    let tail: Vec<&str> = log.lines().rev().take(25).collect();
    Ok(serde_json::json!({
        "aumid": Identity::current().aumid,
        "blocked": blocked,
        "logPath": log_path(),
        "log": tail,
    }))
}
