//! The execution runner: drives `weft-engine` with real IO (the imperative shell).
//! See `docs/plan/04-workflow-engine.md` §6.

pub mod action;
pub mod plan_cache;
mod run;

pub use action::{call_action, step_error};
pub use run::{JobOutcome, infra_backoff, run_execution};
