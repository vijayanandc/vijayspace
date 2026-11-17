# Implementation status

What is built on the `backend` and `client` branches, how it deviates from the plan in `docs/plan/`, and what is still open. Update this file when a backlog item lands.

## Built and tested

### Flows and the engine
- Flow definitions (schema v1) with **webhook**, **schedule** (once, every N minutes, cron with a time zone), **app event** (instant or polling) and **manual** triggers.
- Steps: **action**, **decision** (ordered branches plus otherwise; nested), **delay** (duration or until a templated time), **set variable** and **stop**.
- Drafts with revision-checked autosave, validation (`/validate`), immutable published versions, enable/disable, "listen for a sample", **test run** of the draft and **test step**.
- The engine compiles a definition to a flat plan and interprets it purely. Property tests cover termination, determinism, resume equivalence and no re-run of finished steps.
- Templates use minijinja: a single `{{ … }}` keeps its JSON type, and custom filters (`translate` value maps, dates, JSON, hashing, regex, …) are available. Conditions are JSON trees with 21 operators.
- Retries: per-step automatic retry with backoff (60 s → 6 h, 9 attempts) for transient errors. Executions can be retried from the failed step, re-run, or cancelled.

### Runtime
- MySQL is the job queue: `FOR UPDATE SKIP LOCKED` claims, leases with heartbeat, a reaper for expired leases, and dead letters. Trigger event, execution and job are written in one transaction.
- Webhook ingress: an array body can start one execution per element, plus idempotency-key dedupe, an optional shared secret header and a 6 MB limit.
- App events: subscribe, renew and unsubscribe hooks; reconciliation polling; a 30 s debounce per record.
- Connections: secrets are encrypted with AES-256-GCM (AAD = org and connection, key ring with rotation). OAuth2 with PKCE, token refresh under a per-connection lock, and `needs_reauth` status.
- Outbound HTTP goes through an SSRF guard (DNS resolver check, literal-IP check, redirect re-check).
- Usage counters (tasks per month).

### Integrations (doc 13)
- Connectors: core HTTP, Formatter (11 actions), Email, **Zoho CRM**, **Appblocks** (FSM, HMS and any Appblocks app) and **Autodesk Forma**.
- **Sync layer** (`core.sync`): `upsert_linked_record` (lease, lookups through links, hash-skip of unchanged data, RecoverCreate by match field), `resolve_link`, `find_linked_record`, `save_link`, backfill, plus pickers for the builder.
- **Recipes R1–R9**, installed as drafts once every connection slot has been checked:
  - R1–R5: Zoho CRM ↔ FSM;
  - R6: HMS → CRM, consent-gated and limited to allowlisted fields;
  - R7–R9: CRM ↔ Forma.

### Web client (`client` branch)
- React 18, TypeScript and Vite.
- **Drag-and-drop builder** (React Flow): drag from the palette onto any `+`, or click to insert. It also has:
  - dynamic forms generated from connector field specs, with variable pickers;
  - a condition builder;
  - undo/redo, autosave and an issues panel;
  - test run, test step, publish and enable.
- Flows list, history with per-step input and output, connections (field-based and OAuth), and the recipe install wizard.
- Verified end to end in Chromium against the real server. The journey was: create → drag a decision → configure an action → publish → enable → webhook → history.

## Deviations from the plan
| Plan | Now | Why / follow-up |
|---|---|---|
| Logto login, org membership, RBAC (doc 13 §6) | **Not built.** Requests run as the default org, or as the org in `X-Org-ID`. | Skipped on purpose for this increment. Add the auth middleware before any deployment. |
| Flutter client (old doc 10) | React web client | A drag-and-drop node editor is much more mature on the web (React Flow). Doc 10 has been updated. |
| Compile-time checked SQL (`sqlx::query!` + `.sqlx/`) | Runtime `query_as` / `QueryBuilder`, with parameters always bound | Faster to iterate without a live database at build time. Scenario tests cover every query. Switching is a mechanical follow-up (T-gen). |
| In-memory fakes + port contract suites (doc 09) | Scenario tests against real MySQL, plus unit tests in the pure crates | Fakes and contract suites are still worth adding for faster tests. |
| `openapi/` + `fixtures/` generated contract, `just gen`, `cargo xtask` checks | Hand-written contract in `docs/api/README.md` | Backlog T01–T10. |
| Rhai custom functions (doc 08 §7) | Not built | Next increment candidate. |
| Per-connection rate limiter and budgets (doc 13 §4.3) | Not built; a 429 from an app is retried with backoff | Needed before large backfills against Zoho or Appblocks. |

## Needs checking against live accounts
- **Autodesk Forma:** request and response shapes marked `VERIFY (spike M37a)` in `crates/connectors/src/autodesk_forma/` (10 places):
  - project creation fields;
  - the project users endpoint;
  - the issue `published` flag;
  - the RFI v3 path;
  - webhook scope and region header;
  - where the project ID appears in payloads;
  - the cost event names;
  - polling filters.
- **Zoho CRM:** the notification channel payload and the renewal limits, tested with a real org.
- **Appblocks:** the webhook payload shape (`record` vs top-level fields), and how lookup fields such as `request_id` are serialised.
- **Recipes:**
  - custom fields assumed in Zoho: `FSM_Request_Number`, `Payment_Status`, `Paid_On`, `Forma_Project_ID`, `Contract_Value`, `UHID`;
  - the HMS `crm_consent` field (AB-6);
  - the FSM status values used in value maps.
