//! Flows: CRUD, drafts, publish, enable, runs, step tests.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post, put},
};
use serde::Deserialize;
use serde_json::{Value, json};
use weft_app::{Deps, flows as uc, flows::manage_urls::has_unpublished_changes, tenancy::Actor};
use weft_domain::permissions::Permission;
use weft_domain::{flow::FlowDefinition, ids::FlowId};

use crate::{
    dto::{FlowDto, FlowSummaryDto, VersionDto},
    error::{ApiResult, bad_id},
    extract::Acting,
    state::ApiState,
};

pub fn router() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/flows", get(list).post(create))
        .route("/api/v1/flows/{id}", get(one).patch(update).delete(remove))
        .route("/api/v1/flows/{id}/draft", put(save_draft))
        .route("/api/v1/flows/{id}/validate", post(validate))
        .route("/api/v1/flows/{id}/publish", post(publish))
        .route("/api/v1/flows/{id}/enable", post(enable))
        .route("/api/v1/flows/{id}/disable", post(disable))
        .route("/api/v1/flows/{id}/versions", get(versions))
        .route("/api/v1/flows/{id}/trigger/listen", post(listen))
        .route("/api/v1/flows/{id}/test", post(test_run))
        .route("/api/v1/flows/{id}/run", post(run))
        .route("/api/v1/flows/{id}/steps/{key}/test", post(test_step))
        .route("/api/v1/flows/{id}/backfill", post(backfill))
}

fn id(raw: &str) -> Result<FlowId, crate::ApiError> {
    raw.parse().map_err(|_| bad_id("Flow"))
}

/// The flow as the actor may see it: the trigger's URLs and secrets let anyone start the flow, so they
/// are left out for viewers.
async fn dto(deps: &Deps, actor: &Actor, view: &uc::FlowView) -> ApiResult<FlowDto> {
    let changed = has_unpublished_changes(deps, &view.flow).await?;
    let secrets = actor.require(Permission::FlowEdit).is_ok();
    Ok(FlowDto::new(deps, view, changed, secrets))
}

async fn list(
    State(s): State<ApiState>,
    Acting(actor): Acting,
) -> ApiResult<Json<Vec<FlowSummaryDto>>> {
    let rows = uc::list_flows(&s.deps, &actor).await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in &rows {
        out.push(FlowSummaryDto::from_row(
            row,
            has_unpublished_changes(&s.deps, &row.flow).await?,
        ));
    }
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    name: String,
    description: Option<String>,
    definition: Option<FlowDefinition>,
}

async fn create(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Json(b): Json<CreateBody>,
) -> ApiResult<(StatusCode, Json<FlowDto>)> {
    let view = uc::create_flow(
        &s.deps,
        &actor,
        &b.name,
        b.description.as_deref(),
        b.definition,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(dto(&s.deps, &actor, &view).await?),
    ))
}

async fn one(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
) -> ApiResult<Json<FlowDto>> {
    let view = uc::get_flow(&s.deps, &actor, id(&raw)?).await?;
    Ok(Json(dto(&s.deps, &actor, &view).await?))
}

#[derive(Debug, Deserialize)]
struct UpdateBody {
    name: Option<String>,
    /// Absent = unchanged, `null` = cleared (PATCH semantics need the two layers).
    #[allow(clippy::option_option)]
    #[serde(default, deserialize_with = "double_option")]
    description: Option<Option<String>>,
}

#[allow(clippy::option_option)]
fn double_option<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<Option<String>>, D::Error> {
    Ok(Some(Option::<String>::deserialize(d)?))
}

async fn update(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
    Json(b): Json<UpdateBody>,
) -> ApiResult<Json<FlowDto>> {
    let view = uc::update_flow(
        &s.deps,
        &actor,
        id(&raw)?,
        b.name.as_deref(),
        b.description.as_ref().map(Option::as_deref),
    )
    .await?;
    Ok(Json(dto(&s.deps, &actor, &view).await?))
}

async fn remove(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
) -> ApiResult<StatusCode> {
    uc::delete_flow(&s.deps, &actor, id(&raw)?).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct DraftBody {
    definition: FlowDefinition,
    revision: u32,
}

async fn save_draft(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
    Json(b): Json<DraftBody>,
) -> ApiResult<Json<Value>> {
    let (revision, issues) =
        uc::save_draft(&s.deps, &actor, id(&raw)?, &b.definition, b.revision).await?;
    Ok(Json(json!({"revision": revision, "issues": issues})))
}

async fn validate(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({"issues": uc::validate_draft(&s.deps, &actor, id(&raw)?).await?}),
    ))
}

async fn publish(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
) -> ApiResult<Json<Value>> {
    let (version, issues) = uc::publish_flow(&s.deps, &actor, id(&raw)?).await?;
    Ok(Json(
        json!({"version": VersionDto { id: version.id, version: version.version, created_at: version.created_at }, "issues": issues}),
    ))
}

async fn enable(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
) -> ApiResult<Json<FlowDto>> {
    let view = uc::set_enabled(&s.deps, &actor, id(&raw)?, true).await?;
    Ok(Json(dto(&s.deps, &actor, &view).await?))
}

async fn disable(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
) -> ApiResult<Json<FlowDto>> {
    let view = uc::set_enabled(&s.deps, &actor, id(&raw)?, false).await?;
    Ok(Json(dto(&s.deps, &actor, &view).await?))
}

async fn versions(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
) -> ApiResult<Json<Vec<VersionDto>>> {
    let versions = uc::list_versions(&s.deps, &actor, id(&raw)?).await?;
    Ok(Json(
        versions
            .into_iter()
            .map(|v| VersionDto {
                id: v.id,
                version: v.version,
                created_at: v.created_at,
            })
            .collect(),
    ))
}

async fn listen(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
) -> ApiResult<Json<Value>> {
    let view = uc::start_listening(&s.deps, &actor, id(&raw)?).await?;
    Ok(Json(json!(dto(&s.deps, &actor, &view).await?.trigger)))
}

#[derive(Debug, Deserialize)]
struct PayloadBody {
    #[serde(default)]
    payload: Option<Value>,
}

async fn test_run(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
    body: Option<Json<PayloadBody>>,
) -> ApiResult<Json<Value>> {
    let payload = body.and_then(|Json(b)| b.payload);
    let exe = uc::test_run(&s.deps, &actor, id(&raw)?, payload).await?;
    Ok(Json(json!({"execution_id": exe})))
}

async fn run(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
    body: Option<Json<PayloadBody>>,
) -> ApiResult<Json<Value>> {
    let payload = body.and_then(|Json(b)| b.payload);
    let exe = uc::manual_run(&s.deps, &actor, id(&raw)?, payload).await?;
    Ok(Json(json!({"execution_id": exe})))
}

#[derive(Debug, Deserialize)]
struct StepTestBody {
    #[serde(default)]
    context: Option<Value>,
}

async fn test_step(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path((raw, key)): Path<(String, String)>,
    body: Option<Json<StepTestBody>>,
) -> ApiResult<Json<Value>> {
    let ctx = body.and_then(|Json(b)| b.context);
    Ok(Json(
        uc::test_step(&s.deps, &actor, id(&raw)?, &key, ctx.as_ref()).await?,
    ))
}

#[derive(Debug, Deserialize)]
struct BackfillBody {
    #[serde(default)]
    limit: Option<u32>,
    /// `next_cursor` from the previous call, to continue.
    #[serde(default)]
    cursor: Option<Value>,
}

async fn backfill(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
    body: Option<Json<BackfillBody>>,
) -> ApiResult<Json<Value>> {
    let (limit, cursor) = body.map_or((None, None), |Json(b)| (b.limit, b.cursor));
    let page =
        weft_app::sync::backfill(&s.deps, &actor, id(&raw)?, limit.unwrap_or(500), cursor).await?;
    Ok(Json(
        json!({"enqueued": page.enqueued, "next_cursor": page.next_cursor}),
    ))
}
