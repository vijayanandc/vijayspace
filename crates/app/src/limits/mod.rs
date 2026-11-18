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
use weft_domain::ids::{ConnectionId, OrgId};

pub use bucket::TokenBucket;
pub use manage::{LimitsView, UsageView, connection_limits, set_connection_limits, usage_today};
pub use sender::LimitedSender;

use crate::Deps;

/// Who is asking: live work (instant events, flow runs, the builder) or background work (polls,
/// backfills), which is held back once 95% of the daily budget is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestClass {
    Live,
    Background,
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
