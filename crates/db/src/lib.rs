//! weft-db: MySQL implementations of every port in `weft-app::ports`, the job queue and migrations.
//!
//! All SQL lives here. Every tenant query filters on `org_id`; `*_system` reads are for workers.
//! Times are always bound from the app clock (never `NOW()`).

mod codec;
mod connections;
mod events;
mod executions;
mod flows;
mod jobs;
mod limits;
mod links;
mod logs;
mod members;
mod ops;
mod orgs;
pub mod partitions;
mod quotas;
mod retention;
mod rows;
mod tenancy;
mod tenancy_rows;
mod triggers;
mod usage;

use std::time::Duration;

use sqlx::{
    Executor as _,
    mysql::{MySqlPool, MySqlPoolOptions},
};

/// A pool of MySQL connections (what `connect` returns).
pub type Pool = MySqlPool;

/// The MySQL adapter (cheap to clone).
#[derive(Debug, Clone)]
pub struct MySqlStore {
    pool: MySqlPool,
}

impl MySqlStore {
    #[must_use]
    pub const fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }

    #[must_use]
    pub const fn pool(&self) -> &MySqlPool {
        &self.pool
    }
}

/// Opens a pool with UTC sessions and READ COMMITTED isolation (avoids gap locks on claims).
pub async fn connect(url: &str, max_connections: u32) -> Result<MySqlPool, sqlx::Error> {
    MySqlPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(10))
        // No ping before every query (a round trip each): connections are recycled instead, and a broken one
        // fails its query, which callers already retry or report.
        .test_before_acquire(false)
        .max_lifetime(Duration::from_mins(30))
        .idle_timeout(Duration::from_mins(10))
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                conn.execute("SET time_zone = '+00:00'").await?;
                conn.execute("SET SESSION TRANSACTION ISOLATION LEVEL READ COMMITTED")
                    .await?;
                Ok(())
            })
        })
        .connect(url)
        .await
}

/// MySQL's connection limit and how many connections are open now: every server opens up to its pool size, so
/// the pools of all servers together must stay below the limit (or new connections fail with "Too many
/// connections").
pub async fn connection_headroom(pool: &MySqlPool) -> Result<(u32, u32), sqlx::Error> {
    let max: u32 = sqlx::query_scalar("SELECT CAST(@@max_connections AS UNSIGNED)")
        .fetch_one(pool)
        .await?;
    let open: (String, String) = sqlx::query_as("SHOW GLOBAL STATUS LIKE 'Threads_connected'")
        .fetch_one(pool)
        .await?;
    Ok((max, open.1.parse().unwrap_or(0)))
}

/// Applies pending migrations from `migrations/`.
pub async fn migrate(pool: &MySqlPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations").run(pool).await
}
