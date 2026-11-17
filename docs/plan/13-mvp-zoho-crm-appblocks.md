# 13: The MVP: Zoho CRM ↔ Appblocks apps (FSM, HMS), then Autodesk

> **This document replaces the generic MVP in doc 01 §3 and the original Phase 1 backlog in doc 12.**
> The engine, crates, test strategy and security model in docs 02–11 still apply. This doc adds what the new focus needs: record-oriented connectors, a sync layer, Logto multi-tenancy, and the Zoho CRM, Appblocks and Autodesk connectors.

## 0. What changed and why

| Before (doc 01) | Now |
|---|---|
| A generic Zoho Flow clone with Slack and Telegram as demo apps | An **integration hub for Appblocks apps**. Its first job is replacing today's Zoho Flow setup (FSM workflow webhooks → `flow.zoho.*`, and Zoho Flow/Deluge → Appblocks API tokens) with our own multi-tenant platform. |
| Our own email and password auth | **Logto from day one**, in the same Logto tenant as Appblocks (single sign-on), with Weft's own orgs selected by `X-Org-ID`, as Appblocks does it |
| App triggers (polling and instant) in Phase 2 | **In the MVP.** Zoho CRM notifications and Appblocks workflow webhooks are the core triggers. |
| "Steps" only | Plus a **sync layer**: record links, deduplication, loop prevention, backfill. Syncing records is the main thing users of this MVP will do. |
| Per-org fairness in Phase 3 | **In the MVP.** Multi-tenant syncs and backfills are bursty. |

---

## 1. What your Appblocks repos already give us (findings)

| Capability | Where (in `vijayanandc/appblocks` unless noted) | What it means for Weft |
|---|---|---|
| Metadata APIs: `GET /api/modules`, `GET /api/fields?module_api_name=` (types, picklist `options`, `is_unique`, `is_encrypted`) | `openapi.yaml` (~:3276, :4348, `Field` :11752) | One **metadata-driven Appblocks connector** serves FSM, HMS and any future appkit app |
| Record CRUD `/api/{module}`, `POST /{m}/search` (criteria), `POST /{m}/upsert?duplicate_check_fields=…`, child records `/api/{parent}/{id}/{child}` | `openapi.yaml`, `application/entities/service.go:97` | Every record operation a sync needs already exists |
| `external_id` (unique per tenant) on FSM accounts, items, quotes and invoices | `appblocks_fsm/migrations/app/goose/007_*.sql`, 006 | Idempotent upsert keyed by the Zoho record ID. Contacts and jobs have no `external_id`; Weft's link table covers them. |
| API tokens `apb_…` (Bearer + `X-Org-ID`): per-module CRUD scopes, 300 requests/min per token, **10,000 per org per day** | `internal/secrettoken`, `application/apitokens`, `docs/api_token.md` | The Appblocks connection's credential. The daily cap is a budget Weft must respect. |
| `?trigger=` on writes (API tokens only) skips Appblocks workflows | `application/entities/triggers.go:37` | **Loop prevention** when Weft writes into Appblocks |
| Workflow webhooks on `create`, `update`, `field_update`, `delete`. The payload is an admin-written JSON template with `{field}` placeholders, including `{api_id}`. Every delivery carries `Idempotency-Key` / `X-Appblocks-Event-Id`. | `docs/workflow.md`, `docs/webhook.md`, `internal/queuehandlers/webhook.go:104` | The **instant trigger** for Appblocks. But: no HMAC, no fixed envelope, enqueued **best-effort after commit** (`application/entities/workflow_webhooks.go:36`), and workflows are admin-only, so a token cannot register them. |
| Logto: JWT checked via JWKS with issuer and audience; opaque tokens through introspection (LRU, 45 min). Orgs and membership live in Appblocks' own tenant DB, selected by `X-Org-ID`; Logto Organizations are not used. | `adapter/auth/logto/provider.go:114-155,276`, `adapter/http/middleware/tenant/middleware.go`, `adapter/sql/tenants/store.go` | Weft copies this model, and asks for **JWT** access tokens for its own API resource |
| An unfinished outbound sync engine: links, payload-hash no-op skip, `RecoverCreate`, record lease locks, deterministic idempotency keys, mapping kinds | `docs/entity_integration_architecture.md`, `domain/integrations/integrations.go`, `application/integrations/service.go` | **We reuse these ideas** in Weft's sync layer (§4) |
| MySQL queue: `READ COMMITTED` + `SKIP LOCKED` claims, idempotency unique key, dead letters, permanent errors | `adapter/sql/queue/store.go:53,144`, `internal/queue/worker.go` | Confirms our queue design; we adopt the details (§9) |
| FSM modules: accounts, contacts, properties, assets, requests, quotes (+line_items), jobs, job_visits, maintenance_plans, invoices (+lines, payments), items, tasks | `appblocks_fsm/seeds/meta.yaml` | Natural Zoho CRM pairs: Accounts, Contacts, Products, Quotes, Invoices, Tasks. **Deals have no direct equivalent**; the closest is requests. |
| HMS modules: patients, appointments, encounters, admissions, diagnostics, prescriptions, invoices/payments, insurance providers/policies/claims | `appblocks_hms/seeds/meta.yaml` | Only demographic and payer data should reach a CRM; clinical data must not (§8) |

---

## 2. MVP scope

**Personas**
- **Integration admin** at a service company running Zoho CRM plus Appblocks FSM.
- **Hospital admin** running Zoho CRM plus Appblocks HMS.
- **Your team (platform operator)**, who installs and monitors integrations for many tenants.

**In the MVP**
- Multi-tenant Weft:
  - Logto login (SSO with Appblocks).
  - Orgs, members and roles, with email invitations.
  - `X-Org-ID` tenancy.
  - Per-org concurrency caps, and per-connection rate and credit budgets.
- The engine from docs 04/09: triggers (app, webhook, schedule, manual); steps (action, decision/if-else, delay, set variable, stop); durable execution; retries; history; rerun.
- Connectors:
  - **Zoho CRM** (native, metadata-driven).
  - **Appblocks** (native, metadata-driven, one connector for FSM, HMS and any appkit app).
  - `core.http`, `core.formatter`, `core.email`.
  - **`core.sync`** (§4).
- The **sync layer**: record links, upsert-linked-record, lookups through links, echo suppression, record leases, debounce, a reconciliation poll, and **backfill** (the initial matching of existing records).
- **Recipes** R1–R6 (§3), installed through a wizard.
- A Flutter web client covering: Logto login, an org switcher, connections, recipes, the builder with a two-pane field mapper, and history.

**MVP 1.1 (right after):** Autodesk (BuildingConnected and/or Autodesk Construction Cloud), recipe R7.

**Not in the MVP:**
- True two-way sync of the *same* field with conflict resolution. In the MVP each field has one owning direction (§4.4).
- Quotes and invoices with line items (child records plus finance locks).
- Loops and custom functions (Rhai).
- Public API keys, the templates gallery, subflows, billing, mobile.

---

## 3. Recipes (the user-facing value of the MVP)

A recipe is a set of flow templates with connection placeholders, field mappings and value maps. A wizard asks for the connections and the choices, then creates and publishes the flows.

| ID | Recipe | Trigger | What it does | Release |
|---|---|---|---|---|
| **R1** | Zoho CRM Accounts & Contacts → FSM accounts & contacts | Zoho notifications (`Accounts.all`, `Contacts.all`) + hourly reconciliation | Upserts linked records. Accounts match on `external_id` = Zoho ID. Contacts match on email. `contact.account_id` is resolved through the account link. | MVP |
| **R2** | Zoho CRM Deal → **Closed Won** → FSM service request | Zoho notification `Deals.edit` with `notification_condition` on Stage | Ensures the account and contact exist (R1 links, or creates them inline), creates a `request` linked to the deal, then writes the FSM request number back to a Deal custom field (Zoho write with `trigger: []`) | MVP |
| **R3** | FSM job / job visit status → Zoho CRM Deal stage + Note | Appblocks workflow webhook (`jobs` `field_update` on `status`) | Finds the deal through links (job → request → deal), maps the stage with a value map, and adds a Note ("Job JOB-0042 completed by …") | MVP |
| **R4** | FSM invoice **Paid** → Zoho CRM Deal fields + Note | Appblocks webhook (`invoices` `field_update` on `status`) | Updates "Payment status" and "Paid on" on the deal | MVP |
| **R5** | Zoho CRM Products → FSM items | Zoho notification `Products.all` + backfill | Upserts on `external_id`. Price and tax mapping use a tax value map. | MVP |
| **R6** | HMS → Zoho CRM (needs consent, allowlisted fields only) | Appblocks webhooks on `patients` and `appointments` | A patient registered with CRM consent becomes a Contact (name, mobile, email, city, UHID). An online or phone appointment becomes a Zoho Event, or a Lead for a first visit. Insurance providers become Accounts. | MVP (HMS track) |
| **R7** | Autodesk → Zoho CRM | Autodesk Platform Services (APS) webhooks / Zoho notification | BuildingConnected `opportunity.created` / `opportunity.status.updated` create or update a Zoho Deal; a Zoho Deal Closed Won creates an Autodesk Construction Cloud (ACC) project (to be confirmed, §13) | 1.1 |
| R8 | Zoho Quotes/Invoices ↔ FSM quotes/invoices with line items | — | Child-record sync; respects FSM finance locks | P2 |

---

## 4. Architecture additions

### 4.1 Record-oriented connectors (`RecordObjects` capability)
Zoho CRM and Appblocks are both **metadata-driven record systems** with modules, typed fields, picklists, lookups, upsert and search. We model that once in `weft-connector-sdk`:

```rust
/// Optional capability for connectors whose data is "records in objects".
#[async_trait]
pub trait RecordObjects: Send + Sync {
    async fn list_objects(&self, ctx: &ActionCtx) -> Result<Vec<ObjectInfo>, ConnectorError>;              // modules
    async fn describe(&self, ctx: &ActionCtx, object: &str) -> Result<Vec<FieldSpec>, ConnectorError>;    // → our field schema
    async fn get(&self, ctx: &ActionCtx, object: &str, id: &str) -> Result<Option<Record>, ConnectorError>;
    async fn find_by(&self, ctx: &ActionCtx, object: &str, field: &str, value: &Value) -> Result<Vec<Record>, ConnectorError>;
    async fn create(&self, ctx: &ActionCtx, object: &str, values: &Map) -> Result<RecordRef, ConnectorError>;
    async fn update(&self, ctx: &ActionCtx, object: &str, id: &str, values: &Map) -> Result<RecordRef, ConnectorError>;
    async fn changed_since(&self, ctx: &ActionCtx, object: &str, cursor: Option<&Cursor>, limit: u32)
        -> Result<Page<Record>, ConnectorError>;                                                         // polling, reconciliation, backfill
}
pub struct Record { pub id: String, pub modified_at: Option<Timestamp>, pub values: Map }
```
- **Generic actions come free:** any connector that implements `RecordObjects` gets `create_record`, `update_record`, `upsert_record`, `get_record`, `search_records` and the "record changed" polling trigger. Each is a thin wrapper, so Zoho CRM and Appblocks share one tested implementation.
- **Dynamic fields:** the field schema gains `Field::dynamic_from("object_fields", depends_on: ["object"])`, so the builder renders a module's real fields, custom fields included, from `describe`. This extends dynamic options (doc 07 §2). The UI caches it per connection, with a "Refresh fields" button.
- **Type mapping table** (tested in both connectors):

  | Our type | Appblocks `data_type` | Zoho `data_type` |
  |---|---|---|
  | `string` | `text`, `url` | `text`, `email`, `phone`, `website` |
  | `text` | `long_text` | `textarea` |
  | `number` | `integer`, `decimal` | `integer`, `double`, `currency`, `percent` |
  | `boolean` | `boolean` | `boolean` |
  | `date` / `datetime` | `date` / `date_time` | `date` / `datetime` |
  | `select` | `drop_down` | `picklist` |
  | `select multiple` | `multi_select` | `multiselectpicklist` |
  | `lookup` (an ID, resolved through links) | `lookup`, `user` | `lookup`, `ownerlookup` |

  Read-only fields are shown in mappings as sources but never as targets: `formula`, `auto_number` and system fields (Appblocks); `formula`, `autonumber` and system fields (Zoho).

### 4.2 The sync layer (`core.sync` connector + `weft-app::sync`)
The engine stays control-flow only (doc 04). Sync is a set of `core.sync` actions that call the `RecordObjects` of the two connections involved. The ideas come from Appblocks' unfinished integration engine.

**Actions**
| Action | Purpose |
|---|---|
| `upsert_linked_record` | The workhorse: create or update the target record that corresponds to a source record (algorithm below) |
| `resolve_link` | Source record ID → linked target ID (for lookups such as `contact.account_id`), with an optional "create if missing" |
| `find_linked_record` | Reverse lookup (e.g. FSM job → Zoho Deal) |
| `save_link` / `delete_link` | Manual control in custom flows |

**`upsert_linked_record` algorithm** (in `weft-app::sync`, with fakes-first tests):
```text
inputs: link_set, source{connection, object, record}, target{connection, object},
        values (already mapped by templates), lookups[], match{field, value}?, write_options
 1. lease  ← sync_leases.acquire(org, key(link_set, source record), ttl 60s)
             busy → StepError::Transient("sync.busy", retry_after 10s)
 2. values ← resolve lookups through record_links (missing link → step error, or create-if-missing)
 3. h      ← sha256(canonical_json(values))
 4. link   ← record_links.find_by_source(link_set, source)
 5. if link and link.target_hash == h      → return {action: "skipped", reason: "unchanged"}   // no-op + echo suppression
 6. if link: target.update(link.target_id, values)
             NotFound → stale link: delete it, continue at 7
    else (RecoverCreate):
       candidates ← match ? target.find_by(match.field, match.value) : []
       0 → target.create(values);  1 → target.update(candidate.id, values);  >1 → fail "sync.ambiguous_match"
 7. record_links.upsert(source ↔ target, target_hash = h, written_at = now)      // unique keys forbid duplicates
 8. release lease; return {action: created|updated|skipped, target_id}
```
`write_options` default to **no echo**: Appblocks writes use `?trigger=` (workflows skipped), and Zoho writes send `"trigger": []` (Zoho workflows skipped).

### 4.3 Making syncs reliable
| Problem | Mechanism |
|---|---|
| Duplicate creates when two events for one record race | A **record lease** (`sync_leases`) plus **unique keys** on `record_links`, plus `RecoverCreate` (search before create) |
| Crash after the target create but before the link is saved | On retry, `RecoverCreate` finds the record through `match` (`external_id` / email) and links it. Recipes always define a match field. |
| Loops (A → B → A …) | Each field is written in one direction only (§4.4); writes skip the target's workflows (`?trigger=` / `trigger: []`); and a write whose hash is unchanged is skipped |
| Notification storms (Zoho sends several edits within seconds) | **Debounce** on app triggers: the dedupe key is `record_id + time bucket (30 s)`, and the execution runs at the end of the bucket. That gives one run per burst. |
| Lost events (Appblocks webhooks are best-effort after commit; Zoho channels can lapse) | A **reconciliation poll** per sync flow (default hourly) reads `changed_since(last_cursor)` and pushes records through the same flow. Hash-skip makes repeats free. |
| Existing data at install time | **Backfill**: `POST /api/v1/flows/{id}/backfill {filter}` pages through `changed_since(None)` under the connection's budget and creates one trigger event per record (dedupe key = record ID + backfill ID), with progress shown in the UI |
| API budgets (Zoho credits and concurrency; Appblocks 300/min and 10k/day) | A **per-connection limiter**: a concurrency semaphore, a token bucket, and a rolling 24 h counter in `connection_usage`. At 80% the owner is warned. At 95%, backfills and reconciliation pause first, and live events continue. A 429 or `TOO_MANY_REQUESTS` becomes `RateLimited`. |

### 4.4 Field mapping (what the mapper UI produces)
The UI is a two-pane mapper: target fields on the left, a source expression on the right. Each kind compiles to existing building blocks:

| Mapping kind | Compiles to |
|---|---|
| Direct field | `{{ trigger.record.Email }}` |
| Constant / default | `"Zoho"` / `{{ trigger.record.Phone | default("") }}` |
| Template | `{{ trigger.record.First_Name }} {{ trigger.record.Last_Name }}` |
| **Value map** (picklist translation, e.g. Deal Stage → request status) | A new pure filter: `{{ trigger.record.Stage | map({"Closed Won": "Open", "Lost": "Cancelled"}, default="New") }}` |
| **Lookup through links** (e.g. FSM `account_id` ← Zoho `Account_Name.id`) | A `lookups[]` entry on `upsert_linked_record`, resolved with IO inside the action (templates stay pure) |

**Direction ownership rule (MVP):** in a recipe, each target field is written by exactly one flow. The wizard checks the recipe's flows for a cycle in which a field written A → B is also written back B → A, and refuses it. Example: R2 writes `request_number` from FSM back into Zoho, but no flow maps that Zoho field into FSM. True two-way sync of one field, with conflict policy (last-writer-wins / CRM-wins / manual), is P2.

---

## 5. Connectors

### 5.1 Appblocks (`appblocks`, native)
- **Connection:**
  - `base_url`: e.g. the FSM or HMS host.
  - `org_id`: the Appblocks numeric org.
  - `api_token`: `apb_…`, minted by an Appblocks admin, with module scopes matching the recipe.
  - Every request sends `Authorization: Bearer` and `X-Org-ID`.
  - Test connection: `GET /api/modules`. It also shows which modules the token can see.
- **RecordObjects:**
  - `list_objects`: `/api/modules`.
  - `describe`: `/api/fields?module_api_name=`.
  - `get`: `/api/{m}/{api_id}`.
  - `find_by`: `POST /{m}/search`.
  - `create` / `update`: `POST` / `PATCH`, always with `?trigger=` for sync writes.
  - `upsert`: `POST /{m}/upsert?duplicate_check_fields=external_id` when the module has `external_id`.
  - `changed_since`: search on `modified_time > cursor`, sorted ascending (verify the sort and filter on `modified_time` early: spike M22a).
  - Extra actions: `create_child_record` (`/api/{parent}/{id}/{child}`) and `add_task`.
- **Instant trigger ("Record event"):** Appblocks workflows are admin-only, so in the MVP the **wizard generates the exact workflow configuration** for the admin to paste:
  - **URL:** `https://hooks.<weft>/hooks/{registration_token}`.
  - **Static header:** `X-Weft-Key: <secret>`. Weft verifies it in constant time.
  - **Payload template:** `{"module":"jobs","operation":"update","api_id":"{api_id}"}`.
  - On receipt, Weft deduplicates on `Idempotency-Key`, then **fetches the full record** by `api_id` (1 call). That gives consistent payloads whatever the template, avoids stale data, and works for every module.
- **Polling trigger:** "New or updated record" via `changed_since` (5 min minimum interval), for orgs that cannot set up webhooks. Reconciliation (§4.3) uses the same code.
- **Errors:**
  - `409` / `DUPLICATE_VALUE` → `InvalidInput("appblocks.duplicate")`.
  - Writes into finance-locked records (FSM: invoices at Sent and later, quotes when decided, jobs when Completed) → `InvalidInput("appblocks.record_locked")`, shown clearly and not retried.
  - `429` → `RateLimited`, honouring the `X-RateLimit-*` headers.

### 5.2 Zoho CRM (`zoho_crm`, native)
- **OAuth 2.0:**
  - Weft is registered as a *Server-based Application* in the Zoho API Console, with multi-DC enabled.
  - Authorization starts at `accounts.zoho.com`. The callback returns `location` and `accounts-server`, and the token exchange goes to that accounts server.
  - Store `api_domain` (e.g. `https://www.zohoapis.in`) and the DC on the connection. Every call uses it.
  - Access tokens last 1 hour. Refresh tokens last until revoked, and refresh uses the row lock from doc 07 §5.
  - Scopes: `ZohoCRM.modules.ALL`, `ZohoCRM.settings.READ`, `ZohoCRM.notifications.ALL`, `ZohoCRM.coql.READ`, `ZohoCRM.users.READ`.
- **RecordObjects:**
  - `list_objects`: `/crm/v8/settings/modules`.
  - `describe`: `/settings/fields?module=`.
  - `get`: `/{Module}/{id}`, or `/{Module}?ids=` in batches (one call for many notified IDs).
  - `find_by`: `/{Module}/search?criteria=` or COQL.
  - `create` / `update` / `upsert`: `/{Module}/upsert` with `duplicate_check_fields`, at most 100 records per call (1 credit per 10 records), with `"trigger": []` on sync writes.
  - `changed_since`: COQL `WHERE Modified_Time > cursor ORDER BY Modified_Time`.
- **Extra actions:** add note, add tags, create task or event, convert lead (P2), related records.
- **Instant trigger (Notifications API):**
  - **Enable:** on flow enable, `POST {api_domain}/crm/v8/actions/watch` with `{channel_id, events ["Deals.edit"], notify_url, token, channel_expiry, notification_condition?, return_affected_field_values: true}`.
  - **Expiry:** at most **one week**. The default when unset or beyond the limit is one hour.
  - **Renewal:** stored in `app_subscriptions` and renewed by a job at `expires_at − 24h`, using an update (PATCH) rather than delete-and-recreate so no changes are missed in a gap.
  - **Callbacks:** they carry **IDs only**. Weft verifies `token`, debounces, then batch-gets the records (credits!).
  - **Changed fields:** `affected_fields` lets a flow skip events whose changed fields are not mapped, without fetching the record.
  - **Disable:** unsubscribe.
  - **If renewal fails:** the connection is flagged, the reconciliation poll covers the gap, and the owner is notified.
- **Budgets:**
  - Daily credits vary by edition (Free 5,000 to Enterprise 5,000,000 a day).
  - Concurrency is 5 (Free), 10 (Standard), 15 (Professional), 20 (Enterprise) or 25 (Ultimate) per org per app, with a sub-limit of 10 for upsert, COQL and search.
  - The edition is chosen when connecting; the limiter (§4.3) uses it.
- **Errors:**
  - Per-record codes in the `data[]` of 207 responses: `DUPLICATE_DATA` / `MANDATORY_NOT_FOUND` / `INVALID_DATA` → `InvalidInput` (with the field).
  - `INVALID_TOKEN` → `AuthExpired`.
  - `TOO_MANY_REQUESTS` → `RateLimited`.
  - 5xx → `Transient`.

### 5.3 Autodesk (`autodesk_bc`, `autodesk_acc`, MVP 1.1)
- **Auth:** Autodesk Platform Services (APS) OAuth 2.0, **3-legged**. BuildingConnected webhooks require 3-legged tokens.
- **BuildingConnected:**
  - Triggers through APS webhooks (`autodesk.construction.bc`): `opportunity.created`, `opportunity.status.updated`, `bid.created`. Subscriptions are scoped by `companyId`.
  - Payloads carry IDs, so the record is fetched afterwards.
  - Requires Bid Board Pro or BuildingConnected Pro.
- **Autodesk Construction Cloud (ACC):**
  - Action: create a project (Admin API).
  - Triggers: `autodesk.construction.issues` `issue.created-1.0` / `issue.updated-1.0`.
- **Recipe R7:** BuildingConnected opportunity → Zoho Deal (pipeline sync); Zoho Deal Closed Won → ACC project.
- **Confirm the Autodesk product and direction before building (§13).**

---

## 6. Multi-tenancy and identity (Logto)

### 6.1 Model (copied from Appblocks, adapted)
- A **Weft org** is the tenant. Every table has `org_id`, every query is scoped by it (doc 08 §3), and isolation sweeps test it.
- **Login is through Logto only.** Weft stores no passwords. The users table holds `logto_subject`, email and name. A user is created **just in time** on their first request (like Appblocks' `EnsureUser`).
- **Org selection:** `X-Org-ID: org_…` on every tenant-scoped request (Appblocks' convention). `GET /api/v1/orgs` (no header) lists the user's memberships, and `POST /api/v1/orgs` creates one. The middleware checks membership and builds the `Actor` (user, org, role).
- **Membership and roles** (`owner | admin | member | viewer`, doc 08 §2) live in Weft's DB, not in Logto Organizations. That matches Appblocks and avoids two sources of truth. **Invitations** are by email, and the membership is bound on first login when the token's verified email matches (Appblocks binds the same way).
- **Weft orgs and Appblocks orgs are independent.** One Weft org can hold several Appblocks connections (an FSM org and an HMS org). P2 adds an "Appblocks-embedded" mode: auto-link an org and open Weft from the Appblocks admin with single sign-on.

### 6.2 Token validation
- Register a **"Weft API" API resource** in the Logto tenant Appblocks already uses (single sign-on across products). Clients request access tokens with `resource = https://api.<weft-domain>`, and Logto then issues **JWTs**.
- Weft validates locally: JWKS from `{endpoint}/oidc/jwks` (cached, refetched on an unknown `kid`), `iss = {endpoint}/oidc`, `aud` = the Weft resource, `exp`/`nbf` with 60 s leeway, and the algorithm allowlist ES384/ES256/RS256.
- This avoids Appblocks' per-token introspection round trip. Revocation lag is the token lifetime; set it to 15 min on the Weft API resource.
- **Email and name** come from Logto custom JWT claims on the Weft resource (`email`, `email_verified`, `name`), so no userinfo call is needed. The fallback is the Logto Management API with an M2M app.
- All of this sits behind an `IdentityVerifier` port in `weft-app`, implemented in `weft-infra::auth::logto`. Tests use an **in-process fake IdP** (its own JWKS plus a token minter), as Appblocks' e2e harness does (`e2e/finance/harness_test.go`).

### 6.3 Logto setup checklist (one-time)
1. In the existing Logto tenant, create an API resource `https://api.<weft-domain>` and set the access token TTL to 900 s.
2. Create an application "Weft Web" (single-page app) with Weft's redirect and post-logout URIs. Native apps come later.
3. Add custom JWT claims for the Weft resource: `email`, `email_verified`, `name`.
4. Optional: an M2M app for the Management API fallback.
5. Weft config: `WEFT__AUTH__LOGTO_ENDPOINT` and `WEFT__AUTH__API_RESOURCE`. Secrets go only in the environment or a secret manager (§9).

### 6.4 Tenant fairness (now in the MVP)
- Per-org concurrency caps on running executions (default 10, configurable per org). A claimed job whose org is at its cap is rescheduled +2 s.
- Per-connection limits (§4.3): one tenant's backfill cannot exhaust another tenant's capacity, and one tenant cannot exhaust its own Zoho budget unnoticed.
- Backfills and reconciliation run on a lower-priority `sync_bulk` queue, so live events always come first.

---

## 7. Data model changes (MVP; additions to doc 05)

**Replace** doc 05's `users` table and **drop** `refresh_tokens` and `user_tokens` (Logto owns credentials):

```sql
CREATE TABLE users (
  id             BINARY(16)   NOT NULL,
  logto_subject  VARCHAR(64)  NOT NULL,          -- Logto `sub`
  email          VARCHAR(320) NULL,              -- from custom JWT claims, lowercased
  display_name   VARCHAR(120) NULL,
  last_seen_at   DATETIME(6)  NULL,
  created_at     DATETIME(6)  NOT NULL,
  updated_at     DATETIME(6)  NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_users_subject (logto_subject),
  KEY ix_users_email (email)
);

CREATE TABLE invitations (
  id           BINARY(16)   NOT NULL,
  org_id       BINARY(16)   NOT NULL,
  email        VARCHAR(320) NOT NULL,
  role         VARCHAR(16)  NOT NULL,
  invited_by   BINARY(16)   NOT NULL,
  expires_at   DATETIME(6)  NOT NULL,
  accepted_at  DATETIME(6)  NULL,
  created_at   DATETIME(6)  NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_invitations_org_email (org_id, email)
);
```

**New sync and integration tables:**
```sql
CREATE TABLE record_links (
  id              BINARY(16)   NOT NULL,
  org_id          BINARY(16)   NOT NULL,
  link_set        VARCHAR(64)  NOT NULL,     -- named by the recipe, e.g. 'crm_accounts__fsm_accounts'
  a_connection_id BINARY(16)   NOT NULL,
  a_object        VARCHAR(64)  NOT NULL,     -- e.g. 'Accounts'
  a_record_id     VARCHAR(191) NOT NULL,
  b_connection_id BINARY(16)   NOT NULL,
  b_object        VARCHAR(64)  NOT NULL,     -- e.g. 'accounts'
  b_record_id     VARCHAR(191) NOT NULL,
  a_hash          BINARY(32)   NULL,         -- sha256 of values Weft last wrote to side A
  b_hash          BINARY(32)   NULL,
  a_written_at    DATETIME(6)  NULL,
  b_written_at    DATETIME(6)  NULL,
  created_at      DATETIME(6)  NOT NULL,
  updated_at      DATETIME(6)  NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_record_links_a (org_id, link_set, a_connection_id, a_object, a_record_id),
  UNIQUE KEY uq_record_links_b (org_id, link_set, b_connection_id, b_object, b_record_id)
);

CREATE TABLE sync_leases (                    -- short per-record mutex; acquire = one upsert if absent or expired
  org_id       BINARY(16)   NOT NULL,
  lease_key    BINARY(32)   NOT NULL,        -- sha256(link_set | connection | object | record_id)
  holder       VARCHAR(128) NOT NULL,        -- worker id + execution id
  locked_until DATETIME(6)  NOT NULL,
  PRIMARY KEY (org_id, lease_key)
);

CREATE TABLE app_subscriptions (              -- webhook channels Weft registered at providers
  id                BINARY(16)   NOT NULL,
  org_id            BINARY(16)   NOT NULL,
  registration_id   BINARY(16)   NOT NULL,
  connection_id     BINARY(16)   NOT NULL,
  provider          VARCHAR(32)  NOT NULL,   -- zoho_crm | autodesk_aps
  external_id       VARCHAR(191) NOT NULL,   -- Zoho channel_id / APS hook id
  events            JSON         NOT NULL,
  verify_token_hash BINARY(32)   NOT NULL,   -- token the provider echoes back
  status            VARCHAR(16)  NOT NULL,   -- active | renewing | failed | removed
  expires_at        DATETIME(6)  NULL,       -- Zoho: at most 7 days after enabling
  renew_after       DATETIME(6)  NULL,
  last_error        TEXT         NULL,
  created_at        DATETIME(6)  NOT NULL,
  updated_at        DATETIME(6)  NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_app_subscriptions_external (provider, external_id),
  KEY ix_app_subscriptions_renew (status, renew_after)
);

CREATE TABLE backfills (
  id          BINARY(16)   NOT NULL,
  org_id      BINARY(16)   NOT NULL,
  flow_id     BINARY(16)   NOT NULL,
  status      VARCHAR(16)  NOT NULL,         -- queued | running | paused | done | failed | cancelled
  filter      JSON         NULL,
  cursor      JSON         NULL,
  scanned     INT UNSIGNED NOT NULL DEFAULT 0,
  enqueued    INT UNSIGNED NOT NULL DEFAULT 0,
  failed      INT UNSIGNED NOT NULL DEFAULT 0,
  created_by  BINARY(16)   NOT NULL,
  started_at  DATETIME(6)  NULL,
  finished_at DATETIME(6)  NULL,
  created_at  DATETIME(6)  NOT NULL,
  updated_at  DATETIME(6)  NOT NULL,
  PRIMARY KEY (id),
  KEY ix_backfills_flow (org_id, flow_id, created_at)
);

CREATE TABLE connection_usage (               -- hourly buckets; rolling 24 h = sum of the last 24
  connection_id BINARY(16)   NOT NULL,
  window_start  DATETIME(6)  NOT NULL,
  calls         INT UNSIGNED NOT NULL DEFAULT 0,
  credits       INT UNSIGNED NOT NULL DEFAULT 0,   -- Zoho credit cost (e.g. upsert: 1 per 10 records)
  PRIMARY KEY (connection_id, window_start)
);
```
Also on `orgs`: `max_concurrent_executions INT UNSIGNED NOT NULL DEFAULT 10`.
Also on `connections`: `limits JSON NULL`, holding the edition, concurrency, per-minute and daily limits.

---

## 8. HMS and health data (must-haves before R6 ships)
- **Clinical modules are blocked as sync sources by default:** medical profiles, encounters, clinical notes, vitals, diagnoses, diagnostic orders and results, prescriptions, admissions, discharge summaries and documents. Unlocking one requires an org owner to acknowledge it explicitly in settings (audited).
- **R6 maps only allowlisted demographic and contact fields** (name, mobile, email, city, UHID) plus appointment metadata (time, department, type). The allowlist is part of the recipe and checked on publish.
- **Consent:** R6 syncs only patients with a CRM-consent flag. HMS has no such field today, so that is an Appblocks-side change (AB-6, §10).
- **History masking and short retention** for flows that touch HMS connections: masked fields are stored as `"••••"` in step input and output, and retention defaults to 7 days.
- **Residency:** choose the Zoho data centre that fits the hospital (e.g. the IN DC for Indian hospitals). Weft stores the DC on the connection.
- Have the processing reviewed against the health-data law that applies (e.g. India's DPDP Act, or HIPAA for US customers) before R6 goes to customers. This plan is not legal advice.

---

## 9. What we borrow from Appblocks, and what we avoid

| Adopt in Weft | Appblocks source |
|---|---|
| `X-Org-ID` tenancy middleware, just-in-time user creation, binding invitations by email | `adapter/http/middleware/tenant/middleware.go`, `adapter/sql/tenants/store.go` |
| Logto JWKS validation (issuer and audience) | `adapter/auth/logto/provider.go` |
| API token design: prefix, SHA-256 hash, display prefix, scopes, expiry, per-token rate limit and daily cap (for Weft API keys, P2) | `internal/secrettoken`, `application/apitokens` |
| Queue details: `READ COMMITTED` isolation for claims (avoids gap locks), idempotency unique key with `ON DUPLICATE KEY`, a dead-letter table, a "permanent error" type | `adapter/sql/queue/store.go`, `internal/queue/worker.go` |
| Sync ideas: links, payload-hash skip, `RecoverCreate`, leases, deterministic idempotency keys, mapping kinds | `domain/integrations/integrations.go`, `application/integrations/service.go` |
| Secret encryption: AES-GCM envelope with a key ring, plus **HKDF-derived per-tenant keys** (Weft's `SecretBox` gets per-org derivation) | `internal/fieldcrypt/service.go` |
| Auto-disabling after N consecutive delivery failures, with an admin alert (Weft applies it to failing `app_subscriptions` and auto-pauses flows, P2) | `internal/queuehandlers/webhook.go` |
| Static tenant-scope audit tests on SQL, schema drift tests, OpenAPI consistency tests, an e2e harness with a fake IdP | `adapter/sql/layouts/rules_store_test.go`, `goose_snapshot_drift_test.go`, `openapi_*_test.go`, `e2e/` |
| Design records with status and locked decisions (our specs and ADRs) | `docs/*_plan.md` |
| Tenant monitor (per-tenant health checks and daily report), P3 | `server/tenant_monitor.go` |

| Avoid | Why |
|---|---|
| Committing secrets in config files (`config.properties` holds a Logto app secret and an encryption key) | Weft reads secrets from the environment or a secret manager only; CI runs secret scanning. **Recommend rotating those Appblocks values.** |
| Enqueuing events best-effort after commit | Weft enqueues in the same transaction as the state change (doc 04 §6) |
| Opaque access tokens plus introspection on every request | Weft requires JWTs for its API resource |
| Unsigned webhooks | Weft verifies a shared secret today, and HMAC once Appblocks adds it (AB-2) |
| Hard-coded hosts and time zones in clients (`client_hms`) | The Weft client reads runtime config |

---

## 10. Proposed changes on the Appblocks side (separate backlog, in your Appblocks repos)
| ID | Change | Why | Priority |
|---|---|---|---|
| AB-1 | An **integration subscription API** usable by a scoped integration token: create or delete a "webhook subscription" (module + events → URL) without admin UI work | Weft registers and unregisters automatically on flow enable/disable (a true instant trigger, no copy-paste) | High (after MVP) |
| AB-2 | **HMAC-SHA256 signature** (`X-Appblocks-Signature: t=…,v1=…`) with a per-subscription secret | Authenticity and replay protection | High |
| AB-3 | A **standard event envelope**: `{event_id, org_id, module, operation, api_id, modified_time, changed_fields}` | No hand-written templates; Weft can skip irrelevant changes without fetching | Medium |
| AB-4 | **Transactional outbox** for workflow webhooks (enqueue inside the write transaction) | Today events can be lost on a crash; Weft's reconciliation only papers over this | Medium |
| AB-5 | `external_id` on contacts, requests, jobs and HMS patients (or a generic per-integration ID map) | Stronger `RecoverCreate` matching than email | Medium |
| AB-6 | An HMS `patients.crm_consent` field (with timestamp) | Required for R6 | High for HMS |
| AB-7 | An integration token tier with its own daily budget | 10,000 calls per org per day is tight for backfills | Medium |
| AB-8 | Decide the fate of the unwired in-core `application/integrations` engine. Recommendation: freeze it, and let Weft be the single integration layer. | One place for connectors, OAuth and monitoring | Decision |
| AB-9 | Move the secrets out of `config.properties` and rotate them | Security | **Now** |

---

## 11. MVP backlog (replaces doc 12's Phase 1 table; Phase 0 tasks T00–T10 are unchanged)

Size: **S** ≤ 1 agent session; **M** 1–2 sessions; **L** split by AC groups. 🔒 means human review.

### Identity and tenancy
| ID | Task | Depends | Size |
|---|---|---|---|
| M01 | `IdentityVerifier` port; Logto JWT verification (JWKS cache, iss/aud/exp/alg); in-process fake IdP for tests | T09 | M 🔒 |
| M02 | Orgs, org_members and invitations; `X-Org-ID` middleware → `Actor`; just-in-time users; `GET/POST /orgs`, `GET /me`, member and invite endpoints | M01 | M 🔒 |
| M03 | Permission matrix, `ROUTE_PERMISSIONS`, RBAC and tenant-isolation sweeps | M02 | M 🔒 |
| M04 | Flutter: Logto login (web), org switcher (`X-Org-ID` interceptor), runtime config | M02, T08 | M |

### Engine and runtime (as in doc 12, renumbered)
| ID | Task | Depends | Size |
|---|---|---|---|
| M05 | `weft-expr` templates, filters (including `map`), conditions | T05 | L 🔒 |
| M06 | `FlowDefinition` v1, validation, fixtures, plus the Dart model | M05 | L 🔒 |
| M07 | `ScheduleSpec` (time zones, DST) | T05 | M |
| M08 | Flows CRUD, draft and publish, versions, `/variables` | M06, M03 | M |
| M09 | Compiler (IR) with plan snapshots | M06 | M 🔒 |
| M10 | Interpreter, Simulator, property tests I1–I5 | M09 | L 🔒 |
| M11 | Connector SDK, including **`RecordObjects`, dynamic fields**, the conformance suite | T05 | M 🔒 |
| M12 | HTTP sender with SSRF guard | M11 | M 🔒 |
| M13 | Job queue: `READ COMMITTED` claims, leases, dead letters, ordering, the `sync_bulk` queue | T06 | M 🔒 |
| M14 | Execution persistence and contract suites | M13 | M |
| M15 | Runner and worker (drain, shutdown, fault injection); **per-org caps** | M10, M14 | L 🔒 |
| M16 | **Per-connection limiter** (semaphore, token bucket, `connection_usage`); usage and failure notifications | M15 | M |

### Connections and triggers
| ID | Task | Depends | Size |
|---|---|---|---|
| M17 | `core.http`, `core.formatter`, `core.email` | M11, M12 | M |
| M18 | Connections and `SecretBox` (key ring + per-org HKDF); test connection | M11 | M 🔒 |
| M19 | OAuth2 framework (PKCE, state, refresh lock) **with Zoho multi-DC hooks** (`location`, `accounts-server`, `api_domain`) | M18 | L 🔒 |
| M20 | Webhook ingress (doc 04 §7.1) plus **app-trigger framework**: instant (subscribe, renew and unsubscribe jobs; `app_subscriptions`; verification), polling (`changed_since`, baseline, cursor), **debounce** | M15 | L 🔒 |
| M21 | Schedule loop; manual and test runs; single-step test | M15, M07 | M |
| M22 | **Appblocks connector**: connection, `RecordObjects`, actions, webhook trigger with setup-instructions generator, polling trigger, error and rate mapping. **M22a spike:** verify `modified_time` filter and sort, `{api_id}` in templates, and that `?trigger=` skips workflows. | M20, M16 | L |
| M23 | **Zoho CRM connector**: OAuth, `RecordObjects` (modules, fields, COQL), actions (upsert with `trigger: []`, notes, tags, tasks), Notifications API trigger with renewal, polling fallback, credits and concurrency profile per edition | M19, M20, M16 | L |

### Sync layer and recipes
| ID | Task | Depends | Size |
|---|---|---|---|
| M24 | `record_links`, `sync_leases`, `core.sync` actions (`upsert_linked_record`, `resolve_link`, `find_linked_record`), `RecoverCreate`, hash skip; property tests ("no duplicate creates under any interleaving") | M15, M11 | L 🔒 |
| M25 | Reconciliation schedule per sync flow; backfill API, worker and progress | M24, M22, M23 | M |
| M26 | Recipe format (flows with connection placeholders, value maps, allowlists), wizard API, direction-ownership validation | M24, M08 | M |
| M27 | Recipes **R1–R5** (FSM) as fixtures, with scenario tests against wiremock Zoho and Appblocks | M26 | L |
| M28 | Recipe **R6** (HMS) plus health-data guard (blocked modules, allowlist, consent condition, masking, 7-day retention) | M26 | M 🔒 |

### History and client
| ID | Task | Depends | Size |
|---|---|---|---|
| M29 | History API (list, detail, retry, rerun, cancel), retention sweep, **sync outcome badges** (created/updated/skipped) | M15 | M |
| M30 | Flutter: flows list, builder canvas, step panel | M04, M08 | L |
| M31 | Flutter: dynamic form renderer with **dynamic fields**, plus a **two-pane field mapper** (value-map editor, lookup picker) | M30, M11 | L |
| M32 | Flutter: connections (Zoho OAuth with DC display; Appblocks token form showing which modules the token can see) | M18, M19 | M |
| M33 | Flutter: recipes wizard (choose connections → map → backfill → enable) and a backfill progress view | M26, M25 | L |
| M34 | Flutter: history list and detail with sync badges and a link viewer ("this Deal ↔ FSM request REQ-0012") | M29 | M |

### Quality and launch
| ID | Task | Depends | Size |
|---|---|---|---|
| M35 | Scenario suite for the **exit demo** (§12) plus multi-tenant isolation and fairness scenarios | M27, M28 | L |
| M36 | e2e against staging Appblocks (FSM/HMS) and a Zoho sandbox org (nightly); load baseline with backfill of 10k records | M35 | M |

### MVP 1.1: Autodesk
| ID | Task | Depends | Size |
|---|---|---|---|
| M37 | APS OAuth (3-legged) and APS webhook subscriptions (via `app_subscriptions`) | M19, M20 | M |
| M38 | `autodesk_bc` connector (opportunity and bid triggers; get opportunity) and `autodesk_acc` (create project; issue triggers) | M37 | L |
| M39 | Recipe **R7** plus scenarios | M38, M26 | M |

**Critical path:**
```
T00–T10 → M01 → M02 → M03 → M08
M05 → M06 → M09 → M10
M11 → M12, M13 → M14 → M15 → M16 → M20 → M22 / M23 → M24 → M25 → M26 → M27 → M35
M18 → M19 → M23
```
**Parallel tracks:**
- Engine: M05–M10.
- Connectors: M11, M12, M17–M19.
- Flutter, working against fixtures: M04, M30, M31.
- Appblocks-side changes AB-1 to AB-9, in their own repos.

---

## 12. Exit demo (automated as scenario tests, then run live on staging)
There are two tenants. **Acme Services** runs Zoho CRM (IN DC) and Appblocks FSM. **City Hospital** runs Zoho CRM and Appblocks HMS.
1. An Acme admin signs in with their Appblocks Logto account (single sign-on), then connects Zoho CRM (OAuth, IN data centre) and FSM (API token and org ID).
2. They install R1–R3 through the wizard. A backfill links 500 existing Zoho accounts to FSM accounts (matching on `external_id` and email) with **zero duplicates**.
3. A Deal moves to Closed Won. Within 10 s an FSM request exists, with the account and contact linked, and the request number is written back to the Deal. The follow-up Zoho notification is **skipped as unchanged**, so there is no loop.
4. An FSM job is completed, and the Deal stage and a Note are updated within 10 s.
5. The clock advances 6 days and the Zoho channel is renewed. Then the channel is forced to lapse for 2 hours, and the reconciliation poll catches every change made during the gap.
6. A worker is killed between "FSM create succeeded" and "link saved". After recovery there is **one** FSM request, and the history marks it "possible duplicate avoided (recovered by match)".
7. City Hospital:
   - A patient registered with CRM consent becomes a Zoho Contact.
   - A patient without consent is skipped and logged.
   - Clinical fields can never be mapped.
   - History shows masked values.
8. Isolation and fairness:
   - The cross-tenant sweep passes.
   - A 10k-record backfill at Acme does not delay City Hospital's live events beyond the 2 s p95 target.

---

## 13. Risks and open questions

| Risk | Mitigation |
|---|---|
| Zoho credit and concurrency exhaustion at large tenants | The per-connection limiter, batch gets for notifications, `affected_fields` filtering, budget alerts |
| Appblocks webhook loss (best-effort after commit) | Reconciliation polling now; AB-4 later |
| The 10k/day Appblocks token cap blocks backfills | Throttled backfills, a budget view in the UI, AB-7 |
| Manual Appblocks workflow setup is error-prone | A generated config with a "Send test event" check; AB-1 removes the manual step |
| PHI leaking into the CRM | §8 guards, review, CODEOWNERS on the HMS recipe and guard code |
| Autodesk scope unclear | Confirm before M37 |

**Open questions for you**
1. **Autodesk:** which product and direction? BuildingConnected bid board → Zoho Deals, Autodesk Construction Cloud (Build) projects and issues, or Fusion? And what is the business trigger, e.g. "Deal won → create the ACC project"?
2. **Zoho CRM editions and data centres** your customers use: this sets the default budgets. Do any customers use Zoho **Books** for invoices instead of CRM?
3. **Logto:** confirm Weft joins the **same Logto tenant** as Appblocks (single sign-on), using JWT for the Weft API resource.
4. **Where Weft runs:** its own domain (e.g. `integrations.appblocks.in`)? Should it be branded "Appblocks Integrations"?
5. **The in-core `application/integrations` engine (AB-8):** freeze it in favour of Weft?
6. **HMS:** add the consent field (AB-6), and confirm exactly which patient fields may go to the CRM.
7. **Client:** Flutter as planned, or React to match `client_hms` and allow embedding in the Appblocks UI? The API-first design supports either.
