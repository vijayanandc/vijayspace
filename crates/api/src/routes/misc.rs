//! Health, org and usage.

use axum::{Json, Router, extract::State, routing::get};
use serde_json::{Value, json};

use crate::{error::ApiResult, extract::Org, state::ApiState};

pub fn router() -> Router<ApiState> {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(ready))
        .route("/api/v1/org", get(org).patch(update_org))
        .route("/api/v1/usage", get(usage))
}

async fn ready(State(s): State<ApiState>) -> ApiResult<&'static str> {
    weft_app::org::resolve_org(&s.deps, None).await?;
    Ok("ok")
}

/// The org plus its effective retention and the server default.
fn org_json(deps: &weft_app::Deps, o: &weft_domain::model::Org) -> Value {
    let mut v = json!(o);
    v["retention_days_effective"] = json!(weft_app::retention::effective_days(deps, o));
    v["retention_days_default"] = json!(deps.config.default_retention_days);
    v
}

async fn org(State(s): State<ApiState>, Org(org): Org) -> ApiResult<Json<Value>> {
    let o = s.deps.store.get_org(org).await?;
    Ok(Json(o.map_or(Value::Null, |o| org_json(&s.deps, &o))))
}

#[derive(Debug, serde::Deserialize)]
struct OrgBody {
    /// Absent = unchanged; `null` = the server default.
    #[allow(clippy::option_option)]
    #[serde(default, deserialize_with = "present")]
    retention_days: Option<Option<u32>>,
}

#[allow(clippy::option_option)]
fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<u32>>, D::Error> {
    Ok(Some(<Option<u32> as serde::Deserialize>::deserialize(d)?))
}

async fn update_org(
    State(s): State<ApiState>,
    Org(org): Org,
    Json(b): Json<OrgBody>,
) -> ApiResult<Json<Value>> {
    if let Some(days) = b.retention_days {
        weft_app::retention::set_retention_days(&s.deps, org, days).await?;
    }
    let o = s.deps.store.get_org(org).await?;
    Ok(Json(o.map_or(Value::Null, |o| org_json(&s.deps, &o))))
}

async fn usage(State(s): State<ApiState>, Org(org): Org) -> ApiResult<Json<Value>> {
    let (period, tasks, executions) = weft_app::executions::usage(&s.deps, org).await?;
    Ok(Json(
        json!({"period": period, "tasks_used": tasks, "executions": executions}),
    ))
}
