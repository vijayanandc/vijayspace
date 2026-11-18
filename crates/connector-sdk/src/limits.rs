//! Per-connection request limits: connector defaults that a connection can override.

use serde::{Deserialize, Serialize};

/// Limits on requests made through one connection. `None` means no limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ConnectionLimits {
    /// Requests per minute (a token bucket that refills evenly).
    #[serde(default)]
    pub requests_per_minute: Option<u32>,
    /// Requests in flight at once.
    #[serde(default)]
    pub max_concurrent: Option<u32>,
    /// Requests per UTC day. Background work (polls, backfills) stops at 95% of it.
    #[serde(default)]
    pub daily_requests: Option<u32>,
}

impl ConnectionLimits {
    /// `self` (the defaults) with `overrides` applied: a field set there wins, and `0` there
    /// means "no limit".
    #[must_use]
    pub fn with_overrides(self, overrides: &Self) -> Self {
        let pick = |default: Option<u32>, over: Option<u32>| match over {
            Some(0) => None,
            Some(n) => Some(n),
            None => default,
        };
        Self {
            requests_per_minute: pick(self.requests_per_minute, overrides.requests_per_minute),
            max_concurrent: pick(self.max_concurrent, overrides.max_concurrent),
            daily_requests: pick(self.daily_requests, overrides.daily_requests),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_win_zero_removes_a_limit_and_unset_keeps_the_default() {
        let defaults = ConnectionLimits {
            requests_per_minute: Some(300),
            max_concurrent: Some(10),
            daily_requests: Some(10_000),
        };
        let overrides = ConnectionLimits {
            requests_per_minute: Some(60),
            max_concurrent: None,
            daily_requests: Some(0),
        };
        assert_eq!(
            defaults.with_overrides(&overrides),
            ConnectionLimits {
                requests_per_minute: Some(60),
                max_concurrent: Some(10),
                daily_requests: None,
            }
        );
    }
}
