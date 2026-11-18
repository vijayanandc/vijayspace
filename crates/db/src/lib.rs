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
mod orgs;
mod rows;
mod triggers;
mod usage;

use std::time::Duration;

use sqlx::{
    Executor as _,
    mysql::{MySqlPool, MySqlPoolOptions},
};

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

/// Applies pending migrations from `migrations/`.
pub async fn migrate(pool: &MySqlPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations").run(pool).await
}
