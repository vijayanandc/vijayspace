//! `oauthapp`: an app that connects with OAuth 2.0 (PKCE), whose token endpoint is a test server.

use async_trait::async_trait;
use serde_json::{Value, json};
use weft_connector_sdk::{
    ActionCtx, AuthKind, AuthSpec, ClientAuth, Connector, ConnectorError, ConnectorManifest,
    FieldSpec, OAuth2Spec,
};

#[derive(Debug, Clone)]
pub struct OAuthTestConnector {
    manifest: ConnectorManifest,
}

impl OAuthTestConnector {
    /// `token_url`: where the authorization code is exchanged (a mock server in tests).
    #[must_use]
    pub fn new(token_url: &str) -> Self {
        Self {
            manifest: ConnectorManifest {
                key: "oauthapp".into(),
                name: "OAuth test app".into(),
                description: "Connects with OAuth for tests".into(),
                icon: "🔑".into(),
                category: "Test".into(),
                auth: AuthSpec {
                    kind: AuthKind::Oauth2,
                    // A non-secret setting, like a data-centre region.
                    fields: vec![FieldSpec::string("region", "Region")],
                    oauth2: Some(OAuth2Spec {
                        authorize_url: "https://auth.example/authorize".into(),
                        token_url: token_url.to_owned(),
                        scopes: vec!["read".into()],
                        scope_separator: " ".into(),
                        extra_authorize_params: Vec::new(),
                        pkce: true,
                        client_auth: ClientAuth::Body,
                    }),
                },
                actions: Vec::new(),
                triggers: Vec::new(),
                docs_url: None,
            },
        }
    }
}

#[async_trait]
impl Connector for OAuthTestConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    async fn execute(
        &self,
        _ctx: &ActionCtx,
        _action: &str,
        _input: Value,
    ) -> Result<Value, ConnectorError> {
        Ok(json!({}))
    }
}
