//! weft-testkit: deterministic test helpers (manual clock, in-memory record app, fake identity
//! provider).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod clock;
mod identity;
mod memory_app;
mod oauth_app;

pub use clock::ManualClock;
pub use identity::FakeIdentityProvider;
pub use memory_app::MemoryRecordsConnector;
pub use oauth_app::OAuthTestConnector;
