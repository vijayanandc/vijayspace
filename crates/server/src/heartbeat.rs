//! This process's heartbeat into `instances` (every 15 s until shutdown, then it leaves the list), so servers
//! sharing one database know how many of them are live and staff can see them.

use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use weft_app::{
    Deps,
    instances::{self, HEARTBEAT_SECS, Me},
};

/// This process as other servers see it: `kind` is `all`, `api` or `worker`.
#[must_use]
pub fn me(deps: &Deps, kind: &str) -> Me {
    Me {
        id: deps.ids.uuid(),
        kind: kind.to_owned(),
        host: host_name(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        started_at: deps.clock.now(),
    }
}

/// Heartbeats until `shutdown`, then removes this server from the list.
pub fn spawn(deps: Deps, me: Me, shutdown: CancellationToken) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut every = tokio::time::interval(Duration::from_secs(HEARTBEAT_SECS));
        loop {
            tokio::select! {
                () = shutdown.cancelled() => break,
                _ = every.tick() => {
                    if let Err(e) = instances::heartbeat(&deps, &me).await {
                        tracing::warn!(error = %e, "instance heartbeat");
                    }
                }
            }
        }
        instances::leave(&deps, &me).await;
    })
}

/// `HOSTNAME` (set in containers), else `/etc/hostname`, else `unknown`.
fn host_name() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .map(|h| h.trim().chars().take(191).collect::<String>())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}
