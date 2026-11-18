//! A token bucket for "N requests per minute", in integer units on the injected clock.

use chrono::{DateTime, Utc};

/// One token is worth `UNIT` units; the bucket refills `per_minute` units every millisecond,
/// i.e. `per_minute` tokens per minute, evenly. Integer math keeps it exact and deterministic.
const UNIT: u64 = 60_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenBucket {
    per_minute: u64,
    units: u64,
    last: DateTime<Utc>,
}

impl TokenBucket {
    /// A full bucket (a burst of up to `per_minute` requests is allowed).
    #[must_use]
    pub fn new(per_minute: u32, now: DateTime<Utc>) -> Self {
        let per_minute = u64::from(per_minute.max(1));
        Self {
            per_minute,
            units: per_minute * UNIT,
            last: now,
        }
    }

    /// Takes one token, or returns how many whole seconds until one is available.
    pub fn take(&mut self, now: DateTime<Utc>) -> Result<(), u64> {
        let elapsed_ms = u64::try_from((now - self.last).num_milliseconds()).unwrap_or(0);
        if elapsed_ms > 0 {
            let capacity = self.per_minute * UNIT;
            self.units = self
                .units
                .saturating_add(elapsed_ms.saturating_mul(self.per_minute))
                .min(capacity);
            self.last = now;
        }
        if self.units >= UNIT {
            self.units -= UNIT;
            return Ok(());
        }
        let missing_ms = (UNIT - self.units).div_ceil(self.per_minute);
        Err(missing_ms.div_ceil(1000).max(1))
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone as _};

    use super::*;

    #[test]
    fn allows_a_burst_then_refills_evenly() {
        let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let mut b = TokenBucket::new(2, t0);
        assert!(b.take(t0).is_ok());
        assert!(b.take(t0).is_ok());
        assert_eq!(b.take(t0), Err(30), "2 per minute: one token every 30 s");
        assert_eq!(b.take(t0 + Duration::seconds(10)), Err(20));
        assert!(b.take(t0 + Duration::seconds(30)).is_ok());
        // Idle time never fills the bucket beyond its capacity.
        let later = t0 + Duration::hours(1);
        assert!(b.take(later).is_ok());
        assert!(b.take(later).is_ok());
        assert!(b.take(later).is_err());
    }

    #[test]
    fn a_clock_going_backwards_does_not_add_tokens() {
        let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let mut b = TokenBucket::new(1, t0);
        assert!(b.take(t0).is_ok());
        assert!(b.take(t0 - Duration::seconds(120)).is_err());
    }
}
