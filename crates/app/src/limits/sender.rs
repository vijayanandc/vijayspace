//! An `HttpSender` that applies a connection's limits before every request.

use std::{sync::Arc, time::Duration as StdDuration};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use weft_connector_sdk::{ConnectorError, HttpRequest, HttpResponse, HttpSender};
use weft_domain::ids::{ConnectionId, OrgId};

use super::RequestClass;
use crate::Deps;

/// How long a request waits for a concurrency slot before its step is parked and retried later
/// (short, so a busy connection never ties up worker tasks).
const SLOT_WAIT: StdDuration = StdDuration::from_secs(2);

/// Wraps the platform sender for one connection and one class of work.
pub struct LimitedSender {
    pub(crate) deps: Deps,
    pub(crate) org: OrgId,
    pub(crate) connection: ConnectionId,
    pub(crate) class: RequestClass,
}

impl std::fmt::Debug for LimitedSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LimitedSender")
            .field("connection", &self.connection)
            .field("class", &self.class)
            .finish_non_exhaustive()
    }
}

fn limited(retry_after_secs: u64, message: String) -> ConnectorError {
    ConnectorError::Throttled {
        retry_after_secs,
        message,
    }
}

/// Seconds until the next UTC midnight (when the daily budget resets).
pub(crate) fn secs_to_midnight(now: DateTime<Utc>) -> u64 {
    let next = (now.date_naive() + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .map(|t| t.and_utc());
    next.map_or(3600, |t| {
        u64::try_from((t - now).num_seconds())
            .unwrap_or(3600)
            .max(1)
    })
}

/// The share of the daily budget a class may use: background work stops at 95%.
/// Inbound events are never refused (refusing would lose the event); they are only counted.
pub(crate) fn cap_for(daily: Option<u32>, class: RequestClass) -> Option<u32> {
    match class {
        RequestClass::Inbound => None,
        RequestClass::Live => daily,
        RequestClass::Background => daily.map(|cap| cap.saturating_mul(95) / 100),
    }
}

#[async_trait]
impl HttpSender for LimitedSender {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ConnectorError> {
        let entry = self
            .deps
            .limiters
            .entry(&self.deps, self.org, self.connection)
            .await?;
        // Inbound event fetches never wait or get refused: refusing would lose the event.
        let permits = entry
            .permits
            .as_ref()
            .filter(|_| self.class != RequestClass::Inbound);
        let _slot = match permits {
            Some(permits) => {
                match tokio::time::timeout(SLOT_WAIT, Arc::clone(permits).acquire_owned()).await {
                    Ok(Ok(permit)) => Some(permit),
                    Ok(Err(_)) | Err(_) => {
                        return Err(limited(
                            10,
                            "connection.limit: too many requests in flight on this connection"
                                .into(),
                        ));
                    }
                }
            }
            None => None,
        };
        let now = self.deps.clock.now();
        if self.class != RequestClass::Inbound
            && let Ok(mut bucket) = entry.bucket.lock()
            && let Some(bucket) = bucket.as_mut()
            && let Err(wait) = bucket.take(now)
        {
            let per_minute = entry.limits.requests_per_minute.unwrap_or(0);
            return Err(limited(
                wait,
                format!("connection.limit: {per_minute} requests per minute"),
            ));
        }
        let cap = cap_for(entry.limits.daily_requests, self.class);
        let allowed = self
            .deps
            .store
            .count_connection_request(self.org, self.connection, now.date_naive(), cap, now)
            .await
            .map_err(|e| ConnectorError::transient(format!("counting the request: {e}")))?;
        if !allowed {
            let why = match self.class {
                RequestClass::Live | RequestClass::Inbound => "the daily request budget is used up",
                RequestClass::Background => {
                    "95% of the daily request budget is used; background work waits"
                }
            };
            return Err(limited(
                secs_to_midnight(now),
                format!("connection.limit: {why}"),
            ));
        }
        self.deps.http.send(request).await
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    #[test]
    fn background_work_keeps_five_percent_back_and_waits_until_midnight() {
        assert_eq!(cap_for(Some(10_000), RequestClass::Live), Some(10_000));
        assert_eq!(cap_for(Some(10_000), RequestClass::Background), Some(9_500));
        assert_eq!(cap_for(None, RequestClass::Background), None);
        assert_eq!(
            cap_for(Some(10), RequestClass::Inbound),
            None,
            "inbound events are only counted"
        );
        let t = Utc.with_ymd_and_hms(2026, 1, 1, 23, 59, 0).unwrap();
        assert_eq!(secs_to_midnight(t), 60);
    }
}
