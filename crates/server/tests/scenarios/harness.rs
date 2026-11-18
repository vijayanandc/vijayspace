//! `TestApp`: a fresh database per test, the real router, a manual clock and captured email.

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt as _;
use serde_json::{Value, json};
use sqlx::Executor as _;
use tokio::sync::Notify;
use tower::ServiceExt as _;
use weft_app::{AppConfig, Deps, connections::handle::RefreshLocks, runner::plan_cache::PlanCache};
use weft_connector_sdk::testing::CapturingMailer;
use weft_domain::ids::OrgId;
use weft_http::{ReqwestSender, SsrfPolicy};
use weft_infra::{AesGcmSecretBox, UuidV7Ids};
use weft_testkit::{ManualClock, MemoryRecordsConnector};

#[allow(dead_code)] // some fields are used only by some scenarios
pub struct TestApp {
    pub deps: Deps,
    pub router: Router,
    pub clock: ManualClock,
    pub mailer: Arc<CapturingMailer>,
    pub src: MemoryRecordsConnector,
    pub dst: MemoryRecordsConnector,
    base: String,
    db: String,
}

impl TestApp {
    pub async fn spawn() -> Self {
        let base = std::env::var("WEFT_TEST_DATABASE_URL")
            .unwrap_or_else(|_| "mysql://weft:weft@127.0.0.1:3306".into());
        let db = format!("weft_test_{}", uuid::Uuid::new_v4().simple());
        let admin = weft_db::connect(&format!("{base}/mysql"), 1)
            .await
            .expect("MySQL must be running for scenario tests");
        admin
            .execute(
                format!("CREATE DATABASE {db} CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci")
                    .as_str(),
            )
            .await
            .unwrap();
        let pool = weft_db::connect(&format!("{base}/{db}"), 10).await.unwrap();
        weft_db::migrate(&pool).await.unwrap();
        let clock = ManualClock::new();
        let mailer = Arc::new(CapturingMailer::default());
        let src = MemoryRecordsConnector::new("memsrc");
        let dst = MemoryRecordsConnector::new("memdst");
        let mut registry = weft_server::wiring::registry();
        registry.register(Arc::new(src.clone()));
        registry.register(Arc::new(dst.clone()));
        let deps = Deps {
            store: Arc::new(weft_db::MySqlStore::new(pool)),
            clock: Arc::new(clock.clone()),
            ids: Arc::new(UuidV7Ids),
            secrets: Arc::new(AesGcmSecretBox::development()),
            http: Arc::new(ReqwestSender::new(SsrfPolicy::allow_private()).unwrap()),
            mailer: Some(mailer.clone()),
            scripts: Some(Arc::new(weft_infra::PooledScriptRunner::new(
                2,
                weft_script::Limits::default(),
            ))),
            connectors: Arc::new(registry),
            config: Arc::new(AppConfig {
                public_url: "http://weft.test".into(),
                default_org_id: OrgId::from_uuid(weft_server::wiring::DEFAULT_ORG),
                oauth_clients: std::collections::HashMap::from([(
                    "zoho_crm".to_owned(),
                    weft_connector_sdk::oauth::OAuthClient {
                        client_id: "test-client".into(),
                        client_secret: "test-secret".into(),
                        redirect_uri: "http://weft.test/oauth/callback".into(),
                    },
                )]),
            }),
            plans: Arc::new(PlanCache::default()),
            wake: Arc::new(Notify::new()),
            refresh_locks: Arc::new(RefreshLocks::default()),
        };
        let router = weft_api::router(weft_api::ApiState { deps: deps.clone() }, &[]);
        Self {
            deps,
            router,
            clock,
            mailer,
            src,
            dst,
            base,
            db,
        }
    }

    pub async fn req(&self, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut builder = Request::builder().method(method).uri(path);
        let body = match body {
            Some(b) => {
                builder = builder.header("content-type", "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let resp = self
            .router
            .clone()
            .oneshot(builder.body(body).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes)))
        };
        (status, json)
    }

    pub async fn ok(&self, method: &str, path: &str, body: Option<Value>) -> Value {
        let (status, json) = self.req(method, path, body).await;
        assert!(status.is_success(), "{method} {path} → {status}: {json}");
        json
    }

    pub async fn drain(&self) -> usize {
        weft_worker::drain(&self.deps).await
    }

    /// Creates, publishes and enables a flow; returns its JSON.
    pub async fn live_flow(&self, name: &str, definition: Value) -> Value {
        let flow = self
            .ok(
                "POST",
                "/api/v1/flows",
                Some(json!({"name": name, "definition": definition})),
            )
            .await;
        let id = flow["id"].as_str().unwrap().to_owned();
        self.ok("POST", &format!("/api/v1/flows/{id}/publish"), None)
            .await;
        self.ok("POST", &format!("/api/v1/flows/{id}/enable"), None)
            .await
    }

    pub async fn executions(&self, flow_id: &str) -> Vec<Value> {
        let page = self
            .ok(
                "GET",
                &format!("/api/v1/executions?flow_id={flow_id}"),
                None,
            )
            .await;
        page["items"].as_array().cloned().unwrap_or_default()
    }

    pub async fn connection(&self, connector: &str, name: &str) -> String {
        let c = self
            .ok(
                "POST",
                "/api/v1/connections",
                Some(json!({"connector": connector, "name": name, "fields": {"token": "t"}})),
            )
            .await;
        c["id"].as_str().unwrap().to_owned()
    }

    /// Inserts a connection directly (OAuth apps cannot be created through the API in tests).
    pub async fn stored_connection(&self, connector: &str, name: &str) -> String {
        self.ok("GET", "/api/v1/org", None).await; // makes sure the default org exists
        let now = weft_app::ports::Clock::now(&self.clock);
        let connection = weft_domain::model::Connection {
            id: self.deps.new_connection_id(),
            org_id: OrgId::from_uuid(weft_server::wiring::DEFAULT_ORG),
            connector: connector.to_owned(),
            name: name.to_owned(),
            config: json!({}),
            status: weft_domain::model::ConnectionStatus::Active,
            created_at: now,
            updated_at: now,
        };
        let id = connection.id.to_string();
        self.deps
            .store
            .insert_connection(&weft_app::ports::StoredConnection {
                connection,
                ciphertext: Vec::new(),
                key_id: "test".into(),
            })
            .await
            .unwrap();
        id
    }

    pub async fn cleanup(self) {
        if let Ok(admin) = weft_db::connect(&format!("{}/mysql", self.base), 1).await {
            let _ = admin
                .execute(format!("DROP DATABASE {}", self.db).as_str())
                .await;
        }
    }
}
