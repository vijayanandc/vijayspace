//! Sign-in and tenancy (doc 13 §6): the request's actor, users created on first sign-in, orgs,
//! members and invitations.

mod actor;
pub mod invitations;
pub mod members;
pub mod orgs;
pub mod session;

pub use actor::Actor;
pub use session::{actor, normalize_email, signed_in};
