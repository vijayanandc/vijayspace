//! Instant triggers via APS Webhooks (issues, cost) and issue polling for reconciliation.

use chrono::SecondsFormat;
use weft_connector_sdk::{
    EventBatch, EventItem, InboundRequest, PollResult, SubscribeRequest, SubscriptionInfo,
    prelude::*,
};

use super::{
    actions::fetch_issue,
    client::{Forma, check, plain_id, results, seg},
};
use crate::util::{list, req_str};

const ISSUES_SYSTEM: &str = "autodesk.construction.issues";
const COST_SYSTEM: &str = "autodesk.construction.cost";

fn system_of(trigger: &str) -> Result<&'static str, ConnectorError> {
    match trigger {
        "issue_event" => Ok(ISSUES_SYSTEM),
        "cost_event" => Ok(COST_SYSTEM),
        other => Err(ConnectorError::unsupported(format!("trigger `{other}`"))),
    }
}

fn events(trigger: &str, input: &Value) -> Vec<String> {
    let chosen = list(input, "events");
    if !chosen.is_empty() {
        return chosen;
    }
    let defaults: &[&str] = if trigger == "cost_event" {
        &["changeOrder.created-1.0", "changeOrder.updated-1.0"]
    } else {
        &["issue.created-1.0", "issue.updated-1.0"]
    };
    defaults.iter().map(|e| (*e).to_owned()).collect()
}

fn hook_path(system: &str, event: &str) -> String {
    format!(
        "/webhooks/v1/systems/{}/events/{}/hooks",
        seg(system),
        seg(event)
    )
}

/// Creates one APS hook per selected event. On failure, hooks already created are removed.
pub(crate) async fn subscribe(
    ctx: &ActionCtx,
    trigger: &str,
    input: &Value,
    req: &SubscribeRequest,
) -> Result<SubscriptionInfo, ConnectorError> {
    let system = system_of(trigger)?;
    let project = req_str(input, "project_id")?;
    let api = Forma::new(ctx)?;
    // VERIFY (spike M37a): issue/cost webhook scope uses the `b.`-prefixed project ID.
    let scope = json!({"project": format!("b.{}", plain_id(&project))});
    let mut hooks: Vec<Value> = Vec::new();
    for event in events(trigger, input) {
        let body = json!({
            "callbackUrl": req.target_url,
            "scope": scope,
            "hookAttribute": {"verify_token": req.verify_token},
            "autoReactivateHook": true,
        });
        // VERIFY (spike M37a): region header name for webhook calls.
        let request = HttpRequest::post(api.url(&hook_path(system, &event)))
            .header("x-ads-region", api.region())
            .json(&body);
        let created = api.send(request).await.and_then(|resp| {
            check(&resp)?;
            resp.header("location")
                .and_then(|l| l.trim_end_matches('/').rsplit('/').next())
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| {
                    ConnectorError::permanent("Autodesk did not return the new hook's location")
                })
        });
        match created {
            Ok(hook_id) => hooks.push(json!({"event": event, "hook_id": hook_id})),
            Err(err) => {
                // Best-effort cleanup; the original error is what the user needs to see.
                let _ = delete_hooks(&api, system, &hooks).await;
                return Err(err);
            }
        }
    }
    let external_id = hooks
        .iter()
        .map(|h| {
            format!(
                "{}|{}",
                h["event"].as_str().unwrap_or_default(),
                h["hook_id"].as_str().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(SubscriptionInfo {
        external_id,
        expires_at: None,
        data: json!({"system": system, "project_id": project, "hooks": hooks}),
    })
}

async fn delete_hooks(
    api: &Forma<'_>,
    system: &str,
    hooks: &[Value],
) -> Result<(), ConnectorError> {
    let mut first_error = None;
    for hook in hooks {
        let (Some(event), Some(id)) = (
            hook.get("event").and_then(Value::as_str),
            hook.get("hook_id").and_then(Value::as_str),
        ) else {
            continue;
        };
        let url = api.url(&format!("{}/{}", hook_path(system, event), seg(id)));
        let result = api
            .send(HttpRequest::new(Method::Delete, url).header("x-ads-region", api.region()))
            .await;
        let outcome = result.and_then(|resp| {
            if resp.status == 404 {
                Ok(())
            } else {
                check(&resp)
            }
        });
        if let Err(err) = outcome {
            first_error.get_or_insert(err);
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// Removes every hook created by `subscribe` (404 counts as already removed).
pub(crate) async fn unsubscribe(
    ctx: &ActionCtx,
    trigger: &str,
    sub: &SubscriptionInfo,
) -> Result<(), ConnectorError> {
    let system = sub.data.get("system").and_then(Value::as_str).map_or_else(
        || system_of(trigger).map(str::to_owned),
        |s| Ok(s.to_owned()),
    )?;
    let hooks: Vec<Value> = sub
        .data
        .get("hooks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| {
            sub.external_id
                .split(',')
                .filter_map(|pair| pair.split_once('|'))
                .map(|(event, id)| json!({"event": event, "hook_id": id}))
                .collect()
        });
    let api = Forma::new(ctx)?;
    delete_hooks(&api, &system, &hooks).await
}

/// Constant-time string comparison for verification tokens.
fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn str_of<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|k| v.get(*k).and_then(Value::as_str))
        .filter(|s| !s.is_empty())
}

/// Verifies an APS callback and turns it into event items.
pub(crate) async fn handle_event(
    ctx: &ActionCtx,
    trigger: &str,
    input: &Value,
    request: &InboundRequest,
    verify_token: &str,
) -> Result<EventBatch, ConnectorError> {
    let body = &request.body;
    let payload = body.get("payload").filter(|p| !p.is_null());
    let Some(payload) = payload else {
        // APS verification ping (no payload): acknowledge without running anything.
        return Ok(EventBatch {
            items: Vec::new(),
            response: None,
        });
    };
    let hook = body.get("hook").cloned().unwrap_or(Value::Null);
    let token = hook
        .get("hookAttribute")
        .and_then(|a| a.get("verify_token"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !ct_eq(token, verify_token) {
        return Err(ConnectorError::invalid("webhook verification failed"));
    }
    let event = str_of(&hook, &["event"])
        .or_else(|| str_of(body, &["event", "eventType"]))
        .unwrap_or("unknown")
        .to_owned();
    // VERIFY (spike M37a): where the project ID appears in issue/cost payloads.
    let project = str_of(payload, &["projectId", "project_id"])
        .or_else(|| {
            hook.get("scope")
                .and_then(|s| s.get("project"))
                .and_then(Value::as_str)
        })
        .map(|p| plain_id(p).to_owned())
        .or_else(|| crate::util::opt_str(input, "project_id"))
        .unwrap_or_default();
    let item = if system_of(trigger)? == ISSUES_SYSTEM {
        issue_item(ctx, &event, &project, payload).await?
    } else {
        let id = str_of(payload, &["id"]).unwrap_or_default().to_owned();
        let version = str_of(payload, &["updatedAt", "lastModifiedAt", "modifiedAt"])
            .unwrap_or(&event)
            .to_owned();
        EventItem {
            dedupe_key: format!("{id}:{event}:{version}"),
            record_id: (!id.is_empty()).then_some(id),
            data: json!({"event": event, "project_id": project, "payload": payload}),
        }
    };
    Ok(EventBatch {
        items: vec![item],
        response: None,
    })
}

async fn issue_item(
    ctx: &ActionCtx,
    event: &str,
    project: &str,
    payload: &Value,
) -> Result<EventItem, ConnectorError> {
    let issue_id = str_of(payload, &["id", "issueId"])
        .ok_or_else(|| ConnectorError::invalid("issue event without an issue id"))?
        .to_owned();
    let issue = if event.starts_with("issue.deleted") || project.is_empty() {
        payload.clone()
    } else {
        match fetch_issue(ctx, project, &issue_id).await {
            Ok(issue) => issue,
            Err(ConnectorError::NotFound { .. }) => payload.clone(),
            Err(err) => return Err(err),
        }
    };
    let version = str_of(&issue, &["updatedAt"])
        .or_else(|| str_of(payload, &["updatedAt"]))
        .unwrap_or(event)
        .to_owned();
    Ok(EventItem {
        dedupe_key: format!("{issue_id}:{version}"),
        record_id: Some(issue_id),
        data: json!({"event": event, "project_id": project, "issue": issue}),
    })
}

/// Issue reconciliation: issues updated since the cursor, oldest first. The first poll only
/// records a baseline (now) so connecting a large project does not start thousands of runs.
pub(crate) async fn poll(
    ctx: &ActionCtx,
    trigger: &str,
    input: &Value,
    cursor: Option<&Value>,
) -> Result<PollResult, ConnectorError> {
    if trigger != "issue_event" {
        return Err(ConnectorError::unsupported(format!(
            "polling for `{trigger}`"
        )));
    }
    let project = req_str(input, "project_id")?;
    let Some(since) = cursor.and_then(Value::as_str) else {
        let baseline = ctx.now.to_rfc3339_opts(SecondsFormat::Millis, true);
        return Ok(PollResult {
            items: Vec::new(),
            cursor: Some(Value::String(baseline)),
        });
    };
    let api = Forma::new(ctx)?;
    let url = api.url(&format!(
        "/construction/issues/v1/projects/{}/issues",
        seg(plain_id(&project))
    ));
    let range = format!("{since}..");
    // VERIFY (spike M37a): open-ended updatedAt range filter and sortBy parameter.
    let query = [
        ("filter[updatedAt]", range.as_str()),
        ("sortBy", "updatedAt"),
        ("limit", "100"),
    ];
    let body = api.json(HttpRequest::get(url).query(&query)).await?;
    let mut issues = results(&body);
    issues.sort_by(|a, b| str_of(a, &["updatedAt"]).cmp(&str_of(b, &["updatedAt"])));
    let mut items = Vec::with_capacity(issues.len());
    let mut next = since.to_owned();
    for issue in issues {
        let (Some(id), Some(updated)) = (str_of(&issue, &["id"]), str_of(&issue, &["updatedAt"]))
        else {
            continue;
        };
        if updated > next.as_str() {
            updated.clone_into(&mut next);
        }
        items.push(EventItem {
            dedupe_key: format!("{id}:{updated}"),
            record_id: Some(id.to_owned()),
            data: json!({"event": "issue.changed", "project_id": plain_id(&project), "issue": issue}),
        });
    }
    Ok(PollResult {
        items,
        cursor: Some(Value::String(next)),
    })
}
