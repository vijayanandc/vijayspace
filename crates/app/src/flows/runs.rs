//! Use cases: test run (draft), manual run (published), single-step test.

use serde_json::{Value, json};
use weft_domain::{
    error_codes,
    flow::{StepKind, has_errors, validate},
    ids::{ExecutionId, FlowId, OrgId},
    model::{ExecutionMode, FlowVersion, VersionKind},
};
use weft_expr::render_value;

use super::manage::{load, registration};
use crate::{
    AppError, Deps,
    executions::start::new_execution,
    runner::{call_action, step_error},
};

/// Runs the current draft once with `payload` as the trigger output.
pub async fn test_run(
    deps: &Deps,
    org: OrgId,
    id: FlowId,
    payload: Option<Value>,
) -> Result<ExecutionId, AppError> {
    let flow = load(deps, org, id).await?;
    let issues = validate(&flow.draft, deps.connectors.as_ref());
    if has_errors(&issues) {
        return Err(AppError::validation(
            error_codes::FLOW_INVALID,
            "Fix the flow's issues before testing it",
        )
        .with_issues(issues));
    }
    let now = deps.clock.now();
    let snapshot = FlowVersion {
        id: deps.new_version_id(),
        org_id: org,
        flow_id: id,
        kind: VersionKind::TestSnapshot,
        version: None,
        definition: flow.draft.clone(),
        created_at: now,
    };
    deps.store.insert_test_snapshot(&snapshot).await?;
    let data = match payload {
        Some(p) if !p.is_null() => p,
        Some(_) | None => registration(deps, id)
            .await?
            .sample
            .unwrap_or_else(|| json!({})),
    };
    start(deps, org, id, &snapshot, ExecutionMode::Test, data).await
}

/// Runs the published version once with `payload` as the trigger output.
pub async fn manual_run(
    deps: &Deps,
    org: OrgId,
    id: FlowId,
    payload: Option<Value>,
) -> Result<ExecutionId, AppError> {
    let flow = load(deps, org, id).await?;
    let version_id = flow.published_version_id.ok_or_else(|| {
        AppError::conflict(error_codes::FLOW_NOT_PUBLISHED, "Publish the flow first")
    })?;
    let version = deps
        .store
        .get_version(version_id)
        .await?
        .ok_or_else(|| AppError::internal("published version missing"))?;
    start(
        deps,
        org,
        id,
        &version,
        ExecutionMode::Manual,
        payload.unwrap_or_else(|| json!({})),
    )
    .await
}

async fn start(
    deps: &Deps,
    org: OrgId,
    flow: FlowId,
    version: &FlowVersion,
    mode: ExecutionMode,
    data: Value,
) -> Result<ExecutionId, AppError> {
    let item = new_execution(
        deps,
        org,
        flow,
        version.id,
        mode,
        data,
        None,
        deps.clock.now(),
        None,
    );
    let id = item.execution.id;
    deps.store
        .create_executions(org, flow, &[item], deps.clock.now())
        .await?;
    deps.wake.notify_one();
    Ok(id)
}

/// Renders one action step against a sample context and calls it once.
pub async fn test_step(
    deps: &Deps,
    org: OrgId,
    id: FlowId,
    key: &str,
    context: Option<&Value>,
) -> Result<Value, AppError> {
    let flow = load(deps, org, id).await?;
    let step = flow
        .draft
        .find_step(key)
        .ok_or_else(|| AppError::not_found(error_codes::STEP_NOT_FOUND, "Step not found"))?;
    let StepKind::Action(action) = &step.kind else {
        return Err(AppError::validation(
            error_codes::VALIDATION_FAILED,
            "Only action steps can be tested",
        ));
    };
    let reg = registration(deps, id).await?;
    let ctx = context.cloned().unwrap_or_else(|| json!({}));
    let scope = json!({
        "trigger": ctx.get("trigger").cloned().or_else(|| reg.sample.clone()).unwrap_or_else(|| json!({})),
        "steps": ctx.get("steps").cloned().unwrap_or_else(|| json!({})),
        "vars": ctx.get("vars").cloned().unwrap_or_else(|| json!({})),
        "flow": {"id": flow.id, "name": flow.name},
        "execution": {"id": "test", "mode": "test", "started_at": deps.clock.now()},
    });
    let input = match render_value(&action.input, &scope) {
        Ok(i) => i,
        Err(e) => {
            return Ok(
                json!({"input": null, "error": {"code": error_codes::MAPPING_ERROR, "message": e.to_string()}}),
            );
        }
    };
    let idem = format!("test-{}", deps.new_execution_id());
    match call_action(
        deps,
        org,
        &action.connector,
        &action.action,
        action.connection_id.as_deref(),
        input.clone(),
        idem,
        crate::limits::RequestClass::Live,
    )
    .await
    {
        Ok(output) => Ok(json!({"input": input, "output": output})),
        Err(err) => {
            let e = step_error(&err);
            Ok(
                json!({"input": input, "error": {"code": e.code, "message": e.message, "retryable": e.retryable, "details": e.details}}),
            )
        }
    }
}
