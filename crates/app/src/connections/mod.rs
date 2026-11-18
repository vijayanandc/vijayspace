//! Connections: create, test, OAuth, dynamic options/fields, and the runtime connection handle.

pub mod handle;
mod manage;
mod oauth;

pub use manage::{
    ConnectionTest, create_connection, delete_connection, get_connection, list_connections,
    load_fields, load_options, test_connection, update_connection,
};
pub use oauth::{oauth_callback, oauth_finish, oauth_start};

use weft_domain::ids::{ConnectionId, OrgId};

/// AAD binding a ciphertext to its org and connection row.
#[must_use]
pub fn secret_aad(org: OrgId, id: ConnectionId) -> Vec<u8> {
    format!("{org}|{id}").into_bytes()
}
