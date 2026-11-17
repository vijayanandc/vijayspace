//! The Autodesk Forma manifest: OAuth (APS), actions, triggers and their fields.

use weft_connector_sdk::{AuthKind, ClientAuth, OAuth2Spec, prelude::*};

/// Issue webhook events (`autodesk.construction.issues`).
pub(crate) const ISSUE_EVENTS: [(&str, &str); 3] = [
    ("issue.created-1.0", "Issue created"),
    ("issue.updated-1.0", "Issue updated"),
    ("issue.deleted-1.0", "Issue deleted"),
];

/// Cost Management webhook events (`autodesk.construction.cost`).
// VERIFY (spike M37a): exact Cost Management event names and versions.
pub(crate) const COST_EVENTS: [(&str, &str); 5] = [
    ("changeOrder.created-1.0", "Change order created"),
    ("changeOrder.updated-1.0", "Change order updated"),
    ("changeOrder.deleted-1.0", "Change order deleted"),
    ("budget.updated-1.0", "Budget updated"),
    ("contract.updated-1.0", "Contract updated"),
];

fn project_field() -> FieldSpec {
    FieldSpec::select("project_id", "Project")
        .required()
        .options_from("projects")
        .depends_on(&["account_id"])
}

fn account_field() -> FieldSpec {
    FieldSpec::select("account_id", "Account")
        .options_from("accounts")
        .help("Defaults to the account set on the connection")
}

fn issue_status() -> FieldSpec {
    FieldSpec::select("status", "Status").options(&[
        ("draft", "Draft"),
        ("open", "Open"),
        ("pending", "Pending"),
        ("in_progress", "In progress"),
        ("completed", "Completed"),
        ("in_review", "In review"),
        ("not_approved", "Not approved"),
        ("in_dispute", "In dispute"),
        ("closed", "Closed"),
    ])
}

fn auth() -> AuthSpec {
    AuthSpec {
        kind: AuthKind::Oauth2,
        fields: vec![
            FieldSpec::select("region", "Data region")
                .options(&[("US", "US"), ("EMEA", "EMEA"), ("AUS", "Australia")])
                .default_value(json!("US"))
                .not_mappable(),
            FieldSpec::string("account_id", "Account ID")
                .help("Forma account (hub) ID without the b. prefix")
                .not_mappable(),
        ],
        oauth2: Some(OAuth2Spec {
            authorize_url: "https://developer.api.autodesk.com/authentication/v2/authorize".into(),
            token_url: "https://developer.api.autodesk.com/authentication/v2/token".into(),
            scopes: [
                "data:read",
                "data:write",
                "data:create",
                "account:read",
                "account:write",
            ]
            .map(String::from)
            .to_vec(),
            scope_separator: " ".into(),
            extra_authorize_params: Vec::new(),
            pkce: true,
            client_auth: ClientAuth::Basic,
        }),
    }
}

fn actions() -> Vec<ActionSpec> {
    vec![
        ActionSpec::new("create_project", "Create project", "Create a Forma Build project in an account (needs account admin).")
            .field(account_field())
            .field(FieldSpec::string("name", "Project name").required())
            .field(FieldSpec::string("type", "Project type").default_value(json!("Office")).help("e.g. Office, Hospital, Residential"))
            .field(FieldSpec::string("job_number", "Job number"))
            .field(FieldSpec::new("start_date", "Start date", FieldKind::Datetime))
            .field(FieldSpec::new("end_date", "End date", FieldKind::Datetime))
            .field(FieldSpec::new("project_value", "Project value", FieldKind::Number))
            .field(FieldSpec::string("currency", "Currency").default_value(json!("USD")).advanced())
            .output(json!({"id": "a1b2c3d4-0000-4000-8000-000000000001", "name": "Riverside Clinic", "status": "pending", "account_id": "acc1"})),
        ActionSpec::new("add_project_user", "Add project member", "Invite a user to a Forma project.")
            .field(account_field())
            .field(project_field())
            .field(FieldSpec::string("email", "Email").required())
            .field(FieldSpec::string("products", "Products").help("Comma-separated product keys, e.g. docs, build").advanced())
            .output(json!({"id": "u1", "email": "pm@example.com", "status": "pending"})),
        ActionSpec::new("create_issue", "Create issue", "Create an issue in a Forma project.")
            .field(account_field())
            .field(project_field())
            .field(FieldSpec::string("title", "Title").required())
            .field(FieldSpec::text("description", "Description"))
            .field(FieldSpec::select("issue_subtype_id", "Issue type").required().options_from("issue_subtypes").depends_on(&["project_id"]))
            .field(issue_status().default_value(json!("open")))
            .field(FieldSpec::new("due_date", "Due date", FieldKind::Datetime))
            .field(FieldSpec::string("assigned_to", "Assignee ID").advanced())
            .output(json!({"id": "iss-1", "displayId": 42, "title": "Leak in level 2", "status": "open"})),
        ActionSpec::new("update_issue", "Update issue", "Change an issue's status, title, description or due date.")
            .field(account_field())
            .field(project_field())
            .field(FieldSpec::string("issue_id", "Issue ID").required())
            .field(issue_status())
            .field(FieldSpec::string("title", "Title"))
            .field(FieldSpec::text("description", "Description"))
            .field(FieldSpec::new("due_date", "Due date", FieldKind::Datetime))
            .output(json!({"id": "iss-1", "status": "closed"})),
        ActionSpec::new("get_issue", "Get issue", "Fetch one issue.")
            .field(account_field())
            .field(project_field())
            .field(FieldSpec::string("issue_id", "Issue ID").required())
            .output(json!({"id": "iss-1", "title": "Leak in level 2", "status": "open", "updatedAt": "2026-01-01T00:00:00.000Z"}))
            .idempotent(),
        ActionSpec::new("list_issues", "List issues", "List issues in a project, optionally by status.")
            .field(account_field())
            .field(project_field())
            .field(issue_status())
            .field(FieldSpec::new("limit", "Limit", FieldKind::Number).default_value(json!(50)).advanced())
            .output(json!({"issues": [{"id": "iss-1", "title": "Leak"}], "count": 1}))
            .idempotent(),
        ActionSpec::new("create_rfi", "Create RFI", "Create a request for information in a Forma Build project.")
            .field(account_field())
            .field(project_field())
            .field(FieldSpec::string("title", "Title").required())
            .field(FieldSpec::text("question", "Question").required())
            .field(FieldSpec::string("assigned_to", "Assignee ID").advanced())
            .output(json!({"id": "rfi-1", "title": "Beam size", "status": "draft"})),
        ActionSpec::new("get_project", "Get project", "Fetch one project.")
            .field(account_field())
            .field(project_field())
            .output(json!({"id": "a1b2c3d4-0000-4000-8000-000000000001", "name": "Riverside Clinic", "status": "active"}))
            .idempotent(),
    ]
}

fn triggers() -> Vec<TriggerSpec> {
    vec![
        TriggerSpec::new("issue_event", "Issue created or updated", "Runs when an issue changes in a Forma project.", TriggerKind::Instant)
            .field(account_field())
            .field(project_field())
            .field(FieldSpec::select("events", "Events").options(&ISSUE_EVENTS).multiple().default_value(json!(["issue.created-1.0", "issue.updated-1.0"])).not_mappable())
            .output(json!({
                "event": "issue.updated-1.0",
                "project_id": "a1b2c3d4-0000-4000-8000-000000000001",
                "issue": {"id": "iss-1", "displayId": 42, "title": "Leak in level 2", "status": "open", "updatedAt": "2026-01-01T00:00:00.000Z"}
            }))
            .with_polling(),
        TriggerSpec::new("cost_event", "Cost change (change orders, budgets)", "Runs when change orders, budgets or contracts change.", TriggerKind::Instant)
            .field(account_field())
            .field(project_field())
            .field(FieldSpec::select("events", "Events").options(&COST_EVENTS).multiple().default_value(json!(["changeOrder.created-1.0", "changeOrder.updated-1.0"])).not_mappable())
            .output(json!({
                "event": "changeOrder.updated-1.0",
                "project_id": "a1b2c3d4-0000-4000-8000-000000000001",
                "payload": {"id": "co-1", "name": "Extra glazing", "status": "approved", "amount": 12500}
            })),
    ]
}

/// The complete manifest.
pub(crate) fn build() -> ConnectorManifest {
    ConnectorManifest {
        key: "autodesk_forma".into(),
        name: "Autodesk Forma".into(),
        description: "Forma Build (formerly Autodesk Construction Cloud): projects, issues, RFIs and cost changes.".into(),
        icon: "🏗️".into(),
        category: "Construction".into(),
        auth: auth(),
        actions: actions(),
        triggers: triggers(),
        docs_url: Some("https://aps.autodesk.com/en/docs/acc/v1/overview/".into()),
    }
}
