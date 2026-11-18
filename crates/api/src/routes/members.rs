//! The current org's members and invitations.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use weft_app::tenancy::{invitations, members};
use weft_domain::{error_codes, ids::UserId, permissions::Role};

use super::orgs::invitation_id;
use crate::{
    error::{ApiError, ApiResult},
    extract::Acting,
    state::ApiState,
};

pub fn router() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/members", get(list))
        .route(
            "/api/v1/members/{user_id}",
            axum::routing::patch(set_role).delete(remove),
        )
        .route("/api/v1/invitations", get(list_invitations).post(invite))
        .route("/api/v1/invitations/{id}", delete(revoke))
        .route("/api/v1/invitations/{id}/resend", post(resend))
}

fn user_id(raw: &str) -> Result<UserId, ApiError> {
    raw.parse().map_err(|_| {
        weft_app::AppError::not_found(error_codes::MEMBER_NOT_FOUND, "Not a member").into()
    })
}

async fn list(State(s): State<ApiState>, Acting(actor): Acting) -> ApiResult<Json<Value>> {
    Ok(Json(json!(members::list_members(&s.deps, &actor).await?)))
}

#[derive(Debug, Deserialize)]
struct RoleBody {
    role: Role,
}

async fn set_role(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
    Json(b): Json<RoleBody>,
) -> ApiResult<Json<Value>> {
    let m = members::set_role(&s.deps, &actor, user_id(&raw)?, b.role).await?;
    Ok(Json(json!(m)))
}

async fn remove(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
) -> ApiResult<StatusCode> {
    members::remove_member(&s.deps, &actor, user_id(&raw)?).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_invitations(
    State(s): State<ApiState>,
    Acting(actor): Acting,
) -> ApiResult<Json<Value>> {
    Ok(Json(json!(invitations::list(&s.deps, &actor).await?)))
}

#[derive(Debug, Deserialize)]
struct InviteBody {
    email: String,
    role: Role,
}

async fn invite(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Json(b): Json<InviteBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let invited = invitations::invite(&s.deps, &actor, &b.email, b.role).await?;
    Ok((StatusCode::CREATED, Json(json!(invited))))
}

async fn resend(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
) -> ApiResult<Json<Value>> {
    let invited = invitations::resend(&s.deps, &actor, invitation_id(&raw)?).await?;
    Ok(Json(json!(invited)))
}

async fn revoke(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
) -> ApiResult<StatusCode> {
    invitations::revoke(&s.deps, &actor, invitation_id(&raw)?).await?;
    Ok(StatusCode::NO_CONTENT)
}
