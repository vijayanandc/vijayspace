//! weft-app: use cases, ports, `Deps`, `AppError` and the execution runner.
//!
//! One use case per file under its feature folder. Use cases take `&Deps` and an org ID,
//! scope every read by org, and never call `tokio::spawn`.

pub mod connections;
pub mod deps;
pub mod error;
pub mod executions;
pub mod flows;
pub mod ids;
pub mod limits;
pub mod org;
pub mod ports;
pub mod recipes;
pub mod runner;
pub mod sync;
pub mod triggers;

pub use deps::{AppConfig, Deps};
pub use error::{AppError, ErrorKind};
