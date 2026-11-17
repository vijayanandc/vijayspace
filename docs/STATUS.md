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
- MySQL is the job queue: `FOR UPDATE SKIP LOCKED` claims, leases with heartbeat, and a reaper for expired leases. A job that keeps failing, or whose worker keeps dying, becomes dead after `max_attempts` and releases its dedupe key. Only the current lease holder can complete, reschedule or fail a job. Trigger event, execution and job are written in one transaction.
- Polls: one poll job per registration at a time. A failed poll is not retried (the next interval polls again).
- Subscription renewals run on their own loop and are claimed per worker, so slow provider APIs never delay schedules or lease recovery.
- Webhook ingress: an array body can start one execution per element, plus idempotency-key dedupe, an optional shared secret header and a 6 MB limit.
- App events: subscribe, renew and unsubscribe hooks; reconciliation polling; a 30 s debounce per record. Every event version is recorded, so the reconciliation poll never re-runs a version already delivered. Within a debounce window the queued run takes the newest event (an edit followed by a delete runs once, as a delete).
- Connections: secrets are encrypted with AES-256-GCM (AAD = org and connection, key ring with rotation). OAuth2 with PKCE, token refresh under a per-connection lock, and `needs_reauth` status.
- Outbound HTTP goes through an SSRF guard: a DNS resolver check, a literal-IP check (including NAT64 and 6to4 addresses that embed IPv4), and a redirect re-check. Environment proxies (`HTTP(S)_PROXY`) are ignored on purpose, because a proxy would resolve hosts outside the guard. An egress proxy can be set explicitly with `WEFT_HTTP_PROXY`; the proxy then resolves hostnames, so it must enforce the destination policy itself.
- OAuth: `oauth/start` keeps only the app's declared settings, and the Zoho accounts server is limited to Zoho's data centres. Both protect the token request, which carries the client secret.
- Usage counters (tasks per month).

### Integrations (doc 13)
- Connectors: core HTTP, Formatter (11 actions), Email, **Zoho CRM**, **Appblocks** (FSM, HMS and any Appblocks app) and **Autodesk Forma**.
- **Sync layer** (`core.sync`): `upsert_linked_record` (lease, lookups through links, hash-skip of unchanged data, RecoverCreate by match field), `resolve_link`, `find_linked_record`, `save_link`, backfill, plus pickers for the builder. If a sync step is re-pointed at a different target connection or module, its existing links are ignored, so without a match field the records are created again.
- **Recipes R1–R9**, installed as drafts once every connection slot has been checked:
  - R1–R5: Zoho CRM ↔ FSM;
  - R6: HMS → CRM, consent-gated and limited to allowlisted fields;
  - R7–R9: CRM ↔ Forma.

### Web client (`client` branch)
- React 18, TypeScript and Vite, styled after enterprise integration tools (Zoho Flow, Lightning, Fluent / Power Automate) with Weft's own branding (doc 10 §3).
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
| Retention sweeps (doc 05 §4, M29) | Not built: `trigger_events`, `executions`, `step_runs`, dead `jobs` and test snapshots grow without limit | Add the sweep before production volumes. |
| Backfill as a background job | One call starts about `limit` records (at most 2,000) inside the HTTP request and returns `next_cursor` to continue. The cursor pages by offset (Appblocks) or page number (Zoho), so records sharing a modified time are not skipped or stuck. | Move to a `sync_bulk` job with progress. |
| OAuth refresh across processes | Refreshes are serialised per process. A forced refresh is skipped if the token already changed. Separate `weft api` and `weft worker` processes can still race on single-use refresh tokens. | Use a database lock (`GET_LOCK`) when running split processes. `weft serve` (one process) is not affected. |
| Files ≤ 400 lines (AGENTS.md) | All files comply except `crates/engine/src/interpreter.rs` (706 lines, about 160 of them tests) | The engine is 🔒: splitting it needs human review. Moving its tests out alone would still leave ~540 lines. |

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
