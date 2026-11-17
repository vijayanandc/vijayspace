//! weft-testkit: deterministic test helpers (manual clock, in-memory record app).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod clock;
mod memory_app;

pub use clock::ManualClock;
pub use memory_app::MemoryRecordsConnector;
