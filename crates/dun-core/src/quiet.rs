//! Quiet hours and mute: windows during which Dun holds rings back.
//!
//! Both are synced settings. Quiet hours are a nightly civil-time window
//! evaluated in the device's zone; mute is an absolute instant.

use jiff::civil::Time;
use jiff::ToSpan;
use serde::{Deserialize, Serialize};

use crate::time::{resolve, TimeZone, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuietHours {
    pub enabled: bool,
    pub start: Time,
    pub end: Time,
}

impl Default for QuietHours {
    fn default() -> Self {
        QuietHours {
            enabled: false,
            start: jiff::civil::time(23, 0, 0, 0),
            end: jiff::civil::time(7, 0, 0, 0),
        }
    }
}

/// Fallback end for "mute until tomorrow" when quiet hours are off.
pub const DEFAULT_MORNING: Time = jiff::civil::time(7, 0, 0, 0);

impl QuietHours {
    /// If `now` falls inside quiet hours, the instant they end.
    ///
    /// A window whose start is after its end crosses midnight (23:00–07:00).
    /// Equal start and end means no window.
    pub fn active_until(&self, now: Timestamp, tz: &TimeZone) -> Option<Timestamp> {
        if !self.enabled || self.start == self.end {
            return None;
        }
        let local = now.to_zoned(tz);
        let (date, t) = (local.date(), local.time());

        let end_date = if self.start < self.end {
            (self.start <= t && t < self.end).then_some(date)?
        } else if t >= self.start {
            date.checked_add(1.day()).ok()?
        } else if t < self.end {
            date
        } else {
            return None;
        };

        let end = resolve(end_date.to_datetime(self.end), tz);
        // A DST gap can push the resolved end back onto or before now.
        (end > now).then_some(end)
    }
}

/// The next instant strictly after `now` whose wall-clock time is `t`.
pub fn next_local_time(now: Timestamp, t: Time, tz: &TimeZone) -> Timestamp {
    let date = now.to_zoned(tz).date();
    let today = resolve(date.to_datetime(t), tz);
    if today > now {
        return today;
    }
    match date.checked_add(1.day()) {
        Ok(tomorrow) => resolve(tomorrow.to_datetime(t), tz),
        Err(_) => Timestamp::MAX,
    }
}

/// When "mute until tomorrow" should end: the next end of quiet hours if they
/// are on (so mute hands straight over to them), otherwise the next 07:00.
pub fn mute_until_tomorrow(now: Timestamp, quiet: &QuietHours, tz: &TimeZone) -> Timestamp {
    if let Some(end) = quiet.active_until(now, tz) {
        return end;
    }
    let morning = if quiet.enabled && quiet.start != quiet.end {
        quiet.end
    } else {
        DEFAULT_MORNING
    };
    next_local_time(now, morning, tz)
}

/// `true` while a mute set to end at `until` is still in effect.
pub fn is_muted(until: Option<Timestamp>, now: Timestamp) -> bool {
    until.is_some_and(|u| u > now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::zone;
    use jiff::civil::{date, time, DateTime};

    fn at(tz: &TimeZone, dt: DateTime) -> Timestamp {
        resolve(dt, tz)
    }

    fn local(tz: &TimeZone, t: Timestamp) -> String {
        t.to_zoned(tz).datetime().to_string()
    }

    fn night() -> QuietHours {
        QuietHours {
            enabled: true,
            start: time(23, 0, 0, 0),
            end: time(7, 0, 0, 0),
        }
    }

    #[test]
    fn overnight_window() {
        let tz = zone("Asia/Manila");
        let q = night();
        let end = |dt| q.active_until(at(&tz, dt), &tz).map(|t| local(&tz, t));

        assert_eq!(end(date(2026, 9, 16).at(22, 59, 0, 0)), None);
        assert_eq!(
            end(date(2026, 9, 16).at(23, 0, 0, 0)).as_deref(),
            Some("2026-09-17T07:00:00")
        );
        assert_eq!(
            end(date(2026, 9, 17).at(3, 0, 0, 0)).as_deref(),
            Some("2026-09-17T07:00:00")
        );
        assert_eq!(end(date(2026, 9, 17).at(7, 0, 0, 0)), None);
    }

    #[test]
    fn same_day_window_and_disabled() {
        let tz = TimeZone::UTC;
        let lunch = QuietHours {
            enabled: true,
            start: time(12, 0, 0, 0),
            end: time(13, 0, 0, 0),
        };
        assert!(lunch
            .active_until(at(&tz, date(2026, 9, 16).at(12, 30, 0, 0)), &tz)
            .is_some());
        assert!(lunch
            .active_until(at(&tz, date(2026, 9, 16).at(13, 30, 0, 0)), &tz)
            .is_none());

        let off = QuietHours {
            enabled: false,
            ..night()
        };
        assert!(off
            .active_until(at(&tz, date(2026, 9, 16).at(23, 30, 0, 0)), &tz)
            .is_none());

        let empty = QuietHours {
            enabled: true,
            start: time(9, 0, 0, 0),
            end: time(9, 0, 0, 0),
        };
        assert!(empty
            .active_until(at(&tz, date(2026, 9, 16).at(9, 0, 0, 0)), &tz)
            .is_none());
    }

    #[test]
    fn ends_on_wall_clock_across_dst() {
        let ny = zone("America/New_York");
        let q = QuietHours {
            enabled: true,
            start: time(1, 0, 0, 0),
            end: time(2, 30, 0, 0),
        };
        // Spring forward: 02:30 doesn't exist, so the window ends at 03:30 EDT.
        let end = q
            .active_until(at(&ny, date(2026, 3, 8).at(1, 30, 0, 0)), &ny)
            .unwrap();
        assert_eq!(local(&ny, end), "2026-03-08T03:30:00");
    }

    #[test]
    fn mute_until_tomorrow_hands_over_to_quiet_hours_or_7am() {
        let tz = TimeZone::UTC;
        let off = QuietHours::default();

        let late = at(&tz, date(2026, 9, 16).at(21, 0, 0, 0));
        assert_eq!(
            local(&tz, mute_until_tomorrow(late, &off, &tz)),
            "2026-09-17T07:00:00"
        );

        let early = at(&tz, date(2026, 9, 17).at(3, 0, 0, 0));
        assert_eq!(
            local(&tz, mute_until_tomorrow(early, &off, &tz)),
            "2026-09-17T07:00:00"
        );

        let q = QuietHours {
            enabled: true,
            start: time(22, 0, 0, 0),
            end: time(6, 30, 0, 0),
        };
        assert_eq!(
            local(&tz, mute_until_tomorrow(late, &q, &tz)),
            "2026-09-17T06:30:00"
        );
        let inside = at(&tz, date(2026, 9, 16).at(23, 0, 0, 0));
        assert_eq!(
            local(&tz, mute_until_tomorrow(inside, &q, &tz)),
            "2026-09-17T06:30:00"
        );
    }

    #[test]
    fn mute_flag() {
        let now = Timestamp(1_000);
        assert!(is_muted(Some(Timestamp(2_000)), now));
        assert!(!is_muted(Some(now), now));
        assert!(!is_muted(None, now));
    }
}
