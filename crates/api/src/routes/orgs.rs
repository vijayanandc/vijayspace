//! Sign-in configuration, the signed-in user, their orgs and the current org's settings.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use weft_app::tenancy::{invitations, orgs};
use weft_domain::{ids::InvitationId, model::Membership};

use crate::{
    error::{ApiError, ApiResult},
    extract::{Acting, SignedIn},
    state::ApiState,
};

pub fn router() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/auth/config", get(auth_config))
        .route("/api/v1/me", get(me))
        .route("/api/v1/me/invitations/{id}/accept", post(accept))
        .route("/api/v1/me/invitations/{id}/decline", post(decline))
        .route("/api/v1/orgs", get(list_orgs).post(create_org))
        .route("/api/v1/org", get(org).patch(update_org))
}

/// An org as the client sees it: settings, retention and the caller's role.
fn membership_json(deps: &weft_app::Deps, m: &Membership) -> Value {
    let mut v = json!(m.org);
    v["role"] = json!(m.role);
    v["retention_days_effective"] = json!(weft_app::retention::effective_days(deps, &m.org));
    v["retention_days_default"] = json!(deps.config.default_retention_days);
    v
}

pub(crate) fn invitation_id(raw: &str) -> Result<InvitationId, ApiError> {
    raw.parse().map_err(|_| {
        weft_app::AppError::not_found(
            weft_domain::error_codes::INVITATION_NOT_FOUND,
            "Invitation not found",
        )
        .into()
    })
}

/// Public: how the web client signs in (`sign_in: null` = sign-in is turned off).
async fn auth_config(State(s): State<ApiState>) -> Json<Value> {
    Json(json!({"sign_in": s.deps.config.sign_in}))
}

async fn me(State(s): State<ApiState>, SignedIn(user): SignedIn) -> ApiResult<Json<Value>> {
    let view = orgs::me(&s.deps, user.as_ref()).await?;
    let memberships: Vec<Value> = view
        .orgs
        .iter()
        .map(|m| membership_json(&s.deps, m))
        .collect();
    Ok(Json(json!({
        "user": view.user,
        "orgs": memberships,
        "invitations": view.invitations,
    })))
}

async fn list_orgs(State(s): State<ApiState>, SignedIn(user): SignedIn) -> ApiResult<Json<Value>> {
    let view = orgs::me(&s.deps, user.as_ref()).await?;
    let memberships: Vec<Value> = view
        .orgs
        .iter()
        .map(|m| membership_json(&s.deps, m))
        .collect();
    Ok(Json(json!(memberships)))
}

#[derive(Debug, Deserialize)]
struct CreateOrg {
    name: String,
    #[serde(default)]
    timezone: Option<String>,
}

async fn create_org(
    State(s): State<ApiState>,
    SignedIn(user): SignedIn,
    Json(b): Json<CreateOrg>,
) -> ApiResult<(axum::http::StatusCode, Json<Value>)> {
    let m = orgs::create_org(&s.deps, user.as_ref(), &b.name, b.timezone.as_deref()).await?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(membership_json(&s.deps, &m)),
    ))
}

async fn accept(
    State(s): State<ApiState>,
    SignedIn(user): SignedIn,
    Path(raw): Path<String>,
) -> ApiResult<Json<Value>> {
    let m = invitations::accept(&s.deps, user.as_ref(), invitation_id(&raw)?).await?;
    Ok(Json(membership_json(&s.deps, &m)))
}

async fn decline(
    State(s): State<ApiState>,
    SignedIn(user): SignedIn,
    Path(raw): Path<String>,
) -> ApiResult<axum::http::StatusCode> {
    invitations::decline(&s.deps, user.as_ref(), invitation_id(&raw)?).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn org(State(s): State<ApiState>, Acting(actor): Acting) -> ApiResult<Json<Value>> {
    let m = orgs::org_view(&s.deps, &actor).await?;
    Ok(Json(membership_json(&s.deps, &m)))
}

#[derive(Debug, Deserialize)]
struct OrgBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    timezone: Option<String>,
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
    Acting(actor): Acting,
    Json(b): Json<OrgBody>,
) -> ApiResult<Json<Value>> {
    if let Some(days) = b.retention_days {
        weft_app::retention::set_retention_days(&s.deps, &actor, days).await?;
    }
    let m = orgs::update_org(&s.deps, &actor, b.name.as_deref(), b.timezone.as_deref()).await?;
    Ok(Json(membership_json(&s.deps, &m)))
}
