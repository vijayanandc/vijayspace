//! weft-expr: template rendering, typed expressions, condition trees and filters (pure).
//!
//! Templates use Jinja syntax (`{{ trigger.body.email | lower }}`). A string that is exactly one
//! expression keeps the JSON type of its result; anything else renders to text. See
//! `docs/plan/04-workflow-engine.md` §3.

mod condition;
mod dates;
mod env;
mod error;
mod filters;
mod template;

pub use condition::{Comparison, Condition, ConditionResult, Op, eval_condition};
pub use error::ExprError;

/// Lenient date-time parsing (RFC 3339, `YYYY-MM-DD[ HH:MM[:SS]]`, epoch seconds or ms).
#[must_use]
pub fn parse_datetime(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    dates::parse_text(s)
}
pub use template::{
    check_template, eval_expression, referenced_steps, render_text, render_value, single_expression,
};
