//! `appblocks` connector (stub — implemented in increment 3).

use weft_connector_sdk::prelude::*;

/// The Appblocks (FSM, HMS, …) connector.
#[derive(Debug)]
pub struct AppblocksConnector {
    manifest: ConnectorManifest,
}

impl AppblocksConnector {
    #[must_use]
    pub fn new() -> Self {
        Self {
            manifest: ConnectorManifest {
                key: "appblocks".into(),
                name: "Appblocks (FSM, HMS, …)".into(),
                description: "Coming soon".into(),
                icon: "🧩".into(),
                category: "Business apps".into(),
                auth: AuthSpec::none(),
                actions: Vec::new(),
                triggers: Vec::new(),
                docs_url: None,
            },
        }
    }
}

impl Default for AppblocksConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Connector for AppblocksConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    async fn execute(&self, _ctx: &ActionCtx, action: &str, _input: Value) -> Result<Value, ConnectorError> {
        Err(ConnectorError::unsupported(action))
    }
}
