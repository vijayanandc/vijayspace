//! `zoho_crm` connector (stub — implemented in increment 3).

use weft_connector_sdk::prelude::*;

/// The Zoho CRM connector.
#[derive(Debug)]
pub struct ZohoCrmConnector {
    manifest: ConnectorManifest,
}

impl ZohoCrmConnector {
    #[must_use]
    pub fn new() -> Self {
        Self {
            manifest: ConnectorManifest {
                key: "zoho_crm".into(),
                name: "Zoho CRM".into(),
                description: "Coming soon".into(),
                icon: "🟥".into(),
                category: "CRM".into(),
                auth: AuthSpec::none(),
                actions: Vec::new(),
                triggers: Vec::new(),
                docs_url: None,
            },
        }
    }
}

impl Default for ZohoCrmConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Connector for ZohoCrmConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    async fn execute(&self, _ctx: &ActionCtx, action: &str, _input: Value) -> Result<Value, ConnectorError> {
        Err(ConnectorError::unsupported(action))
    }
}
