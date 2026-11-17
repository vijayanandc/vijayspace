# 06: API design

## 1. Conventions

| Topic | Rule |
|---|---|
| Base path | `/api/v1`. A breaking change means `/api/v2`; additive changes stay in v1. |
| Tenancy | **`X-Org-ID: org_…` header** on every tenant-scoped request (the Appblocks convention). The tenancy middleware checks membership and builds the `Actor`; a non-member gets **404**, so the org's existence isn't leaked. Only `/api/v1/me` and `GET/POST /api/v1/orgs` work without the header. |
| Auth | `Authorization: Bearer <Logto-issued JWT for the Weft API resource>` (15 min; doc 13 §6). Weft has no login endpoints. P2 adds API keys: `Authorization: Bearer wft_…`. |
| JSON | snake_case fields; RFC 3339 UTC timestamps; prefixed IDs (`flw_…`); absent optional fields are omitted, not `null` |
| DTOs | Separate request/response structs in `routes/<feature>/dto.rs`; never serialize domain types directly, **except `FlowDefinition`**, which is itself a versioned contract |
| Validation | 422 with field-level issues (JSON Pointer paths) |
| Concurrency | Flow drafts use `ETag: "<revision>"` and require `If-Match` on `PUT …/draft` and `POST …/publish`. A mismatch gives **412**, a missing header gives **428**. |
| Idempotency | P2: optional `Idempotency-Key` on POSTs that create things. Webhook ingress supports it from the MVP. |
| Pagination | Cursor-based: `?limit=50&cursor=…` returns `{ "items": [...], "next_cursor": "…" | null }`. The cursor is opaque base64 of `(sort_key, id)`. Default 50, max 200. |
| Filtering and sorting | Explicit query parameters per endpoint (`status`, `flow_id`, `from`, `to`, `q`); there is no generic query language. |
| Rate limits | Responses carry `RateLimit-Limit`, `RateLimit-Remaining` and `RateLimit-Reset`, plus `Retry-After` on 429. |
| Request ID | `X-Request-Id` is accepted or generated, echoed back, included in logs and in problem bodies. |
| Docs | `GET /api/openapi.json` and `GET /api/docs` (Scalar UI) |

## 2. Errors: RFC 9457 problem details

```http
HTTP/1.1 422 Unprocessable Content
Content-Type: application/problem+json

{
  "type": "https://docs.weft.dev/errors/flow.invalid",
  "title": "Flow is not valid",
  "status": 422,
  "code": "flow.invalid",
  "detail": "Fix 2 issues before publishing.",
  "request_id": "req_01JD…",
  "issues": [
    { "path": "/steps/0/input/channel", "code": "field.required", "message": "Channel is required", "severity": "error" },
    { "path": "/steps/2/input/text", "code": "mapping.unknown_step", "message": "Step 'foo' does not exist before this step", "severity": "error" }
  ]
}
```

| `ErrorKind` | HTTP | Example codes |
|---|---|---|
| Validation | 422 (400 for malformed JSON) | `validation.failed`, `flow.invalid` |
| Unauthenticated | 401 | `auth.invalid_token`, `auth.token_expired` |
| Forbidden | 403 | `auth.forbidden` |
| NotFound | 404 | `flow.not_found`, `connection.not_found` |
| Conflict | 409 | `flow.not_published`, `connection.in_use` |
| PreconditionFailed | 412 / 428 | `flow.revision_conflict`, `request.if_match_required` |
| RateLimited | 429 | `rate_limited` |
| QuotaExceeded | 402 | `usage.quota_exceeded` |
| Unavailable | 503 | `service.unavailable` |
| Internal | 500 | `internal.error`; the detail is never exposed |

The client switches on `code`, never on `title` or `detail`. The code catalog is exported to Dart (see doc 10).

## 3. Endpoint catalog

**Phase:** M = MVP, then P2, P3. Paths written `org/…` are tenant-scoped: they mean `/api/v1/…` **plus the `X-Org-ID` header**.

### 3.1 Identity, orgs and members (Logto handles sign-in; doc 13 §6)
| Method | Path | Phase | Notes |
|---|---|---|---|
| GET | `/api/v1/me` | M | Creates the user just in time from the Logto token; returns the user plus memberships `[{org, role}]`. Accepts pending invitations whose email matches a verified token email. |
| GET | `/api/v1/orgs` | M | The user's orgs (no `X-Org-ID` needed) |
| POST | `/api/v1/orgs` | M | `{name, timezone}`; the caller becomes owner |
| PATCH | `org/settings` | M | Name, time zone, `max_concurrent_executions` (owner) |
| GET | `org/members` | M | |
| PATCH, DELETE | `org/members/{user_id}` | M | Change a role or remove a member (admin); the last owner cannot be removed |
| GET, POST | `org/invitations` | M | `{email, role}`; an email with a link to the Weft app |
| DELETE | `org/invitations/{id}` | M | Revoke |

### 3.2 Folders and flows
| Method | Path | Phase | Notes |
|---|---|---|---|
| GET, POST | `org/folders` | M | The golden slice |
| PATCH, DELETE | `org/folders/{folder_id}` | M | |
| GET | `org/flows?folder_id&q&enabled&cursor&limit` | M | List items carry status, trigger kind, last run status, `has_unpublished_changes` |
| POST | `org/flows` | M | `{name, description?, folder_id?, definition?}` |
| GET | `org/flows/{flow_id}` | M | Metadata, draft and published version number; `ETag` is the draft revision |
| PATCH | `org/flows/{flow_id}` | M | Name, description, folder |
| PUT | `org/flows/{flow_id}/draft` | M | `If-Match`; the body is a `FlowDefinition`; returns the new revision and validation `issues` |
| POST | `org/flows/{flow_id}/validate` | M | Validates the draft without saving |
| POST | `org/flows/{flow_id}/publish` | M | `If-Match`; returns the version |
| POST | `org/flows/{flow_id}/enable` and `/disable` | M | Enabling requires a published version |
| DELETE | `org/flows/{flow_id}` | M | Soft delete; deactivates the trigger |
| GET | `org/flows/{flow_id}/versions` and `/versions/{n}` | M | |
| POST | `org/flows/{flow_id}/versions/{n}/restore` | P3 | Copies the version into the draft |
| GET | `org/flows/{flow_id}/variables?before_step={key}` | M | The variable tree for the picker, computed server-side from trigger and step samples and connector output schemas |
| POST | `org/flows/{flow_id}/trigger/listen` | M | Starts sample capture (5 min) |
| GET | `org/flows/{flow_id}/trigger/sample` | M | The latest captured sample |
| POST | `org/flows/{flow_id}/steps/{step_key}/test` | M | Synchronous single-step test |
| POST | `org/flows/{flow_id}/test` | M | Runs the draft with `{payload}`; returns `{execution_id}` |
| POST | `org/flows/{flow_id}/run` | M | Manual run of the published version |
| POST | `org/flows/{flow_id}/duplicate` | P2 | |
| GET | `org/flows/{flow_id}/export` | P2 | Portable JSON with connection placeholders |
| POST | `org/flows/import` | P2 | Maps placeholders to connections |

### 3.3 Connectors and connections
| Method | Path | Phase | Notes |
|---|---|---|---|
| GET | `/api/v1/connectors?q&category` | M | The catalog (key, name, icon, categories, auth kind) |
| GET | `/api/v1/connectors/{key}` | M | The full manifest: actions, triggers, field schemas |
| POST | `org/connectors/{key}/options/{source}` | M | Dynamic dropdown: `{connection_id, input}` returns `[{value, label}]` |
| GET, POST | `org/connections` | M | POST takes `{connector_key, name, credentials}` for non-OAuth connections |
| GET, PATCH, DELETE | `org/connections/{id}` | M | Secrets are never returned; DELETE returns 409 if flows use the connection, unless `?force=true` |
| POST | `org/connections/{id}/test` | M | |
| POST | `org/connections/oauth/start` | M | `{connector_key, name, connection_id?, return_to}` returns `{authorize_url}` |
| GET | `/oauth/callback` | M | Public; consumes the state and redirects to `return_to` |
| GET, POST | `org/custom-connectors` | P2 | Custom apps |

### 3.4 Executions and usage
| Method | Path | Phase | Notes |
|---|---|---|---|
| GET | `org/executions?flow_id&status&mode&from&to&cursor` | M | Org-wide history and per-flow history |
| GET | `org/executions/{id}` | M | The execution plus its step-run timeline (input/output/error) |
| POST | `org/executions/{id}/retry` | M | From the failed step, same version |
| POST | `org/executions/{id}/rerun` | M | A new execution on the latest published version |
| POST | `org/executions/{id}/cancel` | M | |
| POST | `org/executions/rerun` | P2 | Bulk: `{ids:[…≤100]}` |
| GET | `org/executions/{id}/events` | P2 | SSE live updates |
| GET | `org/usage?period=YYYY-MM` | M | |

### 3.5 Platform (P2/P3)
`org/audit-logs`, `org/api-keys`, `/api/v1/templates`, `/api/v1/templates/{key}/use`, `org/settings` (retention, security, notifications).

### 3.6 Public ingress and operations
| Method | Path | Auth | Notes |
|---|---|---|---|
| ANY | `/hooks/{token}` | None (token = capability) | See doc 04 §7.1. Separate rate limits. |
| GET | `/oauth/callback` | State parameter | Also handles Zoho's `location` / `accounts-server` parameters (doc 13 §5.2) |
| GET | `/healthz` | None | Liveness |
| GET | `/readyz` | None | DB ping and migration version check |
| GET | `/metrics` | Internal port only | Prometheus |

## 4. Contract generation and change control

```
backend branch:  Rust handlers + DTOs (#[utoipa::path], ToSchema)
                        │  just gen (cargo xtask openapi)
                        ▼
                 openapi/openapi.json  (committed; CI fails on drift)
                        │
client branch:          │  just sync-contract: copies openapi/ + fixtures/ from origin/backend,
                        │  then runs openapi-generator (dart-dio, pinned version, via Docker)
                        ▼
                 packages/weft_api  (generated, never edited, committed)
```
Client CI fails when the synced `contract/` copy differs from the latest `origin/backend`, so the client cannot silently fall behind.

- **Breaking-change gate:** CI runs `oasdiff breaking` between `backend` and the PR's `openapi.json`. A breaking change in `/api/v1` fails the build unless the PR carries a `breaking-change-approved` label applied by a human.
- **FlowDefinition contract:** its JSON Schema (`schemars`) is published at `openapi/flow-definition.schema.json`, and the shared fixtures in `fixtures/flows/` are round-tripped by both the Rust and Dart test suites (doc 09 §11).
- **Error code catalog:** `openapi/error-codes.json` is generated from `weft-domain::error_codes`, and the Dart side has an exhaustive mapping test.

## 5. Real-time updates
- **MVP:** the client polls `GET org/executions/{id}` every 2 s while a test run is active, and polls the history list every 15 s.
- **P2:** Server-Sent Events. The API instance tails a small `execution_events` table by cursor (`WHERE id > ? AND execution_id = ?`, using an `AUTO_INCREMENT` id) about once a second. This needs no extra infrastructure, and any API replica can serve any client.
