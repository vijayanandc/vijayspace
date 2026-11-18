//! Per-connection request limits (doc 13 §4.3): every HTTP request made through a connection
//! passes a concurrency cap, a per-minute token bucket and a daily budget. Limits are the
//! connector's defaults with the connection's overrides; background work stops at 95% of the day.

mod bucket;
mod manage;
mod sender;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Duration, Utc};
use tokio::sync::Semaphore;
use weft_connector_sdk::{ConnectionLimits, ConnectorError};
use weft_domain::ids::{ConnectionId, ExecutionId, OrgId};

pub use bucket::TokenBucket;
pub use manage::{LimitsView, UsageView, connection_limits, set_connection_limits, usage_today};
pub use sender::LimitedSender;

use crate::Deps;

/// Who is asking: live work (flow runs, the builder), background work (polls, backfills), which
/// is held back once 95% of the daily budget is used, or inbound event handling (fetching the
/// record an app just told us about), which is counted but never refused, so no event is lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestClass {
    Live,
    Background,
    Inbound,
}

/// How long a connection's effective limits are cached before being read again.
const RELOAD_SECS: i64 = 60;

/// In-process limiter state per connection: the effective limits, the minute bucket and the
/// concurrency permits. The daily budget lives in the database (shared by every process).
#[derive(Default)]
pub struct ConnectionLimiters {
    entries: Mutex<HashMap<ConnectionId, Arc<Entry>>>,
}

impl std::fmt::Debug for ConnectionLimiters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionLimiters").finish_non_exhaustive()
    }
}

pub(crate) struct Entry {
    pub limits: ConnectionLimits,
    loaded_at: Mutex<DateTime<Utc>>,
    pub bucket: Mutex<Option<TokenBucket>>,
    pub permits: Option<Arc<Semaphore>>,
}

impl Entry {
    fn new(limits: ConnectionLimits, now: DateTime<Utc>) -> Self {
        Self {
            limits,
            loaded_at: Mutex::new(now),
            bucket: Mutex::new(limits.requests_per_minute.map(|n| TokenBucket::new(n, now))),
            permits: limits
                .max_concurrent
                .map(|n| Arc::new(Semaphore::new(usize::try_from(n).unwrap_or(usize::MAX)))),
        }
    }

    fn fresh(&self, now: DateTime<Utc>) -> bool {
        self.loaded_at
            .lock()
            .is_ok_and(|t| now - *t < Duration::seconds(RELOAD_SECS))
    }
}

impl ConnectionLimiters {
    /// Forgets a connection's cached limits (after they were changed).
    pub fn invalidate(&self, id: ConnectionId) {
        if let Ok(mut map) = self.entries.lock() {
            map.remove(&id);
        }
    }

    fn cached(&self, id: ConnectionId, now: DateTime<Utc>) -> Option<Arc<Entry>> {
        let map = self.entries.lock().ok()?;
        map.get(&id).filter(|e| e.fresh(now)).cloned()
    }

    /// Stores freshly loaded limits; unchanged limits keep the bucket and permits.
    fn store(&self, id: ConnectionId, limits: ConnectionLimits, now: DateTime<Utc>) -> Arc<Entry> {
        let Ok(mut map) = self.entries.lock() else {
            return Arc::new(Entry::new(limits, now));
        };
        if let Some(existing) = map.get(&id).filter(|e| e.limits == limits) {
            if let Ok(mut t) = existing.loaded_at.lock() {
                *t = now;
            }
            return Arc::clone(existing);
        }
        let entry = Arc::new(Entry::new(limits, now));
        map.insert(id, Arc::clone(&entry));
        entry
    }

    /// The effective limits of a connection (cached for a minute).
    pub(crate) async fn entry(
        &self,
        deps: &Deps,
        org: OrgId,
        id: ConnectionId,
    ) -> Result<Arc<Entry>, ConnectorError> {
        let now = deps.clock.now();
        if let Some(entry) = self.cached(id, now) {
            return Ok(entry);
        }
        let limits = manage::effective_limits(deps, org, id)
            .await
            .map_err(|e| ConnectorError::transient(format!("loading connection limits: {e}")))?;
        Ok(self.store(id, limits, now))
    }
}

/// How long a step held back by a limit waits: the limiter's hint (at least 5 s), plus a small
/// spread derived from the execution ID (up to 10% of the hint, at most 5 minutes), so parked runs
/// do not all wake together but still wake soon after the limit allows (e.g. just after midnight
/// for a used-up daily budget).
#[must_use]
pub fn park_delay(execution: ExecutionId, retry_after_secs: u64) -> Duration {
    let base = retry_after_secs.clamp(5, 86_400);
    let max_spread = (base / 10).clamp(1, 300);
    let spread = u64::from(execution.as_uuid().as_bytes()[15]) * max_spread / 255;
    Duration::seconds(i64::try_from(base + spread).unwrap_or(86_400))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parked_runs_wait_the_hint_plus_a_small_capped_spread() {
        let id = |last: u8| {
            let mut bytes = [0_u8; 16];
            bytes[15] = last;
            ExecutionId::from_uuid(uuid::Uuid::from_bytes(bytes))
        };
        assert_eq!(park_delay(id(0), 60), Duration::seconds(60));
        assert_eq!(park_delay(id(255), 60), Duration::seconds(66));
        // A used-up daily budget at 00:30: wake at most 5 minutes after midnight.
        assert_eq!(park_delay(id(255), 84_600), Duration::seconds(84_900));
        assert_eq!(
            park_delay(id(0), 0),
            Duration::seconds(5),
            "never a busy loop"
        );
        assert_ne!(
            park_delay(id(10), 60),
            park_delay(id(200), 60),
            "spread by run"
        );
    }
}
