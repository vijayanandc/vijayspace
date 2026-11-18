//! `zoho_crm` OAuth callback: the accounts server (which receives the client secret) is allowlisted, or is the
//! operator's override (`WEFT_ZOHO_ACCOUNTS_URL`, testing against a mock) and nothing else.
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

#[test]
fn an_operator_accounts_override_is_the_only_accounts_server_used() {
    let zoho = ZohoCrmConnector::with_accounts_server("http://127.0.0.1:9200/");
    let spec = zoho.manifest().auth.oauth2.as_ref().unwrap();
    assert_eq!(spec.authorize_url, "http://127.0.0.1:9200/oauth/v2/auth");
    for server in ["https://accounts.zoho.eu", "https://attacker.example"] {
        let query = BTreeMap::from([("accounts-server".to_owned(), server.to_owned())]);
        assert_eq!(
            zoho.oauth_callback_config(&query)["accounts_server"],
            "http://127.0.0.1:9200"
        );
    }
    assert_eq!(
        ZohoCrmConnector::new()
            .manifest()
            .auth
            .oauth2
            .as_ref()
            .unwrap()
            .authorize_url,
        "https://accounts.zoho.com/oauth/v2/auth"
    );
}
