//! weft-script: custom functions in a sandboxed Rhai engine (doc 07 §9, doc 08 §7).
//!
//! A script sees `input` (a map) and `context` (a map), may `print`, and its last expression is the
//! result. There is no clock, file, network or environment access: only the helpers registered in
//! [`helpers`]. Every run gets a fresh engine with operation, depth and size limits.

pub mod child;
mod engine;
mod error;
mod helpers;

pub use engine::{Limits, ScriptOutput, check, run};
pub use error::ScriptError;

#[cfg(test)]
mod tests;
