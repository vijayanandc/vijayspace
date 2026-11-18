//! `Actor`: who is making a request, in which org, with which role. Every API use case starts with
//! `actor.require(…)` (doc 08 §2).

use weft_domain::{
    ids::{OrgId, UserId},
    permissions::{Access, Permission, Role, access},
};

use crate::AppError;

/// The signed-in member acting in one org.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actor {
    pub org: OrgId,
    /// `None` only when sign-in is turned off (local development and tests).
    pub user: Option<UserId>,
    pub role: Role,
}

impl Actor {
    /// The actor when sign-in is turned off: an owner of the requested org.
    #[must_use]
    pub const fn local(org: OrgId) -> Self {
        Self {
            org,
            user: None,
            role: Role::Owner,
        }
    }

    /// Fails unless the role allows `permission` outright.
    pub fn require(&self, permission: Permission) -> Result<(), AppError> {
        match access(self.role, permission) {
            Access::Allowed => Ok(()),
            Access::OwnOnly | Access::Denied => Err(self.denied(permission)),
        }
    }

    /// Fails unless the role allows `permission` on something created by `created_by`.
    pub fn require_on(
        &self,
        permission: Permission,
        created_by: Option<UserId>,
    ) -> Result<(), AppError> {
        match access(self.role, permission) {
            Access::Allowed => Ok(()),
            Access::OwnOnly if created_by.is_some() && created_by == self.user => Ok(()),
            Access::OwnOnly => Err(AppError::forbidden(format!(
                "As a {}, you can only do this to things you created",
                self.role
            ))),
            Access::Denied => Err(self.denied(permission)),
        }
    }

    /// Viewers see runs without their data (doc 08 §2).
    #[must_use]
    pub const fn sees_payloads(&self) -> bool {
        !matches!(self.role, Role::Viewer)
    }

    fn denied(&self, permission: Permission) -> AppError {
        AppError::forbidden(format!(
            "Your role in this organisation ({}) does not allow this ({permission:?})",
            self.role
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(role: Role, user: u128) -> Actor {
        Actor {
            org: OrgId::from_uuid(uuid::Uuid::from_u128(1)),
            user: Some(UserId::from_uuid(uuid::Uuid::from_u128(user))),
            role,
        }
    }

    #[test]
    fn own_only_permissions_need_the_creator() {
        let member = actor(Role::Member, 7);
        let mine = member.user;
        let theirs = Some(UserId::from_uuid(uuid::Uuid::from_u128(8)));
        assert!(member.require_on(Permission::FlowDelete, mine).is_ok());
        assert!(member.require_on(Permission::FlowDelete, theirs).is_err());
        assert!(
            member.require_on(Permission::FlowDelete, None).is_err(),
            "unknown creator"
        );
        assert!(member.require(Permission::FlowDelete).is_err());
        assert!(
            actor(Role::Admin, 9)
                .require_on(Permission::FlowDelete, theirs)
                .is_ok()
        );
        assert!(
            actor(Role::Viewer, 7)
                .require_on(Permission::FlowDelete, mine)
                .is_err()
        );
    }

    #[test]
    fn errors_are_forbidden_and_viewers_do_not_see_payloads() {
        let e = actor(Role::Viewer, 1)
            .require(Permission::FlowEdit)
            .unwrap_err();
        assert_eq!(e.code, weft_domain::error_codes::AUTH_FORBIDDEN);
        assert!(!actor(Role::Viewer, 1).sees_payloads());
        assert!(actor(Role::Member, 1).sees_payloads());
        assert!(
            Actor::local(OrgId::from_uuid(uuid::Uuid::from_u128(1)))
                .require(Permission::OrgManage)
                .is_ok()
        );
    }
}
