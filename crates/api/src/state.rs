//! Shared handler state.

use weft_app::Deps;

/// State passed to every handler.
#[derive(Debug, Clone)]
pub struct ApiState {
    pub deps: Deps,
}
