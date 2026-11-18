// Typed endpoint functions: one per contract row.

import { api } from './client';
import type {
  Connection,
  ConnectionLimits,
  ConnectionTestResult,
  ConnectionUsage,
  ConnectorManifest,
  ExecutionDetail,
  ExecutionLog,
  ExecutionPage,
  ExecutionSummary,
  FieldSpec,
  Flow,
  FlowDefinition,
  FlowSummary,
  FlowVersionInfo,
  Issue,
  JsonObject,
  JsonValue,
  LimitsView,
  OptionItem,
  Org,
  PublishResult,
  Recipe,
  SaveDraftResult,
  StepTestResult,
  TriggerInfo,
  Usage,
} from './types';

const enc = encodeURIComponent;

export const endpoints = {
  // Catalog
  connectors: () => api.get<ConnectorManifest[]>('/api/v1/connectors'),
  options: (connector: string, source: string, body: { connection_id?: string | null; input: JsonObject }) =>
    api.post<{ options: OptionItem[] }>(`/api/v1/connectors/${enc(connector)}/options/${enc(source)}`, body),
  fields: (connector: string, source: string, body: { connection_id?: string | null; input: JsonObject }) =>
    api.post<{ fields: FieldSpec[] }>(`/api/v1/connectors/${enc(connector)}/fields/${enc(source)}`, body),

  // Connections
  connections: () => api.get<Connection[]>('/api/v1/connections'),
  createConnection: (body: { connector: string; name: string; fields: JsonObject }) =>
    api.post<Connection>('/api/v1/connections', body),
  updateConnection: (id: string, body: { name?: string; fields?: JsonObject }) =>
    api.patch<Connection>(`/api/v1/connections/${enc(id)}`, body),
  deleteConnection: (id: string) => api.del(`/api/v1/connections/${enc(id)}`),
  connectionLimits: (id: string) => api.get<LimitsView>(`/api/v1/connections/${enc(id)}/limits`),
  /** Replaces all overrides: `null` = the app default, `0` = no limit. */
  setConnectionLimits: (id: string, body: ConnectionLimits) => api.put<LimitsView>(`/api/v1/connections/${enc(id)}/limits`, body),
  connectionUsage: () => api.get<ConnectionUsage[]>('/api/v1/connections/usage'),
  testConnection: (id: string) => api.post<ConnectionTestResult>(`/api/v1/connections/${enc(id)}/test`),
  oauthStart: (body: { connector: string; name: string; connection_id?: string; fields?: JsonObject; return_to: string }) =>
    api.post<{ authorize_url: string }>('/api/v1/connections/oauth/start', body),
  /** The signed-in step after the provider sent the browser back (`status=finish`), in the org the sign-in began in. */
  oauthFinish: (state: string, finish: string, org?: string) =>
    api.post<{ connection_id: string }>('/api/v1/connections/oauth/finish', { state, finish }, { org }),

  // Flows
  flows: () => api.get<FlowSummary[]>('/api/v1/flows'),
  flow: (id: string) => api.get<Flow>(`/api/v1/flows/${enc(id)}`),
  createFlow: (body: { name: string; description?: string; definition?: FlowDefinition }) =>
    api.post<Flow>('/api/v1/flows', body),
  updateFlow: (id: string, body: { name?: string; description?: string }) =>
    api.patch<Flow>(`/api/v1/flows/${enc(id)}`, body),
  deleteFlow: (id: string) => api.del(`/api/v1/flows/${enc(id)}`),
  /** `org`: the org the builder was opened in (a save that finishes after an org switch still lands there). */
  saveDraft: (id: string, definition: FlowDefinition, revision: number, org?: string) =>
    api.put<SaveDraftResult>(`/api/v1/flows/${enc(id)}/draft`, { definition, revision }, { org }),
  validate: (id: string) => api.post<{ issues: Issue[] }>(`/api/v1/flows/${enc(id)}/validate`),
  publish: (id: string) => api.post<PublishResult>(`/api/v1/flows/${enc(id)}/publish`),
  enable: (id: string) => api.post<Flow>(`/api/v1/flows/${enc(id)}/enable`),
  disable: (id: string) => api.post<Flow>(`/api/v1/flows/${enc(id)}/disable`),
  versions: (id: string) => api.get<FlowVersionInfo[]>(`/api/v1/flows/${enc(id)}/versions`),
  listen: (id: string) => api.post<TriggerInfo>(`/api/v1/flows/${enc(id)}/trigger/listen`),
  testFlow: (id: string, payload: JsonValue) =>
    api.post<{ execution_id: string }>(`/api/v1/flows/${enc(id)}/test`, { payload }),
  runFlow: (id: string, payload: JsonValue) =>
    api.post<{ execution_id: string }>(`/api/v1/flows/${enc(id)}/run`, { payload }),
  testStep: (id: string, key: string, context: JsonObject) =>
    api.post<StepTestResult>(`/api/v1/flows/${enc(id)}/steps/${enc(key)}/test`, { context }),

  // Executions
  executions: (params: { flow_id?: string; status?: string; limit?: number; cursor?: string | null }) => {
    const q = new URLSearchParams();
    if (params.flow_id) q.set('flow_id', params.flow_id);
    if (params.status) q.set('status', params.status);
    q.set('limit', String(params.limit ?? 50));
    if (params.cursor) q.set('cursor', params.cursor);
    return api.get<ExecutionPage>(`/api/v1/executions?${q.toString()}`);
  },
  execution: (id: string) => api.get<ExecutionDetail>(`/api/v1/executions/${enc(id)}`),
  executionLog: (id: string) => api.get<ExecutionLog>(`/api/v1/executions/${enc(id)}/log`),
  retryExecution: (id: string) => api.post<ExecutionSummary>(`/api/v1/executions/${enc(id)}/retry`),
  rerunExecution: (id: string) => api.post<{ execution_id: string }>(`/api/v1/executions/${enc(id)}/rerun`),
  cancelExecution: (id: string) => api.post<ExecutionSummary>(`/api/v1/executions/${enc(id)}/cancel`),

  // Recipes & misc
  recipes: () => api.get<Recipe[]>('/api/v1/recipes'),
  installRecipe: (key: string, connections: Record<string, string>) =>
    api.post<{ flow_ids: string[] }>(`/api/v1/recipes/${enc(key)}/install`, { connections }),
  org: () => api.get<Org>('/api/v1/org'),
  updateOrg: (body: { retention_days: number | null }) => api.patch<Org>('/api/v1/org', body),
  usage: () => api.get<Usage>('/api/v1/usage'),
};
