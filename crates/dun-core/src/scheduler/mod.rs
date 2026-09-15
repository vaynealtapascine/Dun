//! Decides, at a moment in time, which alerts to show, clear or hold back.
//!
//! [`Scheduler::evaluate`] is pure apart from the scheduler's own ring states:
//! given the synced [`State`], `now` and the device's role, it returns
//! [`Effect`]s (show a toast, play a chime, wake me at ...) for the platform
//! shell to carry out. Ring states are device-local and persisted by the
//! caller between runs, so a restart doesn't re-alert everything at once.

mod effects;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::{Item, Nag};
use crate::occurrence::{status, Status};
use crate::quiet::is_muted;
use crate::state::State;
use crate::time::{TimeZone, Timestamp, MINUTE, SECOND};

pub use effects::{AlertLabel, AlertLevel, Effect};

/// A ring whose due time is further in the past than this when first seen is
/// labelled "missed" (the device was off, asleep or not running).
pub const MISSED_AFTER_MS: i64 = 90 * SECOND;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Hold {
    /// Waiting for quiet hours to end.
    Quiet,
    /// Waiting for mute to end.
    Mute,
}

/// Device-local progress of one ringing occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RingState {
    pub occurrence: Timestamp,
    /// When the ring (or the end of its snooze) became due.
    pub rings_at: Timestamp,
    pub next_alert_at: Timestamp,
    pub first_alert_at: Option<Timestamp>,
    pub last_alert_at: Option<Timestamp>,
    pub alerts: u32,
    pub missed: bool,
    pub hold: Option<Hold>,
    /// Quiet hours delayed this ring; reported on the next full alert.
    pub quiet_held: bool,
    /// A silent alert is already on screen for this occurrence.
    pub shown: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Scheduler {
    rings: BTreeMap<String, RingState>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_rings(rings: BTreeMap<String, RingState>) -> Self {
        Scheduler { rings }
    }

    pub fn rings(&self) -> &BTreeMap<String, RingState> {
        &self.rings
    }

    pub fn evaluate(&mut self, state: &State, now: Timestamp, tz: &TimeZone) -> Vec<Effect> {
        let mut out = Vec::new();
        let mut wake: Option<Timestamp> = None;
        let mut chime_for: Option<&Item> = None;
        let mut ringing = 0usize;
        let mut next_up: Option<(Timestamp, &Item)> = None;
        let mut alive = std::collections::BTreeSet::new();

        let muted = is_muted(state.settings.mute_until, now);

        for item in state.live_items() {
            let Some(inputs) = item.inputs() else {
                continue;
            };
            let st = status(inputs, now, tz);

            let (occ, rings_at) = match st {
                Status::Due { occ, rings_at, .. } if rings_at <= now => (occ, rings_at),
                other => {
                    if let Some(at) = other.next_wake(now) {
                        wake = earliest(wake, at);
                        if next_up.is_none_or(|(t, _)| at < t) {
                            next_up = Some((at, item));
                        }
                    }
                    continue;
                }
            };

            ringing += 1;
            alive.insert(item.id.clone());

            let current = matches!(
                self.rings.get(&item.id),
                Some(rs) if rs.occurrence == occ.due && rs.rings_at == rings_at
            );
            if !current {
                let missed = now.since(rings_at) > MISSED_AFTER_MS || occ.count > 1;
                self.rings.insert(
                    item.id.clone(),
                    RingState {
                        occurrence: occ.due,
                        rings_at,
                        next_alert_at: now,
                        first_alert_at: None,
                        last_alert_at: None,
                        alerts: 0,
                        missed,
                        hold: None,
                        quiet_held: false,
                        shown: false,
                    },
                );
            }
            let rs = self.rings.get_mut(&item.id).expect("present");

            // A hold whose reason went away (quiet hours edited, mute cancelled)
            // releases immediately.
            let quiet_end = (!item.quiet_exempt())
                .then(|| state.settings.quiet_hours.active_until(now, tz))
                .flatten();
            match rs.hold {
                Some(Hold::Quiet) if quiet_end.is_none() => {
                    rs.hold = None;
                    rs.next_alert_at = now;
                }
                Some(Hold::Mute) if !muted => {
                    rs.hold = None;
                    rs.next_alert_at = now;
                }
                _ => {}
            }
            // An edited nag interval applies from the last alert.
            if let (None, Nag::Repeat { interval_min }, Some(last)) =
                (rs.hold, item.nag, rs.last_alert_at)
            {
                rs.next_alert_at = rs
                    .next_alert_at
                    .min(last.plus_minutes(i64::from(interval_min.max(1))));
            }

            if now >= rs.next_alert_at {
                let label = AlertLabel {
                    due: occ.due,
                    missed_first: rs.missed.then_some(occ.first),
                    missed_count: if rs.missed { occ.count } else { 1 },
                    quiet_held: rs.quiet_held,
                    snooze_count: item
                        .snooze
                        .filter(|s| s.occurrence == occ.due)
                        .map_or(0, |s| s.count),
                };

                if let Some(end) = quiet_end {
                    rs.hold = Some(Hold::Quiet);
                    rs.quiet_held = true;
                    rs.next_alert_at = end;
                } else if muted {
                    if !rs.shown {
                        rs.shown = true;
                        out.push(Effect::alert(item, occ.due, AlertLevel::Silent, label));
                    }
                    rs.hold = Some(Hold::Mute);
                    rs.next_alert_at = state.settings.mute_until.unwrap_or(now);
                } else {
                    rs.hold = None;
                    rs.alerts += 1;
                    rs.first_alert_at.get_or_insert(now);
                    rs.last_alert_at = Some(now);
                    rs.shown = true;
                    rs.next_alert_at = match item.nag {
                        Nag::Repeat { interval_min } => {
                            now.plus(i64::from(interval_min.max(1)) * MINUTE)
                        }
                        Nag::Once => Timestamp::MAX,
                    };
                    rs.quiet_held = false;
                    out.push(Effect::alert(item, occ.due, AlertLevel::Full, label));
                    chime_for.get_or_insert(item);
                }
            }

            if rs.next_alert_at < Timestamp::MAX {
                wake = earliest(wake, rs.next_alert_at);
            }
        }

        // Rings whose item is no longer ringing (done, snoozed, deleted, edited).
        let gone: Vec<String> = self
            .rings
            .keys()
            .filter(|id| !alive.contains(*id))
            .cloned()
            .collect();
        for id in gone {
            self.rings.remove(&id);
            out.push(Effect::ClearAlert { item_id: id });
        }

        if let Some(item) = chime_for {
            out.push(Effect::PlayChime {
                chime: item.chime.clone(),
            });
        }
        if let Some(at) = wake {
            out.push(Effect::ScheduleWake { at });
        }
        out.push(Effect::TrayStatus {
            ringing,
            next: next_up.map(|(at, item)| (item.title.clone(), at)),
        });
        out
    }
}

fn earliest(a: Option<Timestamp>, b: Timestamp) -> Option<Timestamp> {
    Some(a.map_or(b, |a| a.min(b)))
}

#[cfg(test)]
mod tests;
