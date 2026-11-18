//! `core.script`: custom functions in Rhai (doc 07 §9). The script runs through the platform's
//! `ScriptRunner` (sandboxed, off the async threads); syntax is checked when a flow is validated.

use weft_connector_sdk::prelude::*;
use weft_script::Limits;

use crate::util::req_str;

pub const CONNECTOR_KEY: &str = "core.script";

/// Key under which `print` lines are added to the step output.
pub const LOGS_KEY: &str = "_logs";

/// The custom function connector.
#[derive(Debug)]
pub struct ScriptConnector {
    manifest: ConnectorManifest,
}

impl ScriptConnector {
    #[must_use]
    pub fn new() -> Self {
        let run = ActionSpec::new(
            "run",
            "Custom function",
            "Run a Rhai script on the step's input. Sandboxed: no network, files or clock.",
        )
        .field(
            FieldSpec::new("code", "Code", FieldKind::Code)
                .required()
                .not_mappable()
                .placeholder("#{ total: input.price * input.qty }")
                .help("Rhai. The values below are `input.<name>`; `context.now` is when the step started. The last expression is the result; `throw \"message\"` fails the step; `print(...)` lines are kept in `_logs`."),
        )
        .field(FieldSpec::new("input", "Input values", FieldKind::KeyValue).help("Named values the script reads as `input.<name>`. Map variables from earlier steps here."))
        .field(
            FieldSpec::select("language", "Language")
                .options(&[("rhai", "Rhai")])
                .default_value(json!("rhai"))
                .not_mappable()
                .advanced(),
        )
        .output(json!({"result": "…"}));
        Self {
            manifest: ConnectorManifest {
                key: CONNECTOR_KEY.into(),
                name: "Custom function".into(),
                description: "Write a small Rhai script for logic the built-in steps don't cover."
                    .into(),
                icon: "ƒ".into(),
                category: "Logic".into(),
                auth: AuthSpec::none(),
                actions: vec![run],
                triggers: Vec::new(),
                docs_url: None,
            },
        }
    }
}

impl Default for ScriptConnector {
    fn default() -> Self {
        Self::new()
    }
}

/// `{{` and `{%` start templates in step inputs, so they cannot appear in code.
fn template_markers(code: &str) -> Option<String> {
    (code.contains("{{") || code.contains("{%")).then(|| {
        "The code contains `{{` or `{%`, which start templates in step inputs; add a space (`{ {`)".to_owned()
    })
}

fn check_language(input: &Value) -> Result<(), ConnectorError> {
    match input.get("language").and_then(Value::as_str) {
        None | Some("rhai" | "") => Ok(()),
        Some(other) => Err(ConnectorError::invalid_field(
            "language",
            format!("Unsupported language `{other}`"),
        )),
    }
}

#[async_trait]
impl Connector for ScriptConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    async fn execute(
        &self,
        ctx: &ActionCtx,
        action: &str,
        input: Value,
    ) -> Result<Value, ConnectorError> {
        if action != "run" {
            return Err(ConnectorError::unsupported(action));
        }
        check_language(&input)?;
        let code = req_str(&input, "code")?;
        let values = match input.get("input") {
            Some(Value::Object(map)) => Value::Object(map.clone()),
            None | Some(Value::Null) => json!({}),
            Some(_) => {
                return Err(ConnectorError::invalid_field(
                    "input",
                    "Input values must be name → value pairs",
                ));
            }
        };
        let runner = ctx.scripts.as_ref().ok_or_else(|| {
            ConnectorError::permanent("Custom functions are not available on this server")
        })?;
        let now = ctx.now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        // `now` is when this step (attempt) started; scripts have no clock of their own.
        let context = json!({"now": now, "org_id": ctx.org_id});
        let run = runner.run(&code, values, context).await?;
        let mut output = run.output;
        if !run.logs.is_empty()
            && let Some(map) = output.as_object_mut()
        {
            map.entry(LOGS_KEY).or_insert_with(|| json!(run.logs));
        }
        Ok(output)
    }

    fn check_input(&self, action: &str, input: &Value) -> Vec<(String, String)> {
        if action != "run" {
            return Vec::new();
        }
        let mut issues = Vec::new();
        if let Err(e) = check_language(input) {
            issues.push(("language".to_owned(), e.to_string()));
        }
        let Some(code) = input
            .get("code")
            .and_then(Value::as_str)
            .filter(|c| !c.trim().is_empty())
        else {
            return issues; // "required" is reported by the generic check
        };
        if let Some(message) = template_markers(code) {
            issues.push(("code".to_owned(), message));
        } else if let Err(e) = weft_script::check(code, &Limits::default()) {
            issues.push(("code".to_owned(), e.to_string()));
        }
        issues
    }
}
