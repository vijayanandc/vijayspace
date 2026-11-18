//! Route table: one module per feature.

mod catalog;
mod connections;
mod executions;
mod flows;
mod hooks;
mod members;
mod misc;
mod orgs;
mod recipes;

use axum::Router;

use crate::state::ApiState;

pub fn router(state: &ApiState) -> Router<ApiState> {
    Router::new()
        .merge(misc::router())
        .merge(orgs::router())
        .merge(members::router())
        .merge(catalog::router())
        .merge(connections::router())
        .merge(flows::router())
        .merge(executions::router())
        .merge(recipes::router())
        .merge(hooks::router(state))
}
