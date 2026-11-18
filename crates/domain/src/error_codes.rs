//! Every stable error code the API can return. Clients switch on these strings.

pub const VALIDATION_FAILED: &str = "validation.failed";
pub const NOT_FOUND: &str = "not_found";
pub const INTERNAL: &str = "internal.error";
/// A database deadlock (retried inside the adapter; surfaces only if retries run out).
pub const DB_DEADLOCK: &str = "internal.deadlock";

pub const FLOW_NOT_FOUND: &str = "flow.not_found";
pub const FLOW_INVALID: &str = "flow.invalid";
pub const FLOW_REVISION_CONFLICT: &str = "flow.revision_conflict";
pub const FLOW_NOT_PUBLISHED: &str = "flow.not_published";
pub const STEP_NOT_FOUND: &str = "flow.step_not_found";

pub const CONNECTION_NOT_FOUND: &str = "connection.not_found";
pub const CONNECTION_NEEDS_REAUTH: &str = "connection.needs_reauth";
pub const CONNECTION_IN_USE: &str = "connection.in_use";
pub const CONNECTOR_NOT_FOUND: &str = "connector.not_found";
pub const ACTION_NOT_FOUND: &str = "connector.action_not_found";
pub const OAUTH_STATE_INVALID: &str = "oauth.state_invalid";

pub const EXECUTION_NOT_FOUND: &str = "execution.not_found";
pub const EXECUTION_NOT_RETRYABLE: &str = "execution.not_retryable";

pub const WEBHOOK_UNKNOWN: &str = "webhook.unknown";
pub const WEBHOOK_TOO_LARGE: &str = "webhook.too_large";
pub const WEBHOOK_UNAUTHORIZED: &str = "webhook.unauthorized";

pub const RECIPE_NOT_FOUND: &str = "recipe.not_found";
pub const RECIPE_SLOT_MISSING: &str = "recipe.slot_missing";
pub const RECIPE_SLOT_MISMATCH: &str = "recipe.slot_mismatch";

pub const APP_UNAVAILABLE: &str = "app.unavailable";
pub const APP_RATE_LIMITED: &str = "app.rate_limited";
pub const APP_INVALID_INPUT: &str = "app.invalid_input";
pub const APP_NOT_FOUND: &str = "app.not_found";
pub const APP_ERROR: &str = "app.error";
pub const STEP_TIMEOUT: &str = "step.timeout";
pub const STEP_OUTPUT_TOO_LARGE: &str = "step.output_too_large";
pub const MAPPING_ERROR: &str = "mapping.error";
pub const DELAY_INVALID_DATETIME: &str = "delay.invalid_datetime";
pub const ENGINE_BUDGET_EXCEEDED: &str = "engine.budget_exceeded";
pub const STOPPED_BY_FLOW: &str = "flow.stopped";
pub const CANCELLED: &str = "execution.cancelled";
pub const SCRIPT_ERROR: &str = "script.error";
pub const SCRIPT_LIMIT_EXCEEDED: &str = "script.limit_exceeded";
pub const SYNC_BUSY: &str = "sync.busy";
pub const SYNC_AMBIGUOUS_MATCH: &str = "sync.ambiguous_match";
pub const SYNC_LOOKUP_MISSING: &str = "sync.lookup_missing";
pub const BACKFILL_STALLED: &str = "sync.backfill_stalled";
