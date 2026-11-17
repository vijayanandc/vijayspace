// Flow definition types (the document the builder edits). Mirrors the backend contract.

export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
export type JsonObject = { [key: string]: JsonValue };

export const OPS = [
  'eq', 'neq', 'gt', 'gte', 'lt', 'lte', 'contains', 'not_contains', 'starts_with', 'ends_with',
  'in', 'not_in', 'is_empty', 'is_not_empty', 'is_true', 'is_false', 'matches_regex',
  'date_before', 'date_after', 'exists', 'not_exists',
] as const;
export type Op = (typeof OPS)[number];

export interface Comparison {
  left: JsonValue;
  op: Op;
  right?: JsonValue;
  case_insensitive?: boolean;
}

export type Condition = { all: Condition[] } | { any: Condition[] } | { not: Condition } | Comparison;

export type ScheduleSpec =
  | { kind: 'once'; at: string; timezone?: string }
  | { kind: 'interval'; minutes: number }
  | { kind: 'cron'; expr: string; timezone?: string };

export interface WebhookTriggerNode {
  type: 'webhook';
  split_array?: boolean;
  idempotency_header?: string;
  secret_header?: string;
}
export interface ScheduleTriggerNode {
  type: 'schedule';
  schedule: ScheduleSpec;
}
export interface AppTriggerNode {
  type: 'app';
  connector: string;
  trigger: string;
  connection_id?: string | null;
  input: JsonObject;
  poll_interval_secs?: number;
  debounce_secs?: number;
}
export interface ManualTriggerNode {
  type: 'manual';
}
export type TriggerNode = WebhookTriggerNode | ScheduleTriggerNode | AppTriggerNode | ManualTriggerNode;
export type TriggerType = TriggerNode['type'];

export type OnError = 'fail_flow' | 'continue';

interface StepBase {
  key: string;
  name?: string;
  on_error?: OnError;
  retry?: { max_attempts: number };
  sample_output?: JsonValue;
}
export interface ActionStep extends StepBase {
  type: 'action';
  connector: string;
  action: string;
  connection_id?: string | null;
  input: JsonObject;
}
export interface Branch {
  key: string;
  name?: string;
  condition: Condition;
  steps: Step[];
}
export interface DecisionStep extends StepBase {
  type: 'decision';
  branches: Branch[];
  otherwise: Step[];
}
export interface DelayStep extends StepBase {
  type: 'delay';
  seconds?: number;
  until?: string;
}
export interface Assignment {
  name: string;
  value: JsonValue;
}
export interface SetVariableStep extends StepBase {
  type: 'set_variable';
  assignments: Assignment[];
}
export interface StopStep extends StepBase {
  type: 'stop';
  status?: 'succeeded' | 'failed';
  message?: string;
}
export type Step = ActionStep | DecisionStep | DelayStep | SetVariableStep | StopStep;
export type StepType = Step['type'];

export interface FlowDefinition {
  schema_version: number;
  trigger: TriggerNode;
  steps: Step[];
}
