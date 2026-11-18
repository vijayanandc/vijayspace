//! The operator's staff: owners and admins of the default org. Only staff see platform operations (servers,
//! queue lag) and, unless `WEFT_ORG_CREATION=anyone`, only staff create organisations (each customer's org is
//! set up by staff; customers join by invitation).

use weft_domain::{model::Membership, permissions::Role};

use super::Actor;
use crate::{AppError, Deps};

/// Who may create organisations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OrgCreation {
    /// Staff only (the default): the Logto tenant is shared with other apps, so signing in alone is not enough.
    #[default]
    Staff,
    /// Anyone signed in.
    Anyone,
}

/// Owners and admins count as staff.
#[must_use]
pub const fn staff_role(role: Role) -> bool {
    matches!(role, Role::Owner | Role::Admin)
}

/// Whether these memberships make their user staff.
#[must_use]
pub fn is_staff(deps: &Deps, memberships: &[Membership]) -> bool {
    memberships
        .iter()
        .any(|m| m.org.id == deps.config.default_org_id && staff_role(m.role))
}

/// Whether a user with these memberships may create an organisation.
#[must_use]
pub fn may_create_orgs(deps: &Deps, memberships: &[Membership]) -> bool {
    match deps.config.org_creation {
        OrgCreation::Anyone => true,
        OrgCreation::Staff => is_staff(deps, memberships),
    }
}

/// Fails unless the actor is staff, acting in the default org.
pub fn require_staff(deps: &Deps, actor: &Actor) -> Result<(), AppError> {
    if actor.org == deps.config.default_org_id && staff_role(actor.role) {
        Ok(())
    } else {
        Err(AppError::forbidden("Only the operator's staff can do this"))
    }
}
