//! Instants, time zones and clocks.
//!
//! Everything Dun stores is a UTC [`Timestamp`] in milliseconds. Civil (wall
//! clock) times only exist inside recurrence rules and quiet hours, and are
//! turned into instants with an explicit [`TimeZone`] at evaluation time.

use std::fmt;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

pub use jiff::tz::TimeZone;

pub const SECOND: i64 = 1_000;
pub const MINUTE: i64 = 60 * SECOND;
pub const HOUR: i64 = 60 * MINUTE;
pub const DAY: i64 = 24 * HOUR;

/// A UTC instant with millisecond precision.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(pub i64);

impl Timestamp {
    pub const MIN: Timestamp = Timestamp(i64::MIN / 4);
    pub const MAX: Timestamp = Timestamp(i64::MAX / 4);

    pub const fn from_ms(ms: i64) -> Self {
        Timestamp(ms)
    }

    pub const fn ms(self) -> i64 {
        self.0
    }

    pub fn plus(self, ms: i64) -> Self {
        Timestamp(self.0.saturating_add(ms))
    }

    pub fn minus(self, ms: i64) -> Self {
        Timestamp(self.0.saturating_sub(ms))
    }

    pub fn plus_minutes(self, minutes: i64) -> Self {
        self.plus(minutes.saturating_mul(MINUTE))
    }

    /// `self - other` in milliseconds.
    pub fn since(self, other: Timestamp) -> i64 {
        self.0.saturating_sub(other.0)
    }

    pub fn to_jiff(self) -> jiff::Timestamp {
        jiff::Timestamp::from_millisecond(self.0).unwrap_or(if self.0 < 0 {
            jiff::Timestamp::MIN
        } else {
            jiff::Timestamp::MAX
        })
    }

    pub fn from_jiff(t: jiff::Timestamp) -> Self {
        Timestamp(t.as_millisecond())
    }

    pub fn to_zoned(self, tz: &TimeZone) -> jiff::Zoned {
        self.to_jiff().to_zoned(tz.clone())
    }
}

impl fmt::Debug for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match jiff::Timestamp::from_millisecond(self.0) {
            Ok(t) => write!(f, "{t}"),
            Err(_) => write!(f, "Timestamp({})", self.0),
        }
    }
}

/// Converts a civil date-time to an instant, resolving DST the same way
/// everywhere in Dun: a time inside a spring-forward gap moves forward by the
/// gap (02:30 becomes 03:30); a time inside a fall-back fold uses the earlier
/// instant.
pub fn resolve(dt: jiff::civil::DateTime, tz: &TimeZone) -> Timestamp {
    match tz.to_ambiguous_zoned(dt).compatible() {
        Ok(z) => Timestamp::from_jiff(z.timestamp()),
        // Only possible at the edges of jiff's supported range.
        Err(_) => {
            if dt.date().year() < 0 {
                Timestamp::MIN
            } else {
                Timestamp::MAX
            }
        }
    }
}

/// Resolves an IANA zone name, falling back to UTC for unknown names so a bad
/// value synced from another device can never stop evaluation.
pub fn zone(name: &str) -> TimeZone {
    TimeZone::get(name).unwrap_or(TimeZone::UTC)
}

/// Source of "now" and the device's current zone.
pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
    fn tz(&self) -> TimeZone;
}

/// The real clock. `offset_ms` shifts "now" for skew and time-jump testing; it
/// is only ever non-zero in debug builds (see [`SystemClock::from_env`]).
#[derive(Debug, Default)]
pub struct SystemClock {
    offset_ms: i64,
}

impl SystemClock {
    pub fn new() -> Self {
        Self::default()
    }

    /// Honors `DUN_CLOCK_OFFSET_MS` in debug builds only.
    pub fn from_env() -> Self {
        let offset_ms = if cfg!(debug_assertions) {
            std::env::var("DUN_CLOCK_OFFSET_MS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0)
        } else {
            0
        };
        SystemClock { offset_ms }
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_jiff(jiff::Timestamp::now()).plus(self.offset_ms)
    }

    fn tz(&self) -> TimeZone {
        TimeZone::system()
    }
}

/// A clock tests move by hand.
#[derive(Debug)]
pub struct FakeClock {
    now: AtomicI64,
    tz: Mutex<TimeZone>,
}

impl FakeClock {
    pub fn new(now: Timestamp, tz: TimeZone) -> Self {
        FakeClock {
            now: AtomicI64::new(now.0),
            tz: Mutex::new(tz),
        }
    }

    /// A clock at the given local civil time in the named zone.
    pub fn at_local(tz_name: &str, dt: jiff::civil::DateTime) -> Self {
        let tz = TimeZone::get(tz_name).expect("test time zone must exist");
        let now = resolve(dt, &tz);
        Self::new(now, tz)
    }

    pub fn advance(&self, ms: i64) {
        self.now.fetch_add(ms, Ordering::SeqCst);
    }

    pub fn set(&self, now: Timestamp) {
        self.now.store(now.0, Ordering::SeqCst);
    }

    pub fn set_tz(&self, tz: TimeZone) {
        *self.tz.lock().unwrap_or_else(|p| p.into_inner()) = tz;
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Timestamp {
        Timestamp(self.now.load(Ordering::SeqCst))
    }

    fn tz(&self) -> TimeZone {
        self.tz.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    #[test]
    fn dst_gap_moves_forward_and_fold_takes_earlier() {
        let ny = zone("America/New_York");
        // 2026-03-08 02:30 does not exist in New York (clocks jump 02:00 -> 03:00).
        let gap = resolve(date(2026, 3, 8).at(2, 30, 0, 0), &ny);
        let three_thirty = resolve(date(2026, 3, 8).at(3, 30, 0, 0), &ny);
        assert_eq!(gap, three_thirty);

        // 2026-11-01 01:30 happens twice; we take the first (EDT, UTC-4).
        let fold = resolve(date(2026, 11, 1).at(1, 30, 0, 0), &ny);
        assert_eq!(fold.to_jiff().to_string(), "2026-11-01T05:30:00Z");
    }

    #[test]
    fn unknown_zone_falls_back_to_utc() {
        assert_eq!(zone("Not/AZone"), TimeZone::UTC);
    }

    #[test]
    fn fake_clock_moves() {
        let c = FakeClock::at_local("UTC", date(2026, 9, 16).at(9, 0, 0, 0));
        let start = c.now();
        c.advance(5 * MINUTE);
        assert_eq!(c.now().since(start), 5 * MINUTE);
        c.set_tz(zone("Asia/Manila"));
        assert_eq!(c.tz(), zone("Asia/Manila"));
    }

    #[test]
    fn saturating_arithmetic_never_overflows() {
        assert_eq!(Timestamp::MAX.plus(i64::MAX).0, i64::MAX);
        assert_eq!(Timestamp::MIN.minus(i64::MAX).0, i64::MIN);
    }

    #[test]
    fn debug_prints_iso() {
        assert_eq!(format!("{:?}", Timestamp(0)), "1970-01-01T00:00:00Z");
    }
}
