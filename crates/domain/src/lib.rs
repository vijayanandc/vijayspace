//! weft-domain: IDs, entities, `FlowDefinition` and its validation, schedules, error codes (pure).
//!
//! No IO and no async: everything here is plain data and deterministic rules.

pub mod error_codes;
pub mod flow;
pub mod ids;
pub mod model;
pub mod schedule;
pub mod text;

/// UTC timestamp with microsecond precision (matches MySQL `DATETIME(6)`).
pub type Timestamp = chrono::DateTime<chrono::Utc>;

/// Truncates a timestamp to whole microseconds so it round-trips through `DATETIME(6)`.
#[must_use]
pub fn truncate_micros(t: Timestamp) -> Timestamp {
    use chrono::Timelike as _;
    let micros = t.nanosecond() / 1_000;
    t.with_nanosecond(micros * 1_000).unwrap_or(t)
}
