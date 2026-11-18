//! Extractors: who is signed in (`Authorization: Bearer …`, checked with Logto) and the member
//! acting in the org named by `X-Org-ID` (doc 13 §6). With sign-in turned off, requests act as an
//! owner of the named or default org.

use axum::{extract::FromRequestParts, http::request::Parts};
use weft_app::tenancy::Actor;
use weft_domain::model::User;

use crate::{error::ApiError, state::ApiState};

fn bearer(parts: &Parts) -> Option<&str> {
    let value = parts.headers.get("authorization")?.to_str().ok()?;
    let (scheme, token) = value.trim().split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then_some(token.trim())
}

/// The signed-in user (`None` when sign-in is turned off). Rejects a missing or invalid token.
#[derive(Debug, Clone)]
pub struct SignedIn(pub Option<User>);

impl FromRequestParts<ApiState> for SignedIn {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ApiState,
    ) -> Result<Self, Self::Rejection> {
        let user = weft_app::tenancy::signed_in(&state.deps, bearer(parts)).await?;
        Ok(Self(user))
    }
}

/// The signed-in member acting in the org named by `X-Org-ID`.
#[derive(Debug, Clone, Copy)]
pub struct Acting(pub Actor);

impl FromRequestParts<ApiState> for Acting {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ApiState,
    ) -> Result<Self, Self::Rejection> {
        let SignedIn(user) = SignedIn::from_request_parts(parts, state).await?;
        let header = parts.headers.get("x-org-id").and_then(|v| v.to_str().ok());
        let actor = weft_app::tenancy::actor(&state.deps, user.as_ref(), header).await?;
        Ok(Self(actor))
    }
}
