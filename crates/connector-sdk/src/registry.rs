//! The set of available connectors; also the catalog used by publish validation.

use std::{collections::BTreeMap, sync::Arc};

use serde_json::Value;
use weft_domain::flow::{ActionInfo, Catalog};

use crate::{connector::Connector, manifest::ConnectorManifest};

/// All registered connectors, keyed by manifest key.
#[derive(Clone, Default)]
pub struct ConnectorRegistry {
    connectors: BTreeMap<String, Arc<dyn Connector>>,
}

impl ConnectorRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a connector (a later registration with the same key replaces the earlier one).
    pub fn register(&mut self, connector: Arc<dyn Connector>) {
        self.connectors
            .insert(connector.manifest().key.clone(), connector);
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Arc<dyn Connector>> {
        self.connectors.get(key)
    }

    pub fn manifests(&self) -> impl Iterator<Item = &ConnectorManifest> {
        self.connectors.values().map(|c| c.manifest())
    }
}

impl std::fmt::Debug for ConnectorRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.connectors.keys()).finish()
    }
}

impl Catalog for ConnectorRegistry {
    fn action(&self, connector: &str, action: &str) -> Option<ActionInfo> {
        let manifest = self.get(connector)?.manifest();
        let spec = manifest.action(action)?;
        Some(ActionInfo {
            requires_connection: manifest.requires_connection(),
            required_fields: spec.required_fields(),
        })
    }

    fn check_input(&self, connector: &str, action: &str, input: &Value) -> Vec<(String, String)> {
        self.get(connector)
            .map(|c| c.check_input(action, input))
            .unwrap_or_default()
    }

    fn trigger(&self, connector: &str, trigger: &str) -> Option<ActionInfo> {
        let manifest = self.get(connector)?.manifest();
        let spec = manifest.trigger(trigger)?;
        Some(ActionInfo {
            requires_connection: manifest.requires_connection(),
            required_fields: spec
                .fields
                .iter()
                .filter(|f| f.required)
                .map(|f| f.key.clone())
                .collect(),
        })
    }
}
