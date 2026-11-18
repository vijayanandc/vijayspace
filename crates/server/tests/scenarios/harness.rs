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
use weft_app::{
    AppConfig, Deps, connections::handle::RefreshLocks, runner::plan_cache::PlanCache,
    tenancy::staff::OrgCreation,
};
use weft_connector_sdk::testing::CapturingMailer;
use weft_domain::ids::OrgId;
use weft_http::{ReqwestSender, SsrfPolicy};
use weft_infra::{AesGcmSecretBox, UuidV7Ids};
use weft_testkit::{FakeIdentityProvider, ManualClock, MemoryRecordsConnector, OAuthTestConnector};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

#[allow(dead_code)] // some fields are used only by some scenarios
pub struct TestApp {
    pub deps: Deps,
    pub router: Router,
    pub clock: ManualClock,
    pub mailer: Arc<CapturingMailer>,
    pub src: MemoryRecordsConnector,
    pub dst: MemoryRecordsConnector,
    /// Set by [`TestApp::spawn_signed_in`]: sign-in is on and tokens come from here.
    pub idp: Option<FakeIdentityProvider>,
    /// The token endpoint of the `oauthapp` test app (answers every code exchange).
    pub oauth_provider: MockServer,
    base: String,
    db: String,
}

impl TestApp {
    /// Sign-in turned off: every request acts as an owner of the default org.
    pub async fn spawn() -> Self {
        Self::build(None, OrgCreation::Anyone).await
    }

    /// Sign-in turned on with an in-process identity provider (`owner@weft.test` becomes an owner
    /// of the default org when it signs in).
    pub async fn spawn_signed_in() -> Self {
        Self::build(Some(FakeIdentityProvider::default()), OrgCreation::Anyone).await
    }

    /// Signed in, with the production default: only staff (owners and admins of the default org) create orgs.
    pub async fn spawn_staff_only() -> Self {
        Self::build(Some(FakeIdentityProvider::default()), OrgCreation::Staff).await
    }

    async fn build(idp: Option<FakeIdentityProvider>, org_creation: OrgCreation) -> Self {
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
        let oauth_provider = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "at-1", "refresh_token": "rt-1", "expires_in": 3600, "token_type": "Bearer"
            })))
            .mount(&oauth_provider)
            .await;
        let mut registry = weft_server::wiring::registry();
        registry.register(Arc::new(src.clone()));
        registry.register(Arc::new(dst.clone()));
        registry.register(Arc::new(OAuthTestConnector::new(&format!(
            "{}/token",
            oauth_provider.uri()
        ))));
        let identity: Option<Arc<dyn weft_app::ports::IdentityVerifier>> = idp
            .clone()
            .map(|i| Arc::new(i) as Arc<dyn weft_app::ports::IdentityVerifier>);
        let deps = Deps {
            identity,
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
                default_retention_days: 30,
                default_org_id: OrgId::from_uuid(weft_server::wiring::DEFAULT_ORG),
                oauth_clients: ["zoho_crm", "oauthapp"]
                    .into_iter()
                    .map(|key| {
                        (
                            key.to_owned(),
                            weft_connector_sdk::oauth::OAuthClient {
                                client_id: "test-client".into(),
                                client_secret: "test-secret".into(),
                                redirect_uri: "http://weft.test/oauth/callback".into(),
                            },
                        )
                    })
                    .collect(),
                app_url: "http://app.weft.test".into(),
                client_origins: vec![
                    "http://localhost:5173".into(),
                    "http://app.weft.test".into(),
                ],
                owner_emails: vec!["owner@weft.test".into()],
                org_creation,
                alert_emails: vec!["ops@weft.test".into()],
                sign_in: idp.as_ref().map(|_| weft_app::SignInConfig {
                    endpoint: "https://tenant.logto.test".into(),
                    app_id: "web-app".into(),
                    resource: None,
                    scopes: vec!["openid".into(), "profile".into(), "email".into()],
                }),
            }),
            plans: Arc::new(PlanCache::default()),
            wake: Arc::new(Notify::new()),
            limiters: Arc::new(weft_app::limits::ConnectionLimiters::default()),
            refresh_locks: Arc::new(RefreshLocks::default()),
            instances: Arc::default(),
        };
        let router = weft_api::router(weft_api::ApiState::new(deps.clone()), &[]);
        Self {
            deps,
            router,
            clock,
            mailer,
            src,
            dst,
            idp,
            oauth_provider,
            base,
            db,
        }
    }

    pub async fn req(&self, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        self.send(Request::builder(), method, path, body).await
    }

    /// A request as a signed-in user (`token`), acting in `org` when given.
    pub async fn req_as(
        &self,
        token: &str,
        org: Option<&str>,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder().header("authorization", format!("Bearer {token}"));
        if let Some(org) = org {
            builder = builder.header("x-org-id", org);
        }
        self.send(builder, method, path, body).await
    }

    /// A GET that answers with a redirect: its `Location`.
    pub async fn redirect(&self, path: &str) -> String {
        let resp = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(resp.status().is_redirection(), "{path} → {}", resp.status());
        resp.headers()["location"].to_str().unwrap().to_owned()
    }

    async fn send(
        &self,
        builder: axum::http::request::Builder,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut builder = builder.method(method).uri(path);
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
            created_by: None,
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

    /// This test's database (for checks the API does not show, e.g. partitions).
    pub fn database_url(&self) -> String {
        format!("{}/{}", self.base, self.db)
    }

    pub async fn cleanup(self) {
        if let Ok(admin) = weft_db::connect(&format!("{}/mysql", self.base), 1).await {
            let _ = admin
                .execute(format!("DROP DATABASE {}", self.db).as_str())
                .await;
        }
    }
}
