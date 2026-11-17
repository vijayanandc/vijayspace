//! `zoho_crm` OAuth callback: the accounts server (which receives the client secret) is allowlisted.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use weft_connector_sdk::Connector as _;
use weft_connectors::zoho_crm::ZohoCrmConnector;

#[test]
fn oauth_callback_only_accepts_zoho_accounts_servers() {
    let zoho = ZohoCrmConnector::new();
    let config = |server: &str| {
        let query = BTreeMap::from([("accounts-server".to_owned(), server.to_owned())]);
        zoho.oauth_callback_config(&query)["accounts_server"].clone()
    };
    // The token exchange sends the client secret to this host: an attacker's host must never be used.
    assert_eq!(
        config("https://attacker.example"),
        "https://accounts.zoho.com"
    );
    assert_eq!(
        config("https://accounts.zoho.com.attacker.example"),
        "https://accounts.zoho.com"
    );
    assert_eq!(
        config("http://accounts.zoho.eu"),
        "https://accounts.zoho.com"
    );
    assert_eq!(
        config("https://accounts.zoho.eu/"),
        "https://accounts.zoho.eu"
    );
    assert_eq!(
        config("https://accounts.zoho.in"),
        "https://accounts.zoho.in"
    );
    assert_eq!(
        zoho.oauth_callback_config(&BTreeMap::new())["accounts_server"],
        "https://accounts.zoho.com"
    );
}
