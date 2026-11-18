//! Use cases: the signed-in user's orgs and invitations, creating an org, and the org settings.

use serde::Serialize;
use weft_domain::{
    error_codes,
    model::{Invitation, Membership, Org, User},
    permissions::{Permission, Role},
};

use super::Actor;
use crate::{AppError, Deps};

/// Most orgs one user may belong to (creating more is refused).
const MAX_ORGS_PER_USER: usize = 50;

/// `GET /me`: who is signed in, their orgs and the invitations waiting for them.
#[derive(Debug, Clone, Serialize)]
pub struct MeView {
    /// `None` when sign-in is turned off.
    pub user: Option<User>,
    pub orgs: Vec<Membership>,
    pub invitations: Vec<Invitation>,
    /// Owner or admin of the default org (the operator's staff).
    pub staff: bool,
    /// Whether this user may create an organisation (staff only unless `WEFT_ORG_CREATION=anyone`).
    pub can_create_orgs: bool,
}

pub async fn me(deps: &Deps, user: Option<&User>) -> Result<MeView, AppError> {
    let now = deps.clock.now();
    let Some(user) = user else {
        let org = deps
            .store
            .ensure_default_org(deps.config.default_org_id, now)
            .await?;
        return Ok(MeView {
            user: None,
            orgs: vec![Membership {
                org,
                role: Role::Owner,
            }],
            invitations: Vec::new(),
            staff: true,
            can_create_orgs: false,
        });
    };
    // Picks up an owner email added to the configuration after the user first signed in.
    super::session::bootstrap_owner(deps, user).await?;
    let invitations = match user.email.as_deref().filter(|_| user.email_verified) {
        Some(email) => deps
            .store
            .invitations_for_email(email, now)
            .await?
            .into_iter()
            .filter(|i| i.email == email)
            .collect(),
        None => Vec::new(),
    };
    let orgs = deps.store.memberships(user.id).await?;
    Ok(MeView {
        user: Some(user.clone()),
        staff: super::staff::is_staff(deps, &orgs),
        can_create_orgs: super::staff::may_create_orgs(deps, &orgs),
        orgs,
        invitations,
    })
}

fn checked_name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return Err(AppError::validation(
            error_codes::VALIDATION_FAILED,
            "Name the organisation (1 to 120 characters)",
        ));
    }
    Ok(name.to_owned())
}

fn checked_timezone(tz: &str) -> Result<String, AppError> {
    let tz = tz.trim();
    tz.parse::<chrono_tz::Tz>().map_err(|_| {
        AppError::validation(
            error_codes::VALIDATION_FAILED,
            format!("Unknown time zone `{tz}`"),
        )
    })?;
    Ok(tz.to_owned())
}

/// Creates an org with the signed-in user as its owner.
pub async fn create_org(
    deps: &Deps,
    user: Option<&User>,
    name: &str,
    timezone: Option<&str>,
) -> Result<Membership, AppError> {
    let Some(user) = user else {
        return Err(AppError::validation(
            error_codes::VALIDATION_FAILED,
            "Sign-in is turned off: there is only the default organisation",
        ));
    };
    let name = checked_name(name)?;
    let timezone = checked_timezone(timezone.unwrap_or("UTC"))?;
    let memberships = deps.store.memberships(user.id).await?;
    if !super::staff::may_create_orgs(deps, &memberships) {
        return Err(AppError::forbidden(
            "Organisations are set up by the operator's staff: ask your administrator for an invitation",
        ));
    }
    if memberships.len() >= MAX_ORGS_PER_USER {
        return Err(AppError::validation(
            error_codes::VALIDATION_FAILED,
            format!("You can belong to at most {MAX_ORGS_PER_USER} organisations"),
        ));
    }
    let now = deps.clock.now();
    let org = Org {
        id: deps.new_org_id(),
        name,
        timezone,
        retention_days: None,
        created_at: now,
    };
    deps.store.create_org_with_owner(&org, user.id, now).await?;
    Ok(Membership {
        org,
        role: Role::Owner,
    })
}

/// The org the actor is acting in, with their role.
pub async fn org_view(deps: &Deps, actor: &Actor) -> Result<Membership, AppError> {
    actor.require(Permission::FlowView)?;
    let org =
        deps.store.get_org(actor.org).await?.ok_or_else(|| {
            AppError::not_found(error_codes::ORG_NOT_FOUND, "Organisation not found")
        })?;
    Ok(Membership {
        org,
        role: actor.role,
    })
}

/// Renames the org or changes its time zone (owners only).
pub async fn update_org(
    deps: &Deps,
    actor: &Actor,
    name: Option<&str>,
    timezone: Option<&str>,
) -> Result<Membership, AppError> {
    actor.require(Permission::OrgManage)?;
    if name.is_some() || timezone.is_some() {
        let current = org_view(deps, actor).await?.org;
        let name = name.map_or(Ok(current.name), checked_name)?;
        let timezone = timezone.map_or(Ok(current.timezone), checked_timezone)?;
        deps.store
            .update_org_profile(actor.org, &name, &timezone, deps.clock.now())
            .await?;
    }
    org_view(deps, actor).await
}
