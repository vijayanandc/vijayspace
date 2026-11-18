//! Roles and permissions (doc 08 §2): a pure, exhaustively tested function. Roles live in Weft's
//! `org_members`, never in tokens.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

/// A member's role in one org.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Admin,
    Member,
    Viewer,
}

impl Role {
    pub const ALL: [Self; 4] = [Self::Owner, Self::Admin, Self::Member, Self::Viewer];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Member => "member",
            Self::Viewer => "viewer",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Role {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|r| r.as_str() == s)
            .ok_or_else(|| format!("unknown role `{s}`"))
    }
}

/// What a request wants to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    /// Rename the org, change its settings (retention), delete it.
    OrgManage,
    OrgBilling,
    /// Invite, change roles, remove members.
    MembersManage,
    /// See flows, the org, its members, usage and the app catalog.
    FlowView,
    /// Create, edit, test, publish, enable and disable flows; install recipes.
    FlowEdit,
    FlowDelete,
    /// See runs. Viewers see them with payloads redacted.
    ExecutionView,
    /// Retry, re-run and cancel runs.
    ExecutionRerun,
    /// Create, edit, test and sign in again to connections.
    ConnectionCreate,
    /// Use a connection in a step, a picker or a test.
    ConnectionUse,
    ConnectionDelete,
    AuditView,
    ApiKeysManage,
}

impl Permission {
    pub const ALL: [Self; 13] = [
        Self::OrgManage,
        Self::OrgBilling,
        Self::MembersManage,
        Self::FlowView,
        Self::FlowEdit,
        Self::FlowDelete,
        Self::ExecutionView,
        Self::ExecutionRerun,
        Self::ConnectionCreate,
        Self::ConnectionUse,
        Self::ConnectionDelete,
        Self::AuditView,
        Self::ApiKeysManage,
    ];
}

/// The answer for one role and permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Allowed,
    /// Allowed only on things the member created.
    OwnOnly,
    Denied,
}

/// The permission matrix of doc 08 §2. No wildcard arms: adding a role or a permission fails to
/// compile until every cell is decided.
#[must_use]
#[allow(clippy::match_same_arms)] // one arm per role and outcome, mirroring the table in doc 08
pub const fn access(role: Role, permission: Permission) -> Access {
    use Access::{Allowed, Denied, OwnOnly};
    use Permission as P;
    match (role, permission) {
        (
            Role::Owner,
            P::OrgManage
            | P::OrgBilling
            | P::MembersManage
            | P::FlowView
            | P::FlowEdit
            | P::FlowDelete
            | P::ExecutionView
            | P::ExecutionRerun
            | P::ConnectionCreate
            | P::ConnectionUse
            | P::ConnectionDelete
            | P::AuditView
            | P::ApiKeysManage,
        ) => Allowed,
        (Role::Admin, P::OrgManage | P::OrgBilling) => Denied,
        (
            Role::Admin,
            P::MembersManage
            | P::FlowView
            | P::FlowEdit
            | P::FlowDelete
            | P::ExecutionView
            | P::ExecutionRerun
            | P::ConnectionCreate
            | P::ConnectionUse
            | P::ConnectionDelete
            | P::AuditView
            | P::ApiKeysManage,
        ) => Allowed,
        (
            Role::Member,
            P::FlowView
            | P::FlowEdit
            | P::ExecutionView
            | P::ExecutionRerun
            | P::ConnectionCreate
            | P::ConnectionUse,
        ) => Allowed,
        (Role::Member, P::FlowDelete | P::ConnectionDelete) => OwnOnly,
        (
            Role::Member,
            P::OrgManage | P::OrgBilling | P::MembersManage | P::AuditView | P::ApiKeysManage,
        ) => Denied,
        (Role::Viewer, P::FlowView | P::ExecutionView) => Allowed,
        (
            Role::Viewer,
            P::OrgManage
            | P::OrgBilling
            | P::MembersManage
            | P::FlowEdit
            | P::FlowDelete
            | P::ExecutionRerun
            | P::ConnectionCreate
            | P::ConnectionUse
            | P::ConnectionDelete
            | P::AuditView
            | P::ApiKeysManage,
        ) => Denied,
    }
}

/// Whether `actor` may give someone `to` (inviting: `from` is `None`; changing a role: `from` is
/// their current role). Owners may do anything; admins manage admins, members and viewers but
/// never make or touch owners; nobody else manages members.
#[must_use]
pub const fn can_assign(actor: Role, from: Option<Role>, to: Role) -> bool {
    match actor {
        Role::Owner => true,
        Role::Admin => !matches!(to, Role::Owner) && !matches!(from, Some(Role::Owner)),
        Role::Member | Role::Viewer => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every cell of doc 08 §2, row by row: owner, admin, member, viewer.
    #[test]
    fn every_cell_of_the_matrix() {
        use Access::{Allowed as Y, Denied as N, OwnOnly as O};
        let rows: [(Permission, [Access; 4]); 13] = [
            (Permission::OrgManage, [Y, N, N, N]),
            (Permission::OrgBilling, [Y, N, N, N]),
            (Permission::MembersManage, [Y, Y, N, N]),
            (Permission::FlowView, [Y, Y, Y, Y]),
            (Permission::FlowEdit, [Y, Y, Y, N]),
            (Permission::FlowDelete, [Y, Y, O, N]),
            (Permission::ExecutionView, [Y, Y, Y, Y]),
            (Permission::ExecutionRerun, [Y, Y, Y, N]),
            (Permission::ConnectionCreate, [Y, Y, Y, N]),
            (Permission::ConnectionUse, [Y, Y, Y, N]),
            (Permission::ConnectionDelete, [Y, Y, O, N]),
            (Permission::AuditView, [Y, Y, N, N]),
            (Permission::ApiKeysManage, [Y, Y, N, N]),
        ];
        assert_eq!(rows.len(), Permission::ALL.len());
        for (permission, expected) in rows {
            for (role, want) in Role::ALL.into_iter().zip(expected) {
                assert_eq!(access(role, permission), want, "{role} / {permission:?}");
            }
        }
    }

    #[test]
    fn only_owners_make_or_touch_owners() {
        for to in Role::ALL {
            assert!(can_assign(Role::Owner, Some(Role::Owner), to));
        }
        assert!(can_assign(Role::Admin, None, Role::Admin));
        assert!(can_assign(Role::Admin, Some(Role::Viewer), Role::Member));
        assert!(!can_assign(Role::Admin, None, Role::Owner));
        assert!(!can_assign(Role::Admin, Some(Role::Owner), Role::Admin));
        for from in [None, Some(Role::Viewer)] {
            assert!(!can_assign(Role::Member, from, Role::Viewer));
            assert!(!can_assign(Role::Viewer, from, Role::Viewer));
        }
    }

    #[test]
    fn roles_round_trip_as_text() {
        for role in Role::ALL {
            assert_eq!(role.as_str().parse::<Role>(), Ok(role));
        }
        assert!("root".parse::<Role>().is_err());
    }
}
