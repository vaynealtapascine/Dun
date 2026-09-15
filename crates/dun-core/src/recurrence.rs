//! Recurrence rules and the slots they produce.
//!
//! Two families behave differently on purpose:
//!
//! - [`Rule::Interval`] ("every 90 minutes") counts real elapsed time from the
//!   start instant, so DST changes never stretch or shrink it.
//! - Calendar rules (daily, weekly, monthly, yearly) are wall-clock times in a
//!   zone: "every day at 09:00" stays at 09:00 across DST. Times that fall in
//!   a DST gap move forward by the gap; times in a fold use the earlier
//!   instant (see [`crate::time::resolve`]). Month days past the end of a
//!   month clamp to its last day, and Feb 29 clamps to Feb 28 in common years.
//!
//! Slots are never earlier than [`Recurrence::start`].

use jiff::civil::{Date, DateTime, Time};
use jiff::ToSpan;
use serde::{Deserialize, Serialize};

use crate::time::{resolve, zone, TimeZone, Timestamp, HOUR, MINUTE};

/// Upper bound on slots counted when collapsing missed occurrences.
pub const MAX_COUNT: u32 = 10_000;

/// Upper bound on calendar days scanned in one query (~137 years).
const MAX_SCAN_DAYS: i32 = 50_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IntervalUnit {
    Minutes,
    Hours,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Rule {
    /// Every `every` minutes or hours of real time.
    Interval { every: u32, unit: IntervalUnit },
    /// Every `every` days at each of `times`.
    Daily { every: u32, times: Vec<Time> },
    /// Every `every` weeks on `weekdays` (ISO: 1 = Monday … 7 = Sunday) at `times`.
    Weekly {
        every: u32,
        weekdays: Vec<u8>,
        times: Vec<Time>,
    },
    /// Every `every` months on day `day` (1–31, clamped) at `times`.
    Monthly {
        every: u32,
        day: i8,
        times: Vec<Time>,
    },
    /// Every year on `month`/`day` at `times`.
    Yearly {
        month: i8,
        day: i8,
        times: Vec<Time>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recurrence {
    pub rule: Rule,
    /// Civil date-time of the first possible slot, in [`Recurrence::tz`].
    pub start: DateTime,
    /// IANA zone, or `None` to follow the device's current zone.
    pub tz: Option<String>,
}

/// Slots inside a time window, for collapsing missed occurrences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub first: Timestamp,
    pub last: Timestamp,
    /// Number of slots, capped at [`MAX_COUNT`].
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecurrenceError {
    #[error("repeat interval must be between 1 and {max}")]
    Every { max: u32 },
    #[error("pick at least one time of day")]
    NoTimes,
    #[error("at most 24 times of day")]
    TooManyTimes,
    #[error("the same time of day is listed twice")]
    DuplicateTime,
    #[error("pick at least one weekday")]
    NoWeekdays,
    #[error("weekday must be 1 (Monday) to 7 (Sunday)")]
    BadWeekday,
    #[error("day of month must be 1 to 31")]
    BadMonthDay,
    #[error("{month}/{day} is not a real date")]
    BadYearlyDate { month: i8, day: i8 },
}

impl Rule {
    fn times(&self) -> &[Time] {
        match self {
            Rule::Interval { .. } => &[],
            Rule::Daily { times, .. }
            | Rule::Weekly { times, .. }
            | Rule::Monthly { times, .. }
            | Rule::Yearly { times, .. } => times,
        }
    }

    pub fn validate(&self) -> Result<(), RecurrenceError> {
        let check_every = |every: u32, max: u32| {
            if (1..=max).contains(&every) {
                Ok(())
            } else {
                Err(RecurrenceError::Every { max })
            }
        };
        match self {
            Rule::Interval { every, unit } => {
                return check_every(
                    *every,
                    match unit {
                        IntervalUnit::Minutes => 10_000,
                        IntervalUnit::Hours => 1_000,
                    },
                )
            }
            Rule::Daily { every, .. } => check_every(*every, 3_650)?,
            Rule::Weekly {
                every, weekdays, ..
            } => {
                check_every(*every, 520)?;
                if weekdays.is_empty() {
                    return Err(RecurrenceError::NoWeekdays);
                }
                if weekdays.iter().any(|d| !(1..=7).contains(d)) {
                    return Err(RecurrenceError::BadWeekday);
                }
            }
            Rule::Monthly { every, day, .. } => {
                check_every(*every, 120)?;
                if !(1..=31).contains(day) {
                    return Err(RecurrenceError::BadMonthDay);
                }
            }
            Rule::Yearly { month, day, .. } => {
                // 2024 is a leap year, so Feb 29 is accepted and clamped later.
                if Date::new(2024, *month, *day).is_err() {
                    return Err(RecurrenceError::BadYearlyDate {
                        month: *month,
                        day: *day,
                    });
                }
            }
        }
        let times = self.times();
        if times.is_empty() {
            return Err(RecurrenceError::NoTimes);
        }
        if times.len() > 24 {
            return Err(RecurrenceError::TooManyTimes);
        }
        let mut sorted = times.to_vec();
        sorted.sort();
        sorted.dedup();
        if sorted.len() != times.len() {
            return Err(RecurrenceError::DuplicateTime);
        }
        Ok(())
    }
}

impl Recurrence {
    pub fn validate(&self) -> Result<(), RecurrenceError> {
        self.rule.validate()
    }

    /// The zone this rule is evaluated in on a device whose zone is `device_tz`.
    pub fn zone(&self, device_tz: &TimeZone) -> TimeZone {
        match &self.tz {
            Some(name) => zone(name),
            None => device_tz.clone(),
        }
    }

    /// Instant of the first possible slot.
    pub fn start_instant(&self, device_tz: &TimeZone) -> Timestamp {
        resolve(self.start, &self.zone(device_tz))
    }

    /// The first slot strictly after `after`.
    pub fn next_after(&self, after: Timestamp, device_tz: &TimeZone) -> Option<Timestamp> {
        let tz = self.zone(device_tz);
        match &self.rule {
            Rule::Interval { every, unit } => {
                let (anchor, step) = self.interval(*every, *unit, &tz);
                Some(interval_next(anchor, step, after))
            }
            _ => self.calendar_slots(after, &tz).next(),
        }
    }

    /// The next `n` slots strictly after `after` (for previews).
    pub fn upcoming(&self, after: Timestamp, n: usize, device_tz: &TimeZone) -> Vec<Timestamp> {
        let mut out = Vec::with_capacity(n);
        let mut cursor = after;
        while out.len() < n {
            match self.next_after(cursor, device_tz) {
                Some(t) => {
                    out.push(t);
                    cursor = t;
                }
                None => break,
            }
        }
        out
    }

    /// Slots in `(after, until]`.
    pub fn window(
        &self,
        after: Timestamp,
        until: Timestamp,
        device_tz: &TimeZone,
    ) -> Option<Window> {
        if until <= after {
            return None;
        }
        let tz = self.zone(device_tz);
        match &self.rule {
            Rule::Interval { every, unit } => {
                let (anchor, step) = self.interval(*every, *unit, &tz);
                let first = interval_next(anchor, step, after);
                if first > until {
                    return None;
                }
                let last = Timestamp(anchor.0 + (until.0 - anchor.0).div_euclid(step) * step);
                let count = ((last.0 - first.0) / step + 1).min(i64::from(MAX_COUNT)) as u32;
                Some(Window { first, last, count })
            }
            _ => {
                let mut window: Option<Window> = None;
                for slot in self.calendar_slots(after, &tz) {
                    if slot > until {
                        break;
                    }
                    window = Some(match window {
                        None => Window {
                            first: slot,
                            last: slot,
                            count: 1,
                        },
                        Some(w) => Window {
                            first: w.first,
                            last: slot,
                            count: (w.count + 1).min(MAX_COUNT),
                        },
                    });
                }
                window
            }
        }
    }

    fn interval(&self, every: u32, unit: IntervalUnit, tz: &TimeZone) -> (Timestamp, i64) {
        let unit_ms = match unit {
            IntervalUnit::Minutes => MINUTE,
            IntervalUnit::Hours => HOUR,
        };
        (resolve(self.start, tz), i64::from(every.max(1)) * unit_ms)
    }

    /// Calendar slots strictly after `after`, ascending.
    fn calendar_slots<'a>(
        &'a self,
        after: Timestamp,
        tz: &'a TimeZone,
    ) -> impl Iterator<Item = Timestamp> + 'a {
        let start_date = self.start.date();
        // Begin a day early: a slot on the previous civil date can still be
        // after `after` once DST or zone offsets are applied.
        let from = after
            .to_zoned(tz)
            .date()
            .checked_sub(1.day())
            .unwrap_or(start_date)
            .max(start_date);

        let mut times = self.rule.times().to_vec();
        times.sort();

        let mut day = 0i32;
        let mut pending: Vec<Timestamp> = Vec::new();
        std::iter::from_fn(move || loop {
            if let Some(t) = pending.pop() {
                return Some(t);
            }
            if day >= MAX_SCAN_DAYS {
                return None;
            }
            let date = from.checked_add(day.days()).ok()?;
            day += 1;
            if !self.date_matches(date) {
                continue;
            }
            let mut slots: Vec<Timestamp> = times
                .iter()
                .map(|t| date.to_datetime(*t))
                .filter(|dt| *dt >= self.start)
                .map(|dt| resolve(dt, tz))
                .filter(|t| *t > after)
                .collect();
            // A DST gap can reorder or merge times within one day.
            slots.sort();
            slots.dedup();
            slots.reverse();
            pending = slots;
        })
    }

    fn date_matches(&self, date: Date) -> bool {
        let start = self.start.date();
        if date < start {
            return false;
        }
        match &self.rule {
            Rule::Interval { .. } => false,
            Rule::Daily { every, .. } => days_between(start, date) % i64::from(*every) == 0,
            Rule::Weekly {
                every, weekdays, ..
            } => {
                let iso = date.weekday().to_monday_one_offset() as u8;
                weekdays.contains(&iso)
                    && days_between(monday_of(start), monday_of(date)) / 7 % i64::from(*every) == 0
            }
            Rule::Monthly { every, day, .. } => {
                let months = (i64::from(date.year()) - i64::from(start.year())) * 12
                    + i64::from(date.month())
                    - i64::from(start.month());
                months % i64::from(*every) == 0 && date.day() == (*day).min(date.days_in_month())
            }
            Rule::Yearly { month, day, .. } => {
                date.month() == *month && date.day() == (*day).min(date.days_in_month())
            }
        }
    }
}

fn interval_next(anchor: Timestamp, step: i64, after: Timestamp) -> Timestamp {
    if after < anchor {
        anchor
    } else {
        Timestamp(anchor.0 + ((after.0 - anchor.0) / step + 1) * step)
    }
}

fn days_between(a: Date, b: Date) -> i64 {
    a.until(b).map(|s| i64::from(s.get_days())).unwrap_or(0)
}

fn monday_of(d: Date) -> Date {
    let offset = d.weekday().to_monday_zero_offset();
    d.checked_sub(i32::from(offset).days()).unwrap_or(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::{date, time};

    fn ts(tz: &TimeZone, dt: DateTime) -> Timestamp {
        resolve(dt, tz)
    }

    fn local(tz: &TimeZone, t: Timestamp) -> String {
        t.to_zoned(tz).datetime().to_string()
    }

    fn rec(rule: Rule, start: DateTime) -> Recurrence {
        Recurrence {
            rule,
            start,
            tz: None,
        }
    }

    fn nine() -> Vec<Time> {
        vec![time(9, 0, 0, 0)]
    }

    #[test]
    fn interval_counts_real_time_across_dst() {
        let ny = zone("America/New_York");
        let r = rec(
            Rule::Interval {
                every: 60,
                unit: IntervalUnit::Minutes,
            },
            date(2026, 3, 8).at(0, 30, 0, 0),
        );
        let slots = r.upcoming(ts(&ny, date(2026, 3, 8).at(0, 0, 0, 0)), 4, &ny);
        let locals: Vec<_> = slots.iter().map(|t| local(&ny, *t)).collect();
        // 02:30 doesn't exist that night: one real hour after 01:30 is 03:30.
        assert_eq!(
            locals,
            [
                "2026-03-08T00:30:00",
                "2026-03-08T01:30:00",
                "2026-03-08T03:30:00",
                "2026-03-08T04:30:00"
            ]
        );
    }

    #[test]
    fn interval_window_is_arithmetic() {
        let utc = TimeZone::UTC;
        let r = rec(
            Rule::Interval {
                every: 15,
                unit: IntervalUnit::Minutes,
            },
            date(2026, 9, 16).at(9, 0, 0, 0),
        );
        let after = ts(&utc, date(2026, 9, 16).at(9, 0, 0, 0));
        let until = ts(&utc, date(2026, 9, 16).at(10, 7, 0, 0));
        let w = r.window(after, until, &utc).unwrap();
        assert_eq!(local(&utc, w.first), "2026-09-16T09:15:00");
        assert_eq!(local(&utc, w.last), "2026-09-16T10:00:00");
        assert_eq!(w.count, 4);
        assert_eq!(r.window(after, after.plus(MINUTE), &utc), None);
    }

    #[test]
    fn daily_multiple_times_and_start_is_respected() {
        let utc = TimeZone::UTC;
        let r = rec(
            Rule::Daily {
                every: 1,
                times: vec![time(17, 0, 0, 0), time(9, 0, 0, 0)],
            },
            date(2026, 9, 16).at(10, 0, 0, 0),
        );
        // Before the start: 09:00 on the start day is excluded, 17:00 is first.
        let slots = r.upcoming(ts(&utc, date(2026, 9, 1).at(0, 0, 0, 0)), 3, &utc);
        let locals: Vec<_> = slots.iter().map(|t| local(&utc, *t)).collect();
        assert_eq!(
            locals,
            [
                "2026-09-16T17:00:00",
                "2026-09-17T09:00:00",
                "2026-09-17T17:00:00"
            ]
        );
    }

    #[test]
    fn daily_every_three_days() {
        let utc = TimeZone::UTC;
        let r = rec(
            Rule::Daily {
                every: 3,
                times: nine(),
            },
            date(2026, 9, 16).at(9, 0, 0, 0),
        );
        let slots = r.upcoming(ts(&utc, date(2026, 9, 16).at(9, 0, 0, 0)), 2, &utc);
        assert_eq!(local(&utc, slots[0]), "2026-09-19T09:00:00");
        assert_eq!(local(&utc, slots[1]), "2026-09-22T09:00:00");
    }

    #[test]
    fn weekdays_skip_the_weekend() {
        let utc = TimeZone::UTC;
        let r = rec(
            Rule::Weekly {
                every: 1,
                weekdays: vec![1, 2, 3, 4, 5],
                times: vec![time(15, 0, 0, 0)],
            },
            date(2026, 9, 1).at(0, 0, 0, 0),
        );
        // 2026-09-18 is a Friday.
        let friday_4pm = ts(&utc, date(2026, 9, 18).at(16, 0, 0, 0));
        assert_eq!(
            local(&utc, r.next_after(friday_4pm, &utc).unwrap()),
            "2026-09-21T15:00:00"
        );
    }

    #[test]
    fn every_other_week_is_anchored_to_the_start_week() {
        let utc = TimeZone::UTC;
        // Start Wednesday 2026-09-16; Tuesdays of that week and every second week after.
        let r = rec(
            Rule::Weekly {
                every: 2,
                weekdays: vec![2],
                times: nine(),
            },
            date(2026, 9, 16).at(0, 0, 0, 0),
        );
        let slots = r.upcoming(ts(&utc, date(2026, 9, 16).at(0, 0, 0, 0)), 3, &utc);
        let locals: Vec<_> = slots.iter().map(|t| local(&utc, *t)).collect();
        // The start week's Tuesday (15th) is before the start, so it's skipped.
        assert_eq!(
            locals,
            [
                "2026-09-29T09:00:00",
                "2026-10-13T09:00:00",
                "2026-10-27T09:00:00"
            ]
        );
    }

    #[test]
    fn monthly_31st_clamps_to_month_end() {
        let utc = TimeZone::UTC;
        let r = rec(
            Rule::Monthly {
                every: 1,
                day: 31,
                times: nine(),
            },
            date(2027, 1, 1).at(0, 0, 0, 0),
        );
        let slots = r.upcoming(ts(&utc, date(2027, 1, 1).at(0, 0, 0, 0)), 4, &utc);
        let days: Vec<_> = slots
            .iter()
            .map(|t| t.to_zoned(&utc).date().to_string())
            .collect();
        assert_eq!(
            days,
            ["2027-01-31", "2027-02-28", "2027-03-31", "2027-04-30"]
        );
    }

    #[test]
    fn quarterly_on_the_15th() {
        let utc = TimeZone::UTC;
        let r = rec(
            Rule::Monthly {
                every: 3,
                day: 15,
                times: nine(),
            },
            date(2026, 11, 1).at(0, 0, 0, 0),
        );
        let slots = r.upcoming(ts(&utc, date(2026, 11, 1).at(0, 0, 0, 0)), 3, &utc);
        let days: Vec<_> = slots
            .iter()
            .map(|t| t.to_zoned(&utc).date().to_string())
            .collect();
        assert_eq!(days, ["2026-11-15", "2027-02-15", "2027-05-15"]);
    }

    #[test]
    fn yearly_feb_29_clamps_in_common_years() {
        let utc = TimeZone::UTC;
        let r = rec(
            Rule::Yearly {
                month: 2,
                day: 29,
                times: nine(),
            },
            date(2026, 1, 1).at(0, 0, 0, 0),
        );
        let slots = r.upcoming(ts(&utc, date(2026, 1, 1).at(0, 0, 0, 0)), 3, &utc);
        let days: Vec<_> = slots
            .iter()
            .map(|t| t.to_zoned(&utc).date().to_string())
            .collect();
        assert_eq!(days, ["2026-02-28", "2027-02-28", "2028-02-29"]);
    }

    #[test]
    fn daily_wall_clock_holds_across_dst_and_gap_moves_forward() {
        let ny = zone("America/New_York");
        let r = rec(
            Rule::Daily {
                every: 1,
                times: vec![time(2, 30, 0, 0)],
            },
            date(2026, 3, 6).at(0, 0, 0, 0),
        );
        let slots = r.upcoming(ts(&ny, date(2026, 3, 6).at(0, 0, 0, 0)), 4, &ny);
        let locals: Vec<_> = slots.iter().map(|t| local(&ny, *t)).collect();
        assert_eq!(
            locals,
            [
                "2026-03-06T02:30:00",
                "2026-03-07T02:30:00",
                "2026-03-08T03:30:00", // gap
                "2026-03-09T02:30:00"
            ]
        );
    }

    #[test]
    fn fold_rings_once_at_the_earlier_instant() {
        let ny = zone("America/New_York");
        let r = rec(
            Rule::Daily {
                every: 1,
                times: vec![time(1, 30, 0, 0)],
            },
            date(2026, 10, 31).at(0, 0, 0, 0),
        );
        let slots = r.upcoming(ts(&ny, date(2026, 10, 31).at(12, 0, 0, 0)), 2, &ny);
        assert_eq!(slots[0].to_jiff().to_string(), "2026-11-01T05:30:00Z");
        assert_eq!(local(&ny, slots[1]), "2026-11-02T01:30:00");
    }

    #[test]
    fn gap_that_merges_two_times_yields_one_slot() {
        let ny = zone("America/New_York");
        let r = rec(
            Rule::Daily {
                every: 1,
                times: vec![time(2, 30, 0, 0), time(3, 30, 0, 0), time(3, 15, 0, 0)],
            },
            date(2026, 3, 8).at(0, 0, 0, 0),
        );
        let w = r
            .window(
                ts(&ny, date(2026, 3, 8).at(0, 0, 0, 0)),
                ts(&ny, date(2026, 3, 8).at(4, 0, 0, 0)),
                &ny,
            )
            .unwrap();
        // 02:30 -> 03:30 merges with 03:30; 03:15 stays and sorts first.
        assert_eq!(w.count, 2);
        assert_eq!(local(&ny, w.first), "2026-03-08T03:15:00");
        assert_eq!(local(&ny, w.last), "2026-03-08T03:30:00");
    }

    #[test]
    fn window_collapses_days_of_missed_slots() {
        let utc = TimeZone::UTC;
        let r = rec(
            Rule::Daily {
                every: 1,
                times: vec![time(9, 0, 0, 0), time(21, 0, 0, 0)],
            },
            date(2026, 9, 1).at(0, 0, 0, 0),
        );
        let w = r
            .window(
                ts(&utc, date(2026, 9, 10).at(21, 0, 0, 0)),
                ts(&utc, date(2026, 9, 13).at(10, 0, 0, 0)),
                &utc,
            )
            .unwrap();
        assert_eq!(local(&utc, w.first), "2026-09-11T09:00:00");
        assert_eq!(local(&utc, w.last), "2026-09-13T09:00:00");
        assert_eq!(w.count, 5);
    }

    #[test]
    fn floating_zone_follows_the_device_explicit_zone_does_not() {
        let manila = zone("Asia/Manila");
        let ny = zone("America/New_York");
        let floating = rec(
            Rule::Daily {
                every: 1,
                times: nine(),
            },
            date(2026, 9, 1).at(0, 0, 0, 0),
        );
        let after = ts(&TimeZone::UTC, date(2026, 9, 16).at(0, 0, 0, 0));
        assert_eq!(
            local(&manila, floating.next_after(after, &manila).unwrap()),
            "2026-09-16T09:00:00"
        );
        assert_eq!(
            local(&ny, floating.next_after(after, &ny).unwrap()),
            "2026-09-16T09:00:00"
        );

        let pinned = Recurrence {
            tz: Some("Asia/Manila".into()),
            ..floating
        };
        let slot = pinned.next_after(after, &ny).unwrap();
        assert_eq!(local(&manila, slot), "2026-09-16T09:00:00");
    }

    #[test]
    fn validation() {
        let bad = [
            Rule::Interval {
                every: 0,
                unit: IntervalUnit::Minutes,
            },
            Rule::Daily {
                every: 1,
                times: vec![],
            },
            Rule::Daily {
                every: 1,
                times: vec![time(9, 0, 0, 0), time(9, 0, 0, 0)],
            },
            Rule::Weekly {
                every: 1,
                weekdays: vec![],
                times: nine(),
            },
            Rule::Weekly {
                every: 1,
                weekdays: vec![8],
                times: nine(),
            },
            Rule::Monthly {
                every: 1,
                day: 32,
                times: nine(),
            },
            Rule::Yearly {
                month: 4,
                day: 31,
                times: nine(),
            },
        ];
        for rule in bad {
            assert!(rule.validate().is_err(), "accepted {rule:?}");
        }
        assert!(Rule::Yearly {
            month: 2,
            day: 29,
            times: nine()
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn serializes_for_sync() {
        let r = Recurrence {
            rule: Rule::Weekly {
                every: 1,
                weekdays: vec![1, 3],
                times: vec![time(7, 30, 0, 0)],
            },
            start: date(2026, 9, 16).at(0, 0, 0, 0),
            tz: None,
        };
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "rule": { "kind": "weekly", "every": 1, "weekdays": [1, 3], "times": ["07:30:00"] },
                "start": "2026-09-16T00:00:00",
                "tz": null
            })
        );
        assert_eq!(serde_json::from_value::<Recurrence>(json).unwrap(), r);
    }
}
