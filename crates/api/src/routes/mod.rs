//! Route table: one module per feature.

mod catalog;
mod connections;
mod executions;
mod flows;
mod hooks;
mod misc;
mod recipes;

use axum::Router;

use crate::state::ApiState;

pub fn router() -> Router<ApiState> {
    Router::new()
        .merge(misc::router())
        .merge(catalog::router())
        .merge(connections::router())
        .merge(flows::router())
        .merge(executions::router())
        .merge(recipes::router())
        .merge(hooks::router())
}
