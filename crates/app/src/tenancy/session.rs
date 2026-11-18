//! Use cases: who is signed in (a user created on first sign-in), and the actor of a request.

use weft_domain::{error_codes, ids::OrgId, model::User};

use super::Actor;
use crate::{AppError, Deps, ports::VerifiedIdentity};

/// Normalises an email for matching: trimmed and lower-cased; empty is `None`.
#[must_use]
pub fn normalize_email(email: &str) -> Option<String> {
    let email = email.trim().to_lowercase();
    (!email.is_empty()).then_some(email)
}

/// The signed-in user, or `None` when sign-in is turned off. A missing or invalid token is
/// `auth.unauthenticated`.
pub async fn signed_in(deps: &Deps, bearer: Option<&str>) -> Result<Option<User>, AppError> {
    let Some(verifier) = &deps.identity else {
        return Ok(None);
    };
    let token = bearer
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| AppError::unauthenticated("Sign in to continue"))?;
    let identity = verifier
        .verify(token)
        .await?
        .ok_or_else(|| AppError::unauthenticated("Your sign-in has expired. Sign in again."))?;
    user_for(deps, &identity).await.map(Some)
}

/// The user for a verified identity: created on first sign-in, refreshed when the email or name
/// changed (the common case is one read).
async fn user_for(deps: &Deps, identity: &VerifiedIdentity) -> Result<User, AppError> {
    let mut identity = VerifiedIdentity {
        email: identity.email.as_deref().and_then(normalize_email),
        ..identity.clone()
    };
    let stored = deps.store.user_by_subject(&identity.subject).await?;
    // A token without profile claims (a JWT without custom claims) keeps what is known; userinfo
    // is authoritative (an email removed at Logto is removed here too).
    if let Some(user) = stored.as_ref().filter(|_| !identity.has_profile) {
        if identity.email.is_none() {
            identity.email.clone_from(&user.email);
            identity.email_verified = user.email_verified;
        }
        if identity.name.is_none() {
            identity.name.clone_from(&user.name);
        }
    }
    if let Some(user) = stored
        && user.email == identity.email
        && user.email_verified == identity.email_verified
        && user.name == identity.name
    {
        return Ok(user);
    }
    let now = deps.clock.now();
    let user = deps
        .store
        .upsert_user(deps.new_user_id(), &identity, now)
        .await?;
    bootstrap_owner(deps, &user).await?;
    Ok(user)
}

/// A user whose verified email is in `WEFT_OWNER_EMAILS` claims the default org while it has no
/// owner (so existing flows get an owner once sign-in is turned on). Once it has one, owners
/// manage its members like any other org's: removing a listed owner sticks.
pub(crate) async fn bootstrap_owner(deps: &Deps, user: &User) -> Result<(), AppError> {
    let Some(email) = user.email.as_deref().filter(|_| user.email_verified) else {
        return Ok(());
    };
    if !deps.config.owner_emails.iter().any(|e| e == email) {
        return Ok(());
    }
    let now = deps.clock.now();
    let org = deps
        .store
        .ensure_default_org(deps.config.default_org_id, now)
        .await?;
    deps.store
        .claim_ownerless_org(org.id, user.id, now)
        .await
        .map(|_| ())
}

/// The actor of a tenant-scoped request: the member acting in the org named by `X-Org-ID`. An org
/// the user does not belong to is "not found", never "forbidden" (doc 08 §3). With sign-in off,
/// the request acts as an owner of the named (or default) org.
pub async fn actor(
    deps: &Deps,
    user: Option<&User>,
    org_header: Option<&str>,
) -> Result<Actor, AppError> {
    let Some(user) = user else {
        let org = crate::org::resolve_org(deps, org_header).await?;
        return Ok(Actor::local(org.id));
    };
    let raw = org_header
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .ok_or_else(|| {
            AppError::validation(
                error_codes::VALIDATION_FAILED,
                "Choose an organisation (the X-Org-ID header is missing)",
            )
        })?;
    let not_found = || AppError::not_found(error_codes::ORG_NOT_FOUND, "Organisation not found");
    let org: OrgId = raw.parse().map_err(|_| not_found())?;
    let role = deps
        .store
        .member_role(org, user.id)
        .await?
        .ok_or_else(not_found)?;
    Ok(Actor {
        org,
        user: Some(user.id),
        role,
    })
}
