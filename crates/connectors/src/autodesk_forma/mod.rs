//! `autodesk_forma` connector (stub — implemented in increment 3).

use weft_connector_sdk::prelude::*;

/// The Autodesk Forma connector.
#[derive(Debug)]
pub struct FormaConnector {
    manifest: ConnectorManifest,
}

impl FormaConnector {
    #[must_use]
    pub fn new() -> Self {
        Self {
            manifest: ConnectorManifest {
                key: "autodesk_forma".into(),
                name: "Autodesk Forma".into(),
                description: "Coming soon".into(),
                icon: "🏗️".into(),
                category: "Construction".into(),
                auth: AuthSpec::none(),
                actions: Vec::new(),
                triggers: Vec::new(),
                docs_url: None,
            },
        }
    }
}

impl Default for FormaConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Connector for FormaConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    async fn execute(&self, _ctx: &ActionCtx, action: &str, _input: Value) -> Result<Value, ConnectorError> {
        Err(ConnectorError::unsupported(action))
    }
}
