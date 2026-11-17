# Weft REST API v1 (contract)

The contract between the Rust server (`backend` branch) and the React client (`client` branch).
JSON is snake_case. Timestamps are RFC 3339 UTC. IDs are prefixed strings (`flw_…`, `cxn_…`, `exe_…`).

**Tenancy:**
- Requests may send `X-Org-ID: org_…`. When it is missing, the server uses the default org (authentication is skipped for now; Logto comes later, see plan doc 13 §6).
- CORS allows the dev client origin (`http://localhost:5173`).

**Errors** are `application/problem+json`:
```json
{ "type": "about:blank", "title": "Flow is not valid", "status": 422, "code": "flow.invalid",
  "detail": "Fix 2 issues before publishing.",
  "issues": [{ "severity": "error", "path": "/steps/0/input/url", "code": "field.required", "message": "`url` is required" }] }
```
Clients switch on `code` (see `crates/domain/src/error_codes.rs`).

---

## Flow definition (the document the builder edits)
```json
{
  "schema_version": 1,
  "trigger": { "type": "webhook", "split_array": false },
  "steps": [
    { "key": "is_big", "name": "Big order?", "type": "decision",
      "branches": [ { "key": "big", "name": "Amount > 100",
                      "condition": { "all": [ { "left": "{{ trigger.body.amount }}", "op": "gt", "right": 100 } ] },
                      "steps": [ /* nested steps */ ] } ],
      "otherwise": [ /* nested steps */ ] },
    { "key": "notify", "type": "action", "connector": "core.http", "action": "request",
      "connection_id": null, "input": { "method": "POST", "url": "https://…", "body": { "email": "{{ trigger.body.email }}" } },
      "on_error": "fail_flow", "retry": { "max_attempts": 3 }, "sample_output": { } },
    { "key": "wait", "type": "delay", "seconds": 3600 },
    { "key": "wait2", "type": "delay", "until": "{{ trigger.body.due_at }}" },
    { "key": "mark", "type": "set_variable", "assignments": [ { "name": "stage", "value": "{{ steps.notify.body.stage }}" } ] },
    { "key": "halt", "type": "stop", "status": "failed", "message": "Not eligible" }
  ]
}
```

**Triggers:**
- `{"type":"webhook","split_array":bool,"idempotency_header"?:str,"secret_header"?:str}`
- `{"type":"schedule","schedule":{"kind":"once","at":"2026-12-01T09:00","timezone":"Asia/Kolkata"}}`
- `{"type":"schedule","schedule":{"kind":"interval","minutes":15}}`
- `{"type":"schedule","schedule":{"kind":"cron","expr":"0 9 * * 1-5","timezone":"UTC"}}`
- `{"type":"app","connector":"zoho_crm","trigger":"record_event","connection_id":"cxn_…","input":{…},"poll_interval_secs"?:300,"debounce_secs"?:30}`
- `{"type":"manual"}`

**Step rules:**
- Keys match `[a-z][a-z0-9_]{0,63}` and are unique in the flow. Branch keys are unique within their decision.
- `on_error` is `fail_flow` (default) or `continue`.

**Templates:** Jinja syntax inside strings.
- Scope: `trigger`, `steps.<key>` (outputs), `vars.<name>`, `flow`, `execution`.
- A string that is *exactly* one `{{ … }}` keeps its JSON type.
- Extra filters: `translate(map, default)`, `date(fmt, tz)`, `date_add(n, unit)`, `slugify`, `from_json`, `to_json`, `base64_encode`/`base64_decode`, `sha256`, `hmac_sha256(key)`, `regex_replace(p, r)`, `truncate(n)`, `pluck(key)`, `get(path)`, `digits`, plus the minijinja built-ins (`upper`, `lower`, `title`, `default`, `join`, `round`, `length`, …).

**Condition operators:** `eq neq gt gte lt lte contains not_contains starts_with ends_with in not_in is_empty is_not_empty is_true is_false matches_regex date_before date_after exists not_exists`.
- Groups: `{"all":[…]}`, `{"any":[…]}`, `{"not":{…}}`.
- A leaf may add `"case_insensitive": true`.

**Variables for pickers** (computed by the client):
- `trigger.*` comes from the trigger sample (`GET /flows/{id}` → `trigger.sample`) or the trigger spec's `output_sample`.
- `steps.<key>.*` comes from the step's `sample_output`, or else the action's `output_sample`.

---

## Catalog
| Method | Path | Body | Response |
|---|---|---|---|
| GET | `/api/v1/connectors` | — | `ConnectorManifest[]` |
| GET | `/api/v1/connectors/{key}` | — | `ConnectorManifest` |
| POST | `/api/v1/connectors/{key}/options/{source}` | `{connection_id?, input}` | `{options: [{value, label}]}` |
| POST | `/api/v1/connectors/{key}/fields/{source}` | `{connection_id?, input}` | `{fields: FieldSpec[]}` |

**ConnectorManifest:**
```json
{
  "key": "zoho_crm",
  "name": "Zoho CRM",
  "description": "…",
  "icon": "🟥",
  "category": "CRM",
  "auth": {
    "kind": "none|fields|oauth2",
    "fields": [FieldSpec],
    "oauth2": { "authorize_url": "…", "scopes": ["…"], "…": "…" }
  },
  "actions": [ { "key", "name", "description", "fields": [FieldSpec], "output_sample": {}, "idempotent": false } ],
  "triggers": [ { "key", "name", "description", "kind": "instant|polling", "fields": [FieldSpec],
                  "output_sample": {}, "setup_instructions": "…?", "supports_polling": true } ]
}
```

**FieldSpec:**
```json
{
  "key": "module",
  "label": "Module",
  "type": "string|text|number|boolean|select|datetime|json|key_value|object|code|password",
  "required": true,
  "help": "?", "placeholder": "?", "default": null,
  "options": [{"value": "Leads", "label": "Leads"}],
  "options_from": "modules?",
  "fields_from": "module_fields?",
  "depends_on": ["module"],
  "mappable": true, "multiple": false, "advanced": false, "secret": false, "read_only": false
}
```
- **`options_from`:** load the dropdown with `POST /connectors/{key}/options/{source}`, sending `input` = the current values of the step's fields. Reload when any `depends_on` field changes.
- **`type: "object"` with `fields_from`:** the value is an object. Load its sub-fields with `POST /connectors/{key}/fields/{source}` and render each sub-field as a mappable input.
- **`key_value`:** the value is an object `{"k": "v"}`. The server also accepts `[{key, value}]`.

---

## Connections
| Method | Path | Body | Response |
|---|---|---|---|
| GET | `/api/v1/connections` | — | `Connection[]` |
| POST | `/api/v1/connections` | `{connector, name, fields: {…}}` (auth kind `fields`) | `Connection` (tested on create; `status` reflects the result) |
| GET | `/api/v1/connections/{id}` | — | `Connection` |
| PATCH | `/api/v1/connections/{id}` | `{name?, fields?}` | `Connection` |
| DELETE | `/api/v1/connections/{id}` | — | 204 |
| POST | `/api/v1/connections/{id}/test` | — | `{ok: true, info: {}}` or `{ok: false, error: {code, message}}` |
| POST | `/api/v1/connections/oauth/start` | `{connector, name, connection_id?, fields?: {…non-secret config, e.g. region}, return_to}` | `{authorize_url}` |
| GET | `/oauth/callback` | (provider redirect) | `302` → `return_to?status=connected&connection_id=cxn_…` or `?status=error&message=…` |

**Connection:**
```json
{ "id": "cxn_…", "connector": "appblocks", "name": "Acme FSM", "config": { "base_url": "…", "org_id": "12" },
  "status": "active|needs_reauth|error", "created_at": "…", "updated_at": "…" }
```
Secrets are never returned.

---

## Flows
| Method | Path | Body | Response |
|---|---|---|---|
| GET | `/api/v1/flows` | — | `FlowSummary[]` |
| POST | `/api/v1/flows` | `{name, description?, definition?}` | `Flow` (201) |
| GET | `/api/v1/flows/{id}` | — | `Flow` |
| PATCH | `/api/v1/flows/{id}` | `{name?, description?}` | `Flow` |
| DELETE | `/api/v1/flows/{id}` | — | 204 |
| PUT | `/api/v1/flows/{id}/draft` | `{definition, revision}` | `{revision, issues}`; 409 `flow.revision_conflict` when `revision` is stale |
| POST | `/api/v1/flows/{id}/validate` | — | `{issues}` |
| POST | `/api/v1/flows/{id}/publish` | — | `{version: {id, version, created_at}, issues}`; 422 `flow.invalid` with `issues` |
| POST | `/api/v1/flows/{id}/enable` | — | `Flow` (409 `flow.not_published` if never published) |
| POST | `/api/v1/flows/{id}/disable` | — | `Flow` |
| GET | `/api/v1/flows/{id}/versions` | — | `[{id, version, created_at}]` |
| POST | `/api/v1/flows/{id}/trigger/listen` | — | `TriggerInfo` (captures the next webhook payload as the sample, for 5 min) |
| POST | `/api/v1/flows/{id}/test` | `{payload}` | `{execution_id}` (runs the **draft**) |
| POST | `/api/v1/flows/{id}/run` | `{payload}` | `{execution_id}` (runs the **published** version) |
| POST | `/api/v1/flows/{id}/steps/{key}/test` | `{context?: {trigger, steps, vars}}` | `{input, output?, error?}` (calls the action once; does not modify the draft) |
| POST | `/api/v1/flows/{id}/backfill` | `{limit?}` | `{enqueued}` (app triggers with polling support) |

**FlowSummary:**
```json
{ "id": "flw_…", "name": "Order alert", "description": null, "is_enabled": true, "trigger_type": "webhook",
  "published_version": 3, "has_unpublished_changes": false, "draft_revision": 12, "updated_at": "…",
  "last_execution": { "id": "exe_…", "status": "succeeded", "queued_at": "…" },
  "apps": ["zoho_crm", "core.sync", "appblocks"] }
```
`apps` lists the connector keys the draft uses (an app trigger's connector first, then actions in order, each once).

**Flow:** `FlowSummary` fields, plus these:
```json
{ "draft": FlowDefinition, "published_version_id": "fvr_…", "created_at": "…",
  "trigger": { "kind": "webhook|schedule|poll|instant|manual", "token": "…", "webhook_url": "http://host/hooks/<token>",
               "app_event_url": "http://host/hooks/app/<token>", "verify_token": "<secret for X-Weft-Key / webhook secret header>", "is_active": true,
               "listening_until": null, "next_fire_at": null, "sample": { } } }
```

---

## Executions
| Method | Path | Body | Response |
|---|---|---|---|
| GET | `/api/v1/executions?flow_id=&status=&limit=50&cursor=` | — | `{items: ExecutionSummary[], next_cursor}` |
| GET | `/api/v1/executions/{id}` | — | `ExecutionDetail` |
| POST | `/api/v1/executions/{id}/retry` | — | `ExecutionSummary` (from the failed step, same version) |
| POST | `/api/v1/executions/{id}/rerun` | — | `{execution_id}` (new execution, latest published version) |
| POST | `/api/v1/executions/{id}/cancel` | — | `ExecutionSummary` |

**ExecutionSummary:**
```json
{ "id": "exe_…", "flow_id": "flw_…", "flow_name": "…",
  "status": "queued|running|waiting|succeeded|failed|cancelled",
  "mode": "live|test|manual|rerun|backfill",
  "queued_at": "…", "started_at": "…", "finished_at": "…", "waiting_until": null, "task_count": 2,
  "error_code": null, "error_message": null, "failed_step_key": null, "possible_duplicate": false }
```

**ExecutionDetail:** `ExecutionSummary`, plus `trigger_data`, plus `steps: StepRun[]`.

**StepRun:**
```json
{ "id": "stp_…", "seq": 1, "step_key": "notify", "kind": "action|decision|delay|set_variable|stop",
  "attempt": 1, "status": "running|succeeded|failed|retry_scheduled|skipped|unknown_outcome",
  "input": {}, "output": {}, "error": { "code": "…", "message": "…" }, "warnings": [],
  "started_at": "…", "finished_at": "…" }
```
For a `decision` step run, `output` is `{"branch": "big"}`; when no branch matched it is `{"branch": null}`, meaning "otherwise" ran.

---

## Recipes (prebuilt integrations)
| Method | Path | Body | Response |
|---|---|---|---|
| GET | `/api/v1/recipes` | — | `Recipe[]` |
| POST | `/api/v1/recipes/{key}/install` | `{connections: {slot: "cxn_…"}}` | `{flow_ids: ["flw_…"]}` (flows are created as drafts) |

**Recipe:**
```json
{ "key": "r2_deal_won_to_fsm_request", "name": "…", "description": "…", "apps": ["zoho_crm", "appblocks"],
  "slots": [ { "slot": "crm", "connector": "zoho_crm", "label": "Zoho CRM connection" } ],
  "flows": [ { "name": "…", "description": "…" } ] }
```

---

## Misc
| Method | Path | Response |
|---|---|---|
| GET | `/api/v1/org` | `{id, name, timezone}` |
| GET | `/api/v1/usage` | `{period: "2026-11", tasks_used, executions}` |
| GET | `/healthz`, `/readyz` | `ok` |

## Public ingress
| Method | Path | Response |
|---|---|---|
| ANY | `/hooks/{token}` | `202 {status: "accepted"|"ignored"|"sample_captured", executions: ["exe_…"]}` |
| ANY | `/hooks/app/{token}` | App-event endpoint for instant app triggers (Zoho notifications, Appblocks workflow webhooks, Autodesk webhooks) |
