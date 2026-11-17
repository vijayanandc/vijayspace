//! `memdb`: an in-memory record app (auth kind `fields`, `RecordObjects`) for sync tests.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use serde_json::{Map, Value, json};
use weft_connector_sdk::{
    ActionCtx, ActionSpec, AuthSpec, Connector, ConnectorError, ConnectorManifest, FieldKind,
    FieldSpec, ObjectInfo, Page, Record, RecordObjects, RecordRef,
};

type Tables = BTreeMap<String, BTreeMap<String, Map<String, Value>>>;

/// In-memory record store shared by all clones; records are keyed by object then ID.
#[derive(Debug, Clone)]
pub struct MemoryRecordsConnector {
    manifest: ConnectorManifest,
    pub tables: Arc<Mutex<Tables>>,
    pub writes: Arc<Mutex<Vec<(String, String, String)>>>,
}

impl MemoryRecordsConnector {
    #[must_use]
    pub fn new(key: &str) -> Self {
        let manifest = ConnectorManifest {
            key: key.to_owned(),
            name: format!("Memory app {key}"),
            description: "In-memory records for tests".into(),
            icon: "🧪".into(),
            category: "Test".into(),
            auth: AuthSpec::fields(vec![
                FieldSpec::new("token", "Token", FieldKind::Password).secret(),
            ]),
            actions: vec![
                ActionSpec::new("create_record", "Create record", "Create")
                    .field(FieldSpec::string("module", "Module").required())
                    .field(FieldSpec::new("values", "Values", FieldKind::Object)),
            ],
            triggers: Vec::new(),
            docs_url: None,
        };
        Self {
            manifest,
            tables: Arc::new(Mutex::new(BTreeMap::new())),
            writes: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Inserts a record directly (as if a user created it in the app).
    pub fn seed(&self, object: &str, id: &str, values: Value) {
        let map = values.as_object().cloned().unwrap_or_default();
        self.tables
            .lock()
            .unwrap()
            .entry(object.to_owned())
            .or_default()
            .insert(id.to_owned(), map);
    }

    #[must_use]
    pub fn get_record(&self, object: &str, id: &str) -> Option<Map<String, Value>> {
        self.tables
            .lock()
            .unwrap()
            .get(object)
            .and_then(|t| t.get(id))
            .cloned()
    }

    #[must_use]
    pub fn count(&self, object: &str) -> usize {
        self.tables
            .lock()
            .unwrap()
            .get(object)
            .map_or(0, BTreeMap::len)
    }

    fn next_id(&self, object: &str) -> String {
        format!("{object}-{}", self.count(object) + 1)
    }
}

#[async_trait]
impl Connector for MemoryRecordsConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    async fn execute(
        &self,
        ctx: &ActionCtx,
        action: &str,
        input: Value,
    ) -> Result<Value, ConnectorError> {
        match action {
            "create_record" => {
                let module = input
                    .get("module")
                    .and_then(Value::as_str)
                    .unwrap_or("default")
                    .to_owned();
                let values = input
                    .get("values")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                let r = self.create(ctx, &module, &values).await?;
                Ok(json!({"id": r.id}))
            }
            other => Err(ConnectorError::unsupported(other)),
        }
    }

    async fn test_connection(&self, _ctx: &ActionCtx) -> Result<Value, ConnectorError> {
        Ok(json!({"ok": true}))
    }

    fn records(&self) -> Option<&dyn RecordObjects> {
        Some(self)
    }
}

#[async_trait]
impl RecordObjects for MemoryRecordsConnector {
    async fn list_objects(&self, _ctx: &ActionCtx) -> Result<Vec<ObjectInfo>, ConnectorError> {
        Ok(vec![
            ObjectInfo {
                key: "accounts".into(),
                label: "Accounts".into(),
            },
            ObjectInfo {
                key: "contacts".into(),
                label: "Contacts".into(),
            },
        ])
    }

    async fn describe(
        &self,
        _ctx: &ActionCtx,
        _object: &str,
    ) -> Result<Vec<FieldSpec>, ConnectorError> {
        Ok(vec![
            FieldSpec::string("name", "Name"),
            FieldSpec::string("email", "Email"),
            FieldSpec::string("account_id", "Account"),
        ])
    }

    async fn get(
        &self,
        _ctx: &ActionCtx,
        object: &str,
        id: &str,
    ) -> Result<Option<Record>, ConnectorError> {
        Ok(self.get_record(object, id).map(|values| Record {
            id: id.to_owned(),
            modified_at: None,
            values,
        }))
    }

    async fn find_by(
        &self,
        _ctx: &ActionCtx,
        object: &str,
        field: &str,
        value: &Value,
    ) -> Result<Vec<Record>, ConnectorError> {
        let tables = self.tables.lock().unwrap();
        Ok(tables
            .get(object)
            .map(|t| {
                t.iter()
                    .filter(|(_, v)| v.get(field) == Some(value))
                    .map(|(id, v)| Record {
                        id: id.clone(),
                        modified_at: None,
                        values: v.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn create(
        &self,
        _ctx: &ActionCtx,
        object: &str,
        values: &Map<String, Value>,
    ) -> Result<RecordRef, ConnectorError> {
        let id = self.next_id(object);
        self.tables
            .lock()
            .unwrap()
            .entry(object.to_owned())
            .or_default()
            .insert(id.clone(), values.clone());
        self.writes
            .lock()
            .unwrap()
            .push(("create".into(), object.to_owned(), id.clone()));
        Ok(RecordRef { id })
    }

    async fn update(
        &self,
        _ctx: &ActionCtx,
        object: &str,
        id: &str,
        values: &Map<String, Value>,
    ) -> Result<RecordRef, ConnectorError> {
        let mut tables = self.tables.lock().unwrap();
        let record = tables
            .get_mut(object)
            .and_then(|t| t.get_mut(id))
            .ok_or_else(|| ConnectorError::NotFound {
                message: id.to_owned(),
            })?;
        record.extend(values.clone());
        self.writes
            .lock()
            .unwrap()
            .push(("update".into(), object.to_owned(), id.to_owned()));
        Ok(RecordRef { id: id.to_owned() })
    }

    async fn changed_since(
        &self,
        _ctx: &ActionCtx,
        object: &str,
        _cursor: Option<&Value>,
        _limit: u32,
    ) -> Result<Page, ConnectorError> {
        let tables = self.tables.lock().unwrap();
        let records = tables
            .get(object)
            .map(|t| {
                t.iter()
                    .map(|(id, v)| Record {
                        id: id.clone(),
                        modified_at: None,
                        values: v.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Page {
            records,
            next_cursor: None,
        })
    }
}
