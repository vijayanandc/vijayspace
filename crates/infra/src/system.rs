//! The real clock and ID generator.

use chrono::{DateTime, Utc};
use weft_app::ports::{Clock, IdGen};

/// UTC wall clock truncated to microseconds (round-trips through `DATETIME(6)`).
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        weft_domain::truncate_micros(Utc::now())
    }
}

/// Time-ordered `UUIDv7` IDs.
#[derive(Debug, Clone, Copy, Default)]
pub struct UuidV7Ids;

impl IdGen for UuidV7Ids {
    fn uuid(&self) -> uuid::Uuid {
        uuid::Uuid::now_v7()
    }
}
