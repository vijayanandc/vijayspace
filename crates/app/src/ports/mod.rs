//! Ports: the interfaces use cases depend on. `weft-db` and `weft-infra` implement them.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::AppError;

/// Result alias for port methods.
pub type Res<T> = Result<T, AppError>;

/// Wall clock (truncated to microseconds). Tests use a manual clock.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// `UUIDv7` generator. Tests use a deterministic sequence.
pub trait IdGen: Send + Sync {
    fn uuid(&self) -> Uuid;
}

/// Authenticated encryption for connection secrets (AAD binds a ciphertext to its row).
pub trait SecretBox: Send + Sync {
    /// Returns `(ciphertext, key_id)`.
    fn seal(&self, plaintext: &[u8], aad: &[u8]) -> Res<(Vec<u8>, String)>;
    fn open(&self, ciphertext: &[u8], key_id: &str, aad: &[u8]) -> Res<Vec<u8>>;
}

mod connections;
mod executions;
mod flows;
mod jobs;
mod limits;
mod links;
mod retention;
mod tenancy;
mod triggers;

pub use connections::*;
pub use executions::*;
pub use flows::*;
pub use jobs::*;
pub use limits::*;
pub use links::*;
pub use retention::*;
pub use tenancy::*;
pub use triggers::*;

/// Everything persistent, implemented by one MySQL adapter.
pub trait Store:
    OrgRepo
    + FlowRepo
    + TriggerRepo
    + JobQueue
    + ExecutionRepo
    + ConnectionRepo
    + LinkRepo
    + LimitRepo
    + RetentionRepo
    + TenancyRepo
    + Send
    + Sync
{
}

impl<T> Store for T where
    T: OrgRepo
        + FlowRepo
        + TriggerRepo
        + JobQueue
        + ExecutionRepo
        + ConnectionRepo
        + LinkRepo
        + LimitRepo
        + RetentionRepo
        + TenancyRepo
        + Send
        + Sync
{
}
