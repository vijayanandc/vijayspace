//! Running several servers on one database: each process heartbeats into `instances`, learns how many
//! servers are live (per-connection concurrency caps are shared between them), and staff can see the
//! servers and the job queue's lag (`GET /api/v1/ops/status`), which tells when to add workers.

use std::sync::atomic::{AtomicU32, Ordering};

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use uuid::Uuid;
use weft_domain::permissions::Permission;

use crate::{
    AppError, Deps,
    ports::{Instance, QueueStats},
    tenancy::Actor,
};

/// How often each server heartbeats.
pub const HEARTBEAT_SECS: u64 = 15;
/// A server not seen for this long is no longer counted (three missed heartbeats).
const LIVE_WITHIN_SECS: i64 = 45;

/// The number of live servers that run jobs, as last seen by this one (at least 1).
#[derive(Debug)]
pub struct LiveInstances {
    count: AtomicU32,
}

impl Default for LiveInstances {
    fn default() -> Self {
        Self {
            count: AtomicU32::new(1),
        }
    }
}

impl LiveInstances {
    #[must_use]
    pub fn count(&self) -> u32 {
        self.count.load(Ordering::Relaxed).max(1)
    }

    pub fn set(&self, n: u32) {
        self.count.store(n.max(1), Ordering::Relaxed);
    }
}

/// This server's share of a cap that holds across every server: `cap / live`, at least 1.
#[must_use]
pub const fn share(cap: u32, live: u32) -> u32 {
    let live = if live == 0 { 1 } else { live };
    let each = cap / live;
    if each == 0 { 1 } else { each }
}

/// This process, as other servers see it.
#[derive(Debug, Clone)]
pub struct Me {
    pub id: Uuid,
    pub kind: String,
    pub host: String,
    pub version: String,
    pub started_at: DateTime<Utc>,
}

/// Records that this server is alive and refreshes the count of live servers that run jobs (`worker` or
/// `all`; at least 1). Returns that count.
pub async fn heartbeat(deps: &Deps, me: &Me) -> Result<u32, AppError> {
    let now = deps.clock.now();
    deps.store
        .instance_heartbeat(&Instance {
            id: me.id,
            kind: me.kind.clone(),
            host: me.host.clone(),
            version: me.version.clone(),
            started_at: me.started_at,
            seen_at: now,
        })
        .await?;
    let live = deps
        .store
        .live_instances(now - Duration::seconds(LIVE_WITHIN_SECS))
        .await?;
    // Concurrency is shared between the servers that run jobs; API-only servers make few app calls
    // (test steps, pickers) and would otherwise halve every worker's share.
    let workers = live.iter().filter(|i| i.kind != "api").count();
    let n = u32::try_from(workers).unwrap_or(u32::MAX).max(1);
    if n != deps.instances.count() {
        tracing::info!(
            live = live.len(),
            workers = n,
            "servers sharing the database"
        );
    }
    deps.instances.set(n);
    Ok(n)
}

/// Removes this server on a clean shutdown, so the others take over its share at once.
pub async fn leave(deps: &Deps, me: &Me) {
    if let Err(e) = deps.store.remove_instance(me.id).await {
        tracing::warn!(error = %e, "removing this server from the instance list");
    }
}

/// Lag of one queue: how long its oldest due job has waited.
#[derive(Debug, Clone, Serialize)]
pub struct QueueLag {
    pub queue: String,
    pub ready: u64,
    pub lag_seconds: i64,
}

/// What staff see: live servers and the queue.
#[derive(Debug, Clone, Serialize)]
pub struct OpsStatus {
    pub now: DateTime<Utc>,
    pub instances: Vec<Instance>,
    pub queues: Vec<QueueLag>,
    pub running_jobs: u64,
    pub dead_jobs: u64,
}

/// Live servers and queue lag. Staff only: owners and admins of the default (operator's) org.
pub async fn status(deps: &Deps, actor: &Actor) -> Result<OpsStatus, AppError> {
    require_staff(deps, actor)?;
    let now = deps.clock.now();
    let instances = deps
        .store
        .live_instances(now - Duration::seconds(LIVE_WITHIN_SECS))
        .await?;
    let QueueStats {
        queues,
        running,
        dead,
    } = deps.store.queue_stats(now).await?;
    Ok(OpsStatus {
        now,
        instances,
        queues: queues
            .into_iter()
            .map(|q| QueueLag {
                lag_seconds: q.oldest_due.map_or(0, |t| (now - t).num_seconds().max(0)),
                queue: q.queue,
                ready: q.ready,
            })
            .collect(),
        running_jobs: running,
        dead_jobs: dead,
    })
}

/// Staff are owners and admins of the default org (the operator's own org).
pub fn require_staff(deps: &Deps, actor: &Actor) -> Result<(), AppError> {
    if actor.org != deps.config.default_org_id {
        return Err(AppError::forbidden(
            "Only the operator's staff can see this",
        ));
    }
    actor.require(Permission::OrgManage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cap_is_shared_between_live_servers_and_never_drops_to_zero() {
        assert_eq!(share(10, 1), 10);
        assert_eq!(share(10, 3), 3, "3 × 3 stays within 10");
        assert_eq!(share(2, 3), 1, "every server may still make one request");
        assert_eq!(share(10, 0), 10);
        let live = LiveInstances::default();
        assert_eq!(live.count(), 1);
        live.set(0);
        assert_eq!(live.count(), 1);
        live.set(4);
        assert_eq!(live.count(), 4);
    }
}
