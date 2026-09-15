//! Decides, at a moment in time, which alerts to show, clear or hold back.
//!
//! [`Scheduler::evaluate`] is pure apart from the scheduler's own ring states:
//! given the synced [`State`], `now`, the device's zone and its [`Role`], it
//! returns [`Effect`]s (show a toast, play a chime, wake me at ...) for the
//! platform shell to carry out. Ring states are device-local and persisted by
//! the caller between runs, so a restart doesn't re-alert everything at once.
//!
//! Order of decisions for a ring that is due to alert:
//!
//! 1. **Quiet hours** (unless the item is exempt): hold until they end.
//! 2. **Mute**: show once silently, hold until mute ends.
//! 3. **Role**:
//!    - *Hub* (PC with a paired phone), unattended: defer to the phone while
//!      it confirms it is covering the ring; otherwise defer until
//!      `fail_loud_after`, then ring anyway.
//!    - *Phone*: stay quiet while the PC is attended and already ringing this
//!      occurrence; otherwise ring.
//!    - *Solo*: ring.
//!
//! A hold is released as soon as its reason goes away (quiet hours turned off,
//! mute cancelled, someone came back to the PC).

mod effects;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::model::{Item, Nag};
use crate::occurrence::{status, Occurrence, Status};
use crate::quiet::is_muted;
use crate::state::State;
use crate::time::{TimeZone, Timestamp, MINUTE, SECOND};

pub use effects::{when, AlertLabel, AlertLevel, Effect};

/// A ring whose due time is further in the past than this when first seen is
/// labelled "missed" (the device was off, asleep or not running).
pub const MISSED_AFTER_MS: i64 = 90 * SECOND;

/// How many of a burst of missed rings still alert individually.
pub const SUMMARY_KEEP_INDIVIDUAL: usize = 2;

/// Clock disagreement tolerated when matching a phone's acknowledgement.
pub const ACK_SKEW_MS: i64 = 30 * SECOND;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Hold {
    /// Waiting for quiet hours to end.
    Quiet,
    /// Waiting for mute to end.
    Mute,
    /// PC unattended; the phone is (or may be) handling this ring.
    Handoff,
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
    /// An alert for this occurrence is currently on screen.
    pub shown: bool,
    /// Part of the "several missed" summary instead of alerting on its own.
    #[serde(default)]
    pub summarized: bool,
}

/// A phone's statement that it is about to alert (or suppress) an occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ack {
    pub occurrence: Timestamp,
    /// PC time the acknowledgement arrived.
    pub at: Timestamp,
}

/// What the phone learned from the PC at its last check-in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PcView {
    pub attended: bool,
    /// Occurrences the PC is ringing (or about to).
    pub ringing: BTreeSet<(String, Timestamp)>,
}

#[derive(Debug, Clone, Copy)]
pub enum Role<'a> {
    /// No paired phone: ring normally.
    Solo,
    /// The PC with a paired phone.
    Hub {
        attended: bool,
        acks: &'a BTreeMap<String, Ack>,
    },
    /// The phone. `pc` is `None` when the PC couldn't be reached.
    Phone { pc: Option<&'a PcView> },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Scheduler {
    rings: BTreeMap<String, RingState>,
    /// Items in the summary alert as last shown.
    #[serde(default)]
    summary: Vec<String>,
}

enum Decision {
    HoldQuiet(Timestamp),
    HoldMute(Timestamp),
    Defer(Timestamp),
    Suppress,
    Full,
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_rings(rings: BTreeMap<String, RingState>) -> Self {
        Scheduler {
            rings,
            summary: Vec::new(),
        }
    }

    pub fn rings(&self) -> &BTreeMap<String, RingState> {
        &self.rings
    }

    /// Occurrences that would alert if evaluated at `now`. A phone reports
    /// these in its check-in so the PC knows the phone is covering them.
    pub fn due_for_alert(
        &self,
        state: &State,
        now: Timestamp,
        tz: &TimeZone,
    ) -> Vec<(String, Timestamp)> {
        ringing_now(state, now, tz)
            .filter(|(item, occ, rings_at)| match self.rings.get(&item.id) {
                Some(rs) if rs.occurrence == occ.due && rs.rings_at == *rings_at => {
                    rs.next_alert_at <= now
                }
                _ => true,
            })
            .map(|(item, occ, _)| (item.id.clone(), occ.due))
            .collect()
    }

    pub fn evaluate(
        &mut self,
        state: &State,
        now: Timestamp,
        tz: &TimeZone,
        role: Role<'_>,
    ) -> Vec<Effect> {
        let mut out = Vec::new();
        let mut wake: Option<Timestamp> = None;
        let mut next_up: Option<(Timestamp, &Item)> = None;
        let settings = &state.settings;
        let muted = is_muted(settings.mute_until, now);

        // Phase 1: find what is ringing and keep ring states in step.
        let mut ringing: Vec<(&Item, Occurrence)> = Vec::new();
        let mut newly_missed: Vec<(Timestamp, String)> = Vec::new();
        for item in state.live_items() {
            let Some(inputs) = item.inputs() else {
                continue;
            };
            match status(inputs, now, tz) {
                Status::Due { occ, rings_at, .. } if rings_at <= now => {
                    let current = matches!(
                        self.rings.get(&item.id),
                        Some(rs) if rs.occurrence == occ.due && rs.rings_at == rings_at
                    );
                    if !current {
                        let missed = now.since(rings_at) > MISSED_AFTER_MS || occ.count > 1;
                        if missed {
                            newly_missed.push((occ.due, item.id.clone()));
                        }
                        self.rings
                            .insert(item.id.clone(), new_ring(occ, rings_at, now, missed));
                    }
                    ringing.push((item, occ));
                }
                other => {
                    if let Some(at) = other.next_wake(now) {
                        wake = earliest(wake, at);
                        if next_up.is_none_or(|(t, _)| at < t) {
                            next_up = Some((at, item));
                        }
                    }
                }
            }
        }

        // Phase 2: a burst of missed rings collapses into one summary.
        if newly_missed.len() > settings.missed_summary_threshold as usize {
            newly_missed.sort_by(|a, b| b.cmp(a));
            for (_, id) in newly_missed.iter().skip(SUMMARY_KEEP_INDIVIDUAL) {
                if let Some(rs) = self.rings.get_mut(id) {
                    rs.summarized = true;
                }
            }
        }

        // Phase 3: decide each ring.
        let deferring = matches!(
            role,
            Role::Hub {
                attended: false,
                ..
            }
        ) && settings.handoff.enabled;
        let mut chime_for: Option<&Item> = None;
        let mut summary_level: Option<AlertLevel> = None;

        for (item, occ) in &ringing {
            let rs = self.rings.get_mut(&item.id).expect("created in phase 1");
            let quiet_end = (!item.quiet_exempt())
                .then(|| settings.quiet_hours.active_until(now, tz))
                .flatten();

            let released = match rs.hold {
                Some(Hold::Quiet) => quiet_end.is_none(),
                Some(Hold::Mute) => !muted,
                Some(Hold::Handoff) => !deferring,
                None => false,
            };
            if released {
                rs.hold = None;
                rs.next_alert_at = now;
            }
            if let (None, Nag::Repeat { interval_min }, Some(last)) =
                (rs.hold, item.nag, rs.last_alert_at)
            {
                // An edited nag interval applies from the last alert.
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
                let interval_ms = match item.nag {
                    Nag::Repeat { interval_min } => i64::from(interval_min.max(1)) * MINUTE,
                    Nag::Once => 0,
                };

                let decision = if let Some(end) = quiet_end {
                    Decision::HoldQuiet(end)
                } else if muted {
                    Decision::HoldMute(settings.mute_until.unwrap_or(now))
                } else {
                    match role {
                        Role::Hub { acks, .. } if deferring => {
                            let h = settings.handoff;
                            let grace = i64::from(h.phone_grace_s) * SECOND;
                            let fail_at = rs.rings_at.plus(i64::from(h.fail_loud_after_s) * SECOND);
                            let covered_until = acks
                                .get(&item.id)
                                .filter(|a| {
                                    a.occurrence == occ.due
                                        && a.at >= rs.rings_at.minus(ACK_SKEW_MS)
                                })
                                .map(|a| {
                                    if interval_ms == 0 {
                                        Timestamp::MAX
                                    } else {
                                        a.at.plus(interval_ms + grace)
                                    }
                                });
                            match covered_until {
                                Some(until) if now < until => Decision::Defer(until),
                                _ if now < fail_at => Decision::Defer(fail_at),
                                _ => Decision::Full,
                            }
                        }
                        Role::Phone { pc: Some(pc) }
                            if pc.attended && pc.ringing.contains(&(item.id.clone(), occ.due)) =>
                        {
                            Decision::Suppress
                        }
                        _ => Decision::Full,
                    }
                };

                let summarized = rs.summarized;
                let mut emit = |level: AlertLevel, out: &mut Vec<Effect>| {
                    if summarized {
                        summary_level = Some(match (summary_level, level) {
                            (Some(AlertLevel::Full), _) | (_, AlertLevel::Full) => AlertLevel::Full,
                            _ => AlertLevel::Silent,
                        });
                    } else {
                        out.push(Effect::alert(item, occ.due, level, label));
                    }
                };

                match decision {
                    Decision::HoldQuiet(end) => {
                        rs.hold = Some(Hold::Quiet);
                        rs.quiet_held = true;
                        rs.next_alert_at = end;
                    }
                    Decision::HoldMute(until) | Decision::Defer(until) => {
                        if !rs.shown {
                            rs.shown = true;
                            emit(AlertLevel::Silent, &mut out);
                        }
                        rs.hold = Some(if matches!(decision, Decision::Defer(_)) {
                            Hold::Handoff
                        } else {
                            Hold::Mute
                        });
                        rs.next_alert_at = until;
                    }
                    Decision::Suppress => {
                        if rs.shown && !rs.summarized {
                            out.push(Effect::ClearAlert {
                                item_id: item.id.clone(),
                            });
                        }
                        rs.shown = false;
                        rs.hold = None;
                        rs.next_alert_at = now.plus(interval_ms.max(MINUTE));
                    }
                    Decision::Full => {
                        rs.hold = None;
                        rs.alerts += 1;
                        rs.first_alert_at.get_or_insert(now);
                        rs.last_alert_at = Some(now);
                        rs.shown = true;
                        rs.quiet_held = false;
                        rs.next_alert_at = if interval_ms == 0 {
                            Timestamp::MAX
                        } else {
                            now.plus(interval_ms)
                        };
                        emit(AlertLevel::Full, &mut out);
                        chime_for.get_or_insert(item);
                    }
                }
            }

            if rs.next_alert_at < Timestamp::MAX {
                wake = earliest(wake, rs.next_alert_at);
            }
        }

        // Phase 4: tidy up rings that stopped, and the summary.
        let alive: BTreeSet<&str> = ringing.iter().map(|(i, _)| i.id.as_str()).collect();
        let gone: Vec<String> = self
            .rings
            .keys()
            .filter(|id| !alive.contains(id.as_str()))
            .cloned()
            .collect();
        for id in gone {
            self.rings.remove(&id);
            out.push(Effect::ClearAlert { item_id: id });
        }

        let mut summarized: Vec<(Timestamp, &str)> = self
            .rings
            .iter()
            .filter(|(_, rs)| rs.summarized)
            .map(|(id, rs)| (rs.occurrence, id.as_str()))
            .collect();
        summarized.sort_by(|a, b| b.cmp(a));
        let summary_ids: Vec<String> = summarized.iter().map(|(_, id)| id.to_string()).collect();
        let level = match summary_level {
            Some(level) => Some(level),
            None if summary_ids != self.summary && !summary_ids.is_empty() => {
                Some(AlertLevel::Silent)
            }
            None => None,
        };
        if summary_ids.is_empty() {
            if !self.summary.is_empty() {
                out.push(Effect::ClearSummary);
            }
        } else if let Some(level) = level {
            out.push(Effect::ShowSummary {
                items: summary_ids
                    .iter()
                    .map(|id| (id.clone(), state.items[id].title.clone()))
                    .collect(),
                level,
            });
        }
        self.summary = summary_ids;

        if let Some(item) = chime_for {
            out.push(Effect::PlayChime {
                chime: item.chime.clone(),
            });
        }
        if let Some(at) = wake {
            out.push(Effect::ScheduleWake { at });
        }
        out.push(Effect::TrayStatus {
            ringing: ringing.len(),
            next: next_up.map(|(at, item)| (item.title.clone(), at)),
        });
        out
    }
}

/// Occurrences ringing on this device now or within `lookahead_ms`. The PC
/// reports these to the phone so the phone can stay quiet while you're at the PC.
pub fn ringing_soon(
    state: &State,
    now: Timestamp,
    tz: &TimeZone,
    lookahead_ms: i64,
) -> BTreeSet<(String, Timestamp)> {
    ringing_now(state, now.plus(lookahead_ms), tz)
        .map(|(item, occ, _)| (item.id.clone(), occ.due))
        .collect()
}

fn ringing_now<'s>(
    state: &'s State,
    now: Timestamp,
    tz: &'s TimeZone,
) -> impl Iterator<Item = (&'s Item, Occurrence, Timestamp)> + 's {
    state
        .live_items()
        .filter_map(move |item| match status(item.inputs()?, now, tz) {
            Status::Due { occ, rings_at, .. } if rings_at <= now => Some((item, occ, rings_at)),
            _ => None,
        })
}

fn new_ring(occ: Occurrence, rings_at: Timestamp, now: Timestamp, missed: bool) -> RingState {
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
        summarized: false,
    }
}

fn earliest(a: Option<Timestamp>, b: Timestamp) -> Option<Timestamp> {
    Some(a.map_or(b, |a| a.min(b)))
}

#[cfg(test)]
mod tests;
