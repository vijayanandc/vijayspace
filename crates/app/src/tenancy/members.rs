//! Use cases: an org's members, changing roles, removing members and leaving.

use weft_domain::{
    error_codes,
    ids::UserId,
    model::Member,
    permissions::{Permission, Role, can_assign},
};

use super::Actor;
use crate::{AppError, Deps, ports::MemberChange};

pub async fn list_members(deps: &Deps, actor: &Actor) -> Result<Vec<Member>, AppError> {
    actor.require(Permission::FlowView)?;
    deps.store.list_members(actor.org).await
}

fn not_a_member() -> AppError {
    AppError::not_found(
        error_codes::MEMBER_NOT_FOUND,
        "Not a member of this organisation",
    )
}

async fn current_role(deps: &Deps, actor: &Actor, user: UserId) -> Result<Role, AppError> {
    deps.store
        .member_role(actor.org, user)
        .await?
        .ok_or_else(not_a_member)
}

fn outcome(change: MemberChange) -> Result<(), AppError> {
    match change {
        MemberChange::Done => Ok(()),
        MemberChange::NotFound => Err(not_a_member()),
        MemberChange::LastOwner => Err(AppError::conflict(
            error_codes::MEMBER_LAST_OWNER,
            "An organisation needs at least one owner. Make someone else an owner first.",
        )),
    }
}

/// Changes a member's role. Admins manage admins, members and viewers; only owners make or change
/// owners. The last owner cannot step down.
pub async fn set_role(
    deps: &Deps,
    actor: &Actor,
    user: UserId,
    role: Role,
) -> Result<Member, AppError> {
    actor.require(Permission::MembersManage)?;
    let from = current_role(deps, actor, user).await?;
    if !can_assign(actor.role, Some(from), role) {
        return Err(AppError::forbidden(
            "Only an owner can make or change owners",
        ));
    }
    if from != role {
        let change = deps
            .store
            .set_member_role(actor.org, user, from, role, deps.clock.now())
            .await?;
        outcome(change)?;
    }
    deps.store
        .list_members(actor.org)
        .await?
        .into_iter()
        .find(|m| m.user.id == user)
        .ok_or_else(not_a_member)
}

/// Removes a member. Anyone may leave; removing someone else needs `members.manage`, and only an
/// owner removes an owner. The last owner cannot leave.
pub async fn remove_member(deps: &Deps, actor: &Actor, user: UserId) -> Result<(), AppError> {
    let leaving = actor.user == Some(user);
    if !leaving {
        actor.require(Permission::MembersManage)?;
    }
    let from = current_role(deps, actor, user).await?;
    if !leaving && !can_assign(actor.role, Some(from), from) {
        return Err(AppError::forbidden("Only an owner can remove an owner"));
    }
    let change = deps.store.remove_member(actor.org, user, from).await?;
    outcome(change)
}
