# Weft REST API v1 (contract)

The contract between the Rust server (`backend` branch) and the React client (`client` branch).
JSON is snake_case. Timestamps are RFC 3339 UTC. IDs are prefixed strings (`flw_…`, `cxn_…`, `exe_…`).

**Sign-in and tenancy** (plan doc 13 §6, doc 08 §1–3):
- Every `/api/v1` request except `GET /api/v1/auth/config` (including the catalog and recipes) sends `Authorization: Bearer <Logto access token>`. Without a valid token: **401** `auth.unauthenticated` (the client gets a fresh token once, then shows sign-in). Logto unreachable: **502** `auth.unavailable`.
- Tenant requests also send `X-Org-ID: org_…` (missing: **422** `validation.failed`). An org the user does not belong to, and anything in another org, is **404** (never 403). A role that does not allow the action is **403** `auth.forbidden`. Never send an org ID in a body.
- With sign-in turned off on the server (`WEFT_AUTH=off`, development), no token is needed and requests act as an owner of the org in `X-Org-ID` or the default org.
- Webhooks (`/hooks/…`) and the OAuth callback stay public: their token or state is the credential.
- CORS allows the configured client origins (dev: `http://127.0.0.1:3000`, the Logto redirect URI's origin).

**Errors** are `application/problem+json`:
```json
{ "type": "about:blank", "title": "Flow is not valid", "status": 422, "code": "flow.invalid",
  "detail": "Fix 2 issues before publishing.",
  "issues": [{ "severity": "error", "path": "/steps/0/input/url", "code": "field.required", "message": "`url` is required" }] }
```
Clients switch on `code` (see `crates/domain/src/error_codes.rs`). Errors the framework raises before a handler runs (malformed JSON, a bad query value, an unknown route or method, a body over 6 MB) are problem+json too, with `code` `http.<status>`. **429** means try later (`invitation.rate_limited`; `webhook.rate_limited` with `Retry-After`). Every response carries `X-Request-ID` (sent back if the client sent one); a request that takes over 2 minutes ends with **503**.

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
| GET | `/api/v1/connections/{id}/limits` | — | `Limits` |
| PUT | `/api/v1/connections/{id}/limits` | `{requests_per_minute?, max_concurrent?, daily_requests?}` (absent/`null` = the app's default, `0` = no limit, at most 10,000,000) | `Limits` |
| GET | `/api/v1/connections/usage` | — | `Usage[]` (today, every connection) |
| PATCH | `/api/v1/connections/{id}` | `{name?, fields?}` | `Connection`. Members only on connections they created (403). Changing a setting needs every secret field again (422), and an OAuth connection's settings change only by reconnecting. |
| DELETE | `/api/v1/connections/{id}` | — | 204 |
| POST | `/api/v1/connections/{id}/test` | — | `{ok: true, info: {}}` or `{ok: false, error: {code, message}}` |
| POST | `/api/v1/connections/oauth/start` | `{connector, name, connection_id?, fields?: {…non-secret config, e.g. region}, return_to}` | `{authorize_url}` |
| GET | `/oauth/callback` | (provider redirect; public) | `302` → `return_to?status=finish&state=…&finish=…` (the provider's answer is kept encrypted, unused, for 10 minutes) or `?status=error&message=…`. `400` text if the state is unknown, expired or already used. |
| POST | `/api/v1/connections/oauth/finish` | `{state, finish}` (from the `status=finish` redirect) | `{connection_id}`. Signed in, in the org the sign-in started in, by the user who started it; otherwise `422 oauth.state_invalid`. The state is spent either way, so the client calls this once. |

**Limits** (every request made through a connection counts; limits are the app's defaults with the connection's overrides; a request over a limit is retried later, never dropped; background work (polls, backfills) stops at 95% of the daily budget):
```json
{ "defaults":  {"requests_per_minute": 300, "max_concurrent": 10, "daily_requests": 10000},
  "overrides": {"requests_per_minute": 60,  "max_concurrent": null, "daily_requests": null},
  "effective": {"requests_per_minute": 60,  "max_concurrent": 10, "daily_requests": 10000},
  "usage": Usage }
```
**Usage:** `{connection_id, day: "2026-11-18" (UTC), requests, refused, daily_limit|null, percent|null, level: "ok"|"warning"(≥80%)|"critical"(≥95%)}`
`refused` counts requests held back by any limit (per minute or per day) that day. Both limits are counted in the database, so they hold across every server; `max_concurrent` is split between the live servers that run jobs (`worker`/`serve`).

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
| PUT | `/api/v1/flows/{id}/draft` | `{definition, revision}` | `{revision, issues, has_unpublished_changes}`; 409 `flow.revision_conflict` when `revision` is stale |
| POST | `/api/v1/flows/{id}/validate` | — | `{issues}`. Editors only (owners, admins, members): issue messages quote the draft's templates, which viewers do not see. |
| POST | `/api/v1/flows/{id}/publish` | — | `{version: {id, version, created_at}, issues}`; 422 `flow.invalid` with `issues` |
| POST | `/api/v1/flows/{id}/enable` | — | `Flow` (409 `flow.not_published` if never published) |
| POST | `/api/v1/flows/{id}/disable` | — | `Flow` |
| GET | `/api/v1/flows/{id}/versions` | — | `[{id, version, created_at}]` |
| POST | `/api/v1/flows/{id}/trigger/listen` | — | `TriggerInfo` (captures the next webhook payload as the sample, for 5 min) |
| POST | `/api/v1/flows/{id}/test` | `{payload}` | `{execution_id}` (runs the **draft**) |
| POST | `/api/v1/flows/{id}/run` | `{payload}` | `{execution_id}` (runs the **published** version) |
| POST | `/api/v1/flows/{id}/steps/{key}/test` | `{context?: {trigger, steps, vars}}` | `{input, output?, error?: {code, message, retryable, details}}` (calls the action once; does not modify the draft; for custom functions `details` is `{line, column}`) |
| POST | `/api/v1/flows/{id}/backfill` | `{limit?, cursor?}` | `{enqueued, next_cursor}`: starts an execution per existing record (app triggers with record support), about `limit` records per call (default 500, at most 2,000). Pass `next_cursor` back to continue; `null` means done. |

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
| GET | `/api/v1/executions/{id}/log` | — | `ExecutionLog`: the run's detailed log, oldest first |
| POST | `/api/v1/executions/{id}/retry` | — | `ExecutionSummary` (from the failed step, same version). 409 `execution.not_retryable` if the run is not failed, or finished within an hour of the org's retention cutoff (its history is being removed; re-run it instead). |
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

**ExecutionLog:** `{entries: LogEntry[], truncated}`. One chronological list built by the server:
- the run: queued, started and its end (`Run succeeded in 2.4 s`, `Run failed: code: message`, `Run cancelled`, `Waiting` with `details.until`);
- every step run: an action attempt's start and end, decisions taken, variables set, delays, stops and warnings;
- every HTTP call each attempt made (at most 50 per attempt, plus a line counting the rest);
- requests held back by the connection's limits (not attempts);
- scheduled retries;
- a worker that stopped mid-step.

`truncated` is true when the run has more than 2,000 stored lines.
```json
{ "at": "2026-11-18T09:00:00.050Z", "level": "info|warn|error", "kind": "run|step|http|limit|retry|event",
  "step_key": "crm"|null, "attempt": 1|null, "message": "POST https://crm.example.com/v2/Leads → 201 in 182 ms",
  "details": null|{…} }
```
`details` depends on `kind`:
- `http`: `{method, url, status|null, error|null, duration_ms, request_bytes, response_bytes, response_excerpt|null}`.
  - `url` has no user info, and secret query values are replaced by `***` (`token`, `access_token`, `key`, `secret`, `password`, `signature`, `code`, …).
  - `response_excerpt` is the first 2 KB of the answer, only when the status is 400 or above.
  - Headers and request bodies are never logged.
- `limit`: `{reason, until}`.
- `retry`: `{code, retry_at, next_attempt}`.

Viewers get statuses, codes and timings only:
- no error messages, warnings or variable names;
- HTTP lines show the app's address (`https://host`) without path or query, and their `details` hold only `{method, url, status, duration_ms}`.

Log lines live in the run's day partition and go with it (30 days at most).

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

## Sign-in, organisations, members
Roles: `owner | admin | member | viewer`. The matrix (doc 08 §2): owners do everything; admins everything but org settings and owners; members build, run and test flows, create and use connections, and delete only flows and connections they created (`created_by`); viewers see flows and runs, with run data hidden (`payloads_hidden: true`, no trigger data, step inputs or outputs, trigger URLs or test data).

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/api/v1/auth/config` | — (public) | `{sign_in: {endpoint, app_id, resource, scopes} \| null}`. `null` = sign-in turned off. The client signs in with Logto (`app_id` is the web app; `resource` null = opaque tokens). |
| GET | `/api/v1/me` | — (no X-Org-ID) | `{user: {id, email, email_verified, name, created_at} \| null, orgs: OrgMembership[], invitations: Invitation[], staff, can_create_orgs}`. A user is created on first sign-in; `WEFT_OWNER_EMAILS` own the default org. `staff`: an owner or admin of the default org. `can_create_orgs`: staff only, unless the server sets `WEFT_ORG_CREATION=anyone`. With sign-in off it is false. |
| GET | `/api/v1/orgs` | — | `OrgMembership[]` (org settings as in `GET /org`, plus `role`) |
| POST | `/api/v1/orgs` | `{name (1–120), timezone? (IANA, default UTC)}` | 201 `OrgMembership` (the caller is its owner). 403 `auth.forbidden` unless `can_create_orgs`: staff set up each customer's org and invite its people. |
| POST | `/api/v1/me/invitations/{id}/accept` | — | `OrgMembership`. Only with a **verified** email equal to the invited address (else 404; unverified email 403). An existing membership keeps its role. |
| POST | `/api/v1/me/invitations/{id}/decline` | — | 204 |
| GET | `/api/v1/members` | — | `[{org_id, user, role, joined_at}]` (any member) |
| PATCH | `/api/v1/members/{user_id}` | `{role}` | the member. `members.manage`; only owners make or change owners (403); the last owner cannot step down (409 `member.last_owner`). |
| DELETE | `/api/v1/members/{user_id}` | — | 204. Anyone may remove themselves (leave); others need `members.manage`; owners only by owners; never the last owner (409). |
| GET | `/api/v1/invitations` | — | open invitations `[{id, org_id, org_name, email, role, invited_by, created_at, expires_at}]` (`members.manage`) |
| POST | `/api/v1/invitations` | `{email, role}` | 201 `{invitation, email_sent}`. Renews an open invitation for the same email (403 if the caller may not give that invitation's current role); 409 `member.exists` if already a member; 429 `invitation.rate_limited` after 100 invitation emails in a day for the org (300 for the person sending, across orgs), or within 5 minutes of the last email to that address. Emails a link to `WEFT_APP_URL`; valid 14 days. |
| POST | `/api/v1/invitations/{id}/resend` | — | `{invitation, email_sent}` (new expiry). 429 `invitation.rate_limited` within 5 minutes of the last email. |
| DELETE | `/api/v1/invitations/{id}` | — | 204 (it can no longer be accepted) |

## Misc
| Method | Path | Response |
|---|---|---|
| GET | `/api/v1/org` | `{id, name, timezone, role (the caller's), retention_days (the org's setting, or null), retention_days_effective, retention_days_default}` |
| PATCH | `/api/v1/org` with `{name?, timezone?, retention_days?: 1..30 \| null}` | the org (as above); owners only (403 otherwise). Finished runs older than this are deleted (every 15 minutes) with their step history. Run history is kept 30 days at most: runs queued more than 30 days ago are removed with their day, and one still unfinished then ends as failed (`execution.expired`). `null` = the server default (`WEFT_RETENTION_DAYS`, 30). |
| GET | `/api/v1/usage` | `{period: "2026-11", tasks_used, executions}` |
| GET | `/healthz`, `/readyz` | `ok` |
| GET | `/api/v1/ops/status` | Staff only (owners and admins acting in the default org; others 403): `{now, instances: [{id, kind: "api"\|"worker"\|"all", host, version, started_at, seen_at}], queues: [{queue, ready, lag_seconds}], running_jobs, dead_jobs}`. `lag_seconds` is how long the oldest due job has waited: when it keeps growing, start another `weft worker`. |

## Public ingress
| Method | Path | Response |
|---|---|---|
| ANY | `/hooks/{token}` | `202 {status: "accepted"|"ignored"|"sample_captured", executions: ["exe_…"]}`. At most 600 requests a minute per webhook URL (429 `webhook.rate_limited` with `Retry-After`). A sample is captured only from a request carrying the secret header when one is set. |
| ANY | `/hooks/app/{token}` | App-event endpoint for instant app triggers (Zoho notifications, Appblocks workflow webhooks, Autodesk webhooks) |
