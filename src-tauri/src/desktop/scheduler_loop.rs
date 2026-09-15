//! The desktop scheduler loop.
//!
//! Wakes at the next deadline the scheduler asked for, and at least once a
//! second to notice sleep/resume, clock jumps and attendance changes. A state
//! change (command, sync merge) or a toast click wakes it immediately.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dun_core::scheduler::{Effect, Role};
use dun_core::time::{Timestamp, SECOND};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::audio::Audio;
use super::toast::xml::ToastAction;
use super::toasts;
use crate::app_core::AppCore;
use crate::commands::STATE_CHANGED;

pub enum Msg {
    /// Re-evaluate now.
    Wake,
    /// A toast or toast button was clicked; the raw argument string.
    Toast(String),
}

/// A gap between loop turns this much longer than planned means the PC slept.
const RESUME_GAP_MS: i64 = 5 * SECOND;

/// Latest tray summary, read by the tray module.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrayStatus {
    pub ringing: usize,
    pub next: Option<(String, Timestamp)>,
    pub muted_until: Option<Timestamp>,
}

pub fn spawn<R: Runtime>(
    app: AppHandle<R>,
    core: Arc<AppCore>,
    audio: Audio,
    rx: Receiver<Msg>,
    tray: Arc<Mutex<TrayStatus>>,
    sync: Arc<crate::desktop::sync::SyncHub<R>>,
    on_tray: impl Fn(&AppHandle<R>, &TrayStatus) + Send + 'static,
) {
    std::thread::Builder::new()
        .name("scheduler".into())
        .spawn(move || run(app, core, audio, rx, tray, sync, on_tray))
        .expect("spawn scheduler thread");
}

fn run<R: Runtime>(
    app: AppHandle<R>,
    core: Arc<AppCore>,
    audio: Audio,
    rx: Receiver<Msg>,
    tray: Arc<Mutex<TrayStatus>>,
    sync: Arc<crate::desktop::sync::SyncHub<R>>,
    on_tray: impl Fn(&AppHandle<R>, &TrayStatus),
) {
    let mut last_turn: Option<(Timestamp, Timestamp)> = None; // (when, planned next)
    let mut last_ringing = usize::MAX;

    loop {
        let (now, tz) = (core.now(), core.tz());
        if let Some((_, planned)) = last_turn {
            if now.since(planned) > RESUME_GAP_MS {
                eprintln!("scheduler: resumed after {} s", now.since(planned) / 1_000);
            }
        }

        let local = core.local_settings();
        let attended = super::attended::is_attended(local.idle_threshold_s);
        sync.set_attended(attended);

        // Lock order everywhere: engine first, then acks (see desktop::sync).
        let effects = {
            let mut engine = core.engine();
            let handoff = engine.state().settings.handoff.enabled;
            let acks = sync.acks();
            let acks = acks.lock().unwrap_or_else(|p| p.into_inner());
            let role = if handoff && !sync.peers_empty() {
                Role::Hub {
                    attended,
                    acks: &acks,
                }
            } else {
                Role::Solo
            };
            match engine.evaluate(now, &tz, role) {
                Ok(effects) => effects,
                Err(e) => {
                    eprintln!("scheduler: evaluate failed: {e}");
                    Vec::new()
                }
            }
        };

        let mut wake_at: Option<Timestamp> = None;
        let mut visible_change = false;
        for effect in &effects {
            match effect {
                Effect::ShowAlert {
                    item_id,
                    occurrence,
                    level,
                    title,
                    notes,
                    label,
                } => {
                    visible_change = true;
                    let toast =
                        toasts::ring_toast(item_id, *occurrence, title, notes, label, now, &tz);
                    if let Err(e) = toasts::show(&toast, *level) {
                        eprintln!("scheduler: toast failed: {e}");
                    }
                }
                Effect::ClearAlert { item_id } => {
                    visible_change = true;
                    toasts::clear(item_id);
                }
                Effect::ShowSummary { items, level } => {
                    visible_change = true;
                    if let Err(e) = toasts::show(&toasts::summary_toast(items), *level) {
                        eprintln!("scheduler: summary toast failed: {e}");
                    }
                }
                Effect::ClearSummary => {
                    visible_change = true;
                    toasts::clear(toasts::SUMMARY_TAG);
                }
                Effect::PlayChime { chime } => {
                    audio.play(chime.as_ref(), &local.chime, local.volume)
                }
                Effect::ScheduleWake { at } => wake_at = Some(*at),
                Effect::TrayStatus { ringing, next } => {
                    let status = TrayStatus {
                        ringing: *ringing,
                        next: next.clone(),
                        muted_until: core
                            .engine()
                            .state()
                            .settings
                            .mute_until
                            .filter(|m| *m > now),
                    };
                    let mut current = tray.lock().unwrap_or_else(|p| p.into_inner());
                    if *current != status {
                        *current = status.clone();
                        drop(current);
                        on_tray(&app, &status);
                    }
                    if *ringing != last_ringing {
                        last_ringing = *ringing;
                        visible_change = true;
                    }
                }
            }
        }
        if visible_change {
            let _ = app.emit(STATE_CHANGED, core.snapshot_json());
        }

        let planned = wake_at.map_or(now.plus(SECOND), |w| w.min(now.plus(SECOND)));
        last_turn = Some((now, planned));
        let wait = Duration::from_millis(planned.since(now).clamp(10, 1_000) as u64);

        match rx.recv_timeout(wait) {
            Ok(Msg::Wake) | Err(RecvTimeoutError::Timeout) => {}
            Ok(Msg::Toast(args)) => handle_toast(&app, &core, &args),
            Err(RecvTimeoutError::Disconnected) => return,
        }
        // Coalesce a burst of wake-ups into one evaluation.
        while let Ok(msg) = rx.try_recv() {
            if let Msg::Toast(args) = msg {
                handle_toast(&app, &core, &args);
            }
        }
    }
}

fn handle_toast<R: Runtime>(app: &AppHandle<R>, core: &AppCore, args: &str) {
    let action = match ToastAction::parse(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("scheduler: ignoring toast click: {e}");
            return;
        }
    };
    let (now, tz) = (core.now(), core.tz());
    let result = {
        let mut engine = core.engine();
        match &action {
            ToastAction::Open { item } => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                    if let Some(item) = item {
                        let _ = app.emit("focus-item", item);
                    }
                }
                Ok(())
            }
            ToastAction::Done { item, occ } => engine.done(now, &tz, item, Some(Timestamp(*occ))),
            ToastAction::Snooze { item, occ, minutes } => {
                engine.snooze(now, &tz, item, Some(Timestamp(*occ)), *minutes)
            }
            ToastAction::SnoozeAll { minutes } => {
                let ids = engine.scheduler().summary().to_vec();
                engine.snooze_many(now, &tz, &ids, *minutes)
            }
        }
    };
    match result {
        Ok(()) => {
            let _ = app.emit(STATE_CHANGED, core.snapshot_json());
        }
        // The item may have been deleted or finished elsewhere; nothing to do.
        Err(e) => eprintln!("scheduler: toast action {action:?} failed: {e}"),
    }
}
