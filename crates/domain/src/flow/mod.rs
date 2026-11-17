//! Flow definitions and their validation.

pub mod definition;
pub mod validate;

pub use definition::*;
pub use validate::{ActionInfo, Catalog, Issue, Severity, has_errors, valid_key, validate};
