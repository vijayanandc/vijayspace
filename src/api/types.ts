// Types mirroring the REST contract in the backend's docs/api/README.md.
// Do not change these without the contract changing first.

export * from './definition';
import type { JsonObject, JsonValue, FlowDefinition } from './definition';

// ---------- Catalog ----------

export type FieldKind =
  | 'string' | 'text' | 'number' | 'boolean' | 'select' | 'datetime' | 'json' | 'key_value' | 'object' | 'code' | 'password';

export interface OptionItem {
  value: string;
  label: string;
}

export interface FieldSpec {
  key: string;
  label: string;
  type: FieldKind;
  required?: boolean;
  help?: string;
  placeholder?: string;
  default?: JsonValue;
  options?: OptionItem[];
  options_from?: string;
  fields_from?: string;
  depends_on?: string[];
  mappable?: boolean;
  multiple?: boolean;
  advanced?: boolean;
  secret?: boolean;
  read_only?: boolean;
}

export interface ActionSpec {
  key: string;
  name: string;
  description: string;
  fields: FieldSpec[];
  output_sample?: JsonValue;
  idempotent?: boolean;
}

export interface TriggerSpec {
  key: string;
  name: string;
  description: string;
  kind: 'instant' | 'polling';
  fields: FieldSpec[];
  output_sample?: JsonValue;
  setup_instructions?: string;
  supports_polling?: boolean;
}

export interface OAuth2Spec {
  authorize_url: string;
  token_url?: string;
  scopes: string[];
}

export interface AuthSpec {
  kind: 'none' | 'fields' | 'oauth2';
  fields: FieldSpec[];
  oauth2?: OAuth2Spec;
}

export interface ConnectorManifest {
  key: string;
  name: string;
  description: string;
  icon: string;
  category: string;
  auth: AuthSpec;
  actions: ActionSpec[];
  triggers: TriggerSpec[];
  docs_url?: string;
}

// ---------- Connections ----------

export type ConnectionStatus = 'active' | 'needs_reauth' | 'error';

export interface Connection {
  id: string;
  connector: string;
  name: string;
  config: JsonObject;
  status: ConnectionStatus;
  created_at: string;
  updated_at: string;
  /** Who created it (members may delete only their own); null for older connections. */
  created_by?: string | null;
}

/** Request limits of a connection. `null` means no limit (in `defaults`/`effective`) or the app default (in `overrides`). */
export interface ConnectionLimits {
  requests_per_minute: number | null;
  max_concurrent: number | null;
  daily_requests: number | null;
}

export type UsageLevel = 'ok' | 'warning' | 'critical';

/** Today's (UTC) requests through one connection against its daily budget. */
export interface ConnectionUsage {
  connection_id: string;
  day: string;
  requests: number;
  refused: number;
  daily_limit: number | null;
  percent: number | null;
  level: UsageLevel;
}

/** `GET/PUT /connections/{id}/limits`: the app's defaults, the connection's overrides, the result, and today's usage. */
export interface LimitsView {
  defaults: ConnectionLimits;
  overrides: ConnectionLimits;
  effective: ConnectionLimits;
  usage: ConnectionUsage;
}

export type ConnectionTestResult =
  | { ok: true; info?: JsonValue }
  | { ok: false; error: { code: string; message: string } };

// ---------- Flows ----------

export type Severity = 'error' | 'warning';

export interface Issue {
  severity: Severity;
  path: string;
  code: string;
  message: string;
}

export type ExecutionStatus = 'queued' | 'running' | 'waiting' | 'succeeded' | 'failed' | 'cancelled';
export type ExecutionMode = 'live' | 'test' | 'manual' | 'rerun' | 'backfill';

export interface FlowSummary {
  id: string;
  name: string;
  description: string | null;
  is_enabled: boolean;
  trigger_type: string;
  published_version: number | null;
  has_unpublished_changes: boolean;
  draft_revision: number;
  updated_at: string;
  last_execution?: { id: string; status: ExecutionStatus; queued_at: string } | null;
  /** Connector keys the draft uses: an app trigger's connector first, then actions in order, each once. */
  apps: string[];
  /** Who created it (members may delete only their own); null for older flows. */
  created_by?: string | null;
}

export interface TriggerInfo {
  kind: 'webhook' | 'schedule' | 'poll' | 'instant' | 'manual';
  token: string;
  webhook_url?: string | null;
  app_event_url?: string | null;
  is_active: boolean;
  listening_until?: string | null;
  next_fire_at?: string | null;
  sample?: JsonValue;
  /** Not in the contract table yet: shown when the server provides it (secret header value). */
  verify_token?: string | null;
}

export interface Flow extends Omit<FlowSummary, 'apps'> {
  draft: FlowDefinition;
  published_version_id: string | null;
  created_at: string;
  trigger: TriggerInfo;
}

export interface SaveDraftResult {
  revision: number;
  issues: Issue[];
}

export interface PublishResult {
  version: { id: string; version: number; created_at: string };
  issues: Issue[];
}

export interface FlowVersionInfo {
  id: string;
  version: number;
  created_at: string;
}

export interface StepTestResult {
  input?: JsonValue;
  output?: JsonValue;
  error?: { code: string; message: string; retryable?: boolean } | null;
}

// ---------- Executions ----------

export interface ExecutionSummary {
  id: string;
  flow_id: string;
  flow_name: string;
  status: ExecutionStatus;
  mode: ExecutionMode;
  queued_at: string;
  started_at: string | null;
  finished_at: string | null;
  waiting_until: string | null;
  task_count: number;
  error_code: string | null;
  error_message: string | null;
  failed_step_key: string | null;
  possible_duplicate: boolean;
}

export type StepRunStatus = 'running' | 'succeeded' | 'failed' | 'retry_scheduled' | 'skipped' | 'unknown_outcome';

export interface StepRun {
  id: string;
  seq: number;
  step_key: string;
  kind: string;
  attempt: number;
  status: StepRunStatus;
  input: JsonValue;
  output: JsonValue;
  error: JsonValue;
  warnings: string[];
  started_at: string;
  finished_at: string | null;
}

export interface ExecutionDetail extends ExecutionSummary {
  trigger_data: JsonValue;
  steps: StepRun[];
}

export interface ExecutionPage {
  items: ExecutionSummary[];
  next_cursor: string | null;
}

// ---------- Recipes / misc ----------

export interface RecipeSlot {
  slot: string;
  connector: string;
  label: string;
}

export interface Recipe {
  key: string;
  name: string;
  description: string;
  apps: string[];
  slots: RecipeSlot[];
  flows: { name: string; description: string }[];
}

export interface Org {
  id: string;
  name: string;
  timezone: string;
  /** Days finished runs are kept; null = the server default. */
  retention_days: number | null;
  retention_days_effective: number;
  retention_days_default: number;
}

export interface Usage {
  period: string;
  tasks_used: number;
  executions: number;
}
