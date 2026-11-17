//! Extractors: the request's org (`X-Org-ID` or the default org).

use axum::{extract::FromRequestParts, http::request::Parts};
use weft_domain::ids::OrgId;

use crate::{error::ApiError, state::ApiState};

/// The tenant of the request. Authentication (Logto) will populate this later (doc 13 §6).
#[derive(Debug, Clone, Copy)]
pub struct Org(pub OrgId);

impl FromRequestParts<ApiState> for Org {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ApiState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts.headers.get("x-org-id").and_then(|v| v.to_str().ok());
        let org = weft_app::org::resolve_org(&state.deps, header).await?;
        Ok(Self(org.id))
    }
}
