//! Hybrid logical clock.
//!
//! Every synced write is stamped with an [`Hlc`] plus the writing device's id.
//! HLCs track wall-clock time closely (so "last writer wins" matches what a
//! person expects) but never go backwards, and they stay ahead of every stamp
//! seen from a peer even if the peer's clock is fast.
//!
//! Layout: `(physical_ms << 16) | counter`. 48 bits of milliseconds last until
//! the year 10889.

use std::cmp::Ordering;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::time::{Timestamp, HOUR};

/// Remote stamps further ahead of our clock than this are rejected.
pub const MAX_DRIFT_MS: i64 = 24 * HOUR;

const COUNTER_BITS: u32 = 16;
const COUNTER_MASK: u64 = (1 << COUNTER_BITS) - 1;
const MAX_PHYSICAL_MS: i64 = (1i64 << (64 - COUNTER_BITS)) - 1;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct Hlc(pub u64);

impl Hlc {
    pub fn new(physical_ms: i64, counter: u16) -> Self {
        let ms = physical_ms.clamp(0, MAX_PHYSICAL_MS) as u64;
        Hlc((ms << COUNTER_BITS) | u64::from(counter))
    }

    pub fn physical_ms(self) -> i64 {
        (self.0 >> COUNTER_BITS) as i64
    }

    pub fn counter(self) -> u16 {
        (self.0 & COUNTER_MASK) as u16
    }
}

impl fmt::Debug for Hlc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Hlc({:?}+{})",
            Timestamp(self.physical_ms()),
            self.counter()
        )
    }
}

/// A write's stamp: HLC first, device id breaks ties. Total order.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Stamp {
    pub hlc: Hlc,
    pub device: String,
}

impl Ord for Stamp {
    fn cmp(&self, other: &Self) -> Ordering {
        self.hlc
            .cmp(&other.hlc)
            .then_with(|| self.device.cmp(&other.device))
    }
}

impl PartialOrd for Stamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HlcError {
    #[error(
        "a synced change is {ahead_ms} ms ahead of this device's clock; check the date and time on both devices"
    )]
    Drift { ahead_ms: i64 },
}

/// The clock state for one device. Persist [`HlcClock::last`] so restarts
/// can't reissue stamps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HlcClock {
    last: Hlc,
}

impl HlcClock {
    pub fn resume(last: Hlc) -> Self {
        HlcClock { last }
    }

    pub fn last(&self) -> Hlc {
        self.last
    }

    /// Stamp for a local write at wall-clock `now`.
    pub fn tick(&mut self, now: Timestamp) -> Hlc {
        let pt = now.0.max(0);
        let last_pt = self.last.physical_ms();
        self.last = if pt > last_pt {
            Hlc::new(pt, 0)
        } else {
            match self.last.counter().checked_add(1) {
                Some(c) => Hlc::new(last_pt, c),
                // 65k writes in one millisecond: borrow the next millisecond.
                None => Hlc::new(last_pt + 1, 0),
            }
        };
        self.last
    }

    /// Folds in a stamp received from a peer so later local stamps sort after it.
    pub fn observe(&mut self, remote: Hlc, now: Timestamp) -> Result<(), HlcError> {
        let pt = now.0.max(0);
        let ahead_ms = remote.physical_ms() - pt;
        if ahead_ms > MAX_DRIFT_MS {
            return Err(HlcError::Drift { ahead_ms });
        }
        if remote > self.last {
            self.last = remote;
        }
        Ok(())
    }

    /// Checks a whole batch before any of it is applied.
    pub fn check_batch(
        &self,
        stamps: impl IntoIterator<Item = Hlc>,
        now: Timestamp,
    ) -> Result<(), HlcError> {
        let pt = now.0.max(0);
        match stamps.into_iter().map(|h| h.physical_ms() - pt).max() {
            Some(ahead_ms) if ahead_ms > MAX_DRIFT_MS => Err(HlcError::Drift { ahead_ms }),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_round_trips() {
        let h = Hlc::new(1_789_500_000_123, 42);
        assert_eq!(h.physical_ms(), 1_789_500_000_123);
        assert_eq!(h.counter(), 42);
        assert!(Hlc::new(1, 0) > Hlc::new(0, u16::MAX));
    }

    #[test]
    fn tick_is_strictly_increasing_even_when_the_clock_goes_back() {
        let mut c = HlcClock::default();
        let a = c.tick(Timestamp(10_000));
        let b = c.tick(Timestamp(10_000));
        let back = c.tick(Timestamp(5_000)); // wall clock moved backwards
        let fwd = c.tick(Timestamp(20_000));
        assert!(a < b && b < back && back < fwd);
        assert_eq!(back.physical_ms(), 10_000);
        assert_eq!(fwd, Hlc::new(20_000, 0));
    }

    #[test]
    fn counter_overflow_borrows_the_next_millisecond() {
        let mut c = HlcClock::resume(Hlc::new(1_000, u16::MAX));
        let next = c.tick(Timestamp(1_000));
        assert_eq!(next, Hlc::new(1_001, 0));
    }

    #[test]
    fn observe_keeps_local_stamps_after_a_fast_peer() {
        let mut c = HlcClock::default();
        c.tick(Timestamp(10_000));
        let remote = Hlc::new(70_000, 3); // peer is a minute ahead
        c.observe(remote, Timestamp(10_500)).unwrap();
        let local = c.tick(Timestamp(11_000));
        assert!(local > remote);
    }

    #[test]
    fn drift_guard_rejects_far_future_stamps() {
        let mut c = HlcClock::default();
        let now = Timestamp(1_000_000);
        let far = Hlc::new(now.0 + MAX_DRIFT_MS + 1, 0);
        assert!(matches!(c.observe(far, now), Err(HlcError::Drift { .. })));
        assert_eq!(
            c.last(),
            Hlc::default(),
            "a rejected stamp must not move the clock"
        );
        assert!(c.check_batch([Hlc::new(now.0, 0), far], now).is_err());
        assert!(c
            .check_batch([Hlc::new(now.0 + MAX_DRIFT_MS, 0)], now)
            .is_ok());
    }

    #[test]
    fn stamps_break_ties_by_device() {
        let a = Stamp {
            hlc: Hlc::new(5, 0),
            device: "a".into(),
        };
        let b = Stamp {
            hlc: Hlc::new(5, 0),
            device: "b".into(),
        };
        assert!(a < b);
        let later = Stamp {
            hlc: Hlc::new(5, 1),
            device: "a".into(),
        };
        assert!(later > b);
    }
}
