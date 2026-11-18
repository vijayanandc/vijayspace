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
- **Custom functions** (`core.script`, Rhai): each script runs in a child process of the `weft` binary under an address-space cap (`WEFT_SCRIPT_MEMORY_MB`, 512, plus the 64 MB script stack) and a CPU cap, so a script that defeats the engine's size accounting only kills itself. Inside, the engine has an operation budget, depth and size limits, no IO, clock, modules, `eval` or `Fn`/`curry`. `input` and `context` are ordinary variables (a script may change its copy); a `fn` sees only its parameters, and closures can read variables. A panic inside the engine (Rhai has a few, e.g. changing a field of a `const` map) is caught and reported as `script.error`; a child killed by the OS caps is `script.limit_exceeded`, and one stopped by a shutdown signal is retried. Scripts run on a 64 MB stack, and a result may be nested at most 64 levels deep (it is stored as JSON). With `WEFT_SCRIPT_ISOLATION=thread` there is no OS cap, values captured by closures are not counted by the engine's size limits, and a caught engine panic is still printed to the server's stderr, so use thread mode only for trusted scripts. Syntax errors block publishing with line and column; `print` lines are kept in the step output as `_logs`; failures are `script.error` / `script.limit_exceeded` and never retried. Scripts see `input` and `context.now` (the step's start time). Code cannot contain `{{` or `{%` (template markers in step inputs).
- **Per-connection limits:** concurrency, requests per minute and a daily budget on every request made through a connection. Apps declare defaults (Appblocks 300/min and 10,000/day; Zoho and Forma defaults are conservative and marked VERIFY), and each connection can override them. A request held back by a limit is not sent and is not an attempt: it leaves no history row, the step waits (the limiter's hint plus a per-run spread of at most 10%, capped at 5 minutes) and runs when allowed (retry limits and "continue on error" do not apply). A request waits at most 2 s for a concurrency slot; inbound event fetches then go ahead without one. Background work stops at 95% of the daily budget; inbound event handling is counted but never refused (it would lose the event). Usage warns at 80%.
- **Retention:** every 15 minutes a sweep deletes finished runs older than the org's retention (default `WEFT_RETENTION_DAYS`, 30) in small batches (step runs in chunks of 500), with their trigger events and test snapshots; one org's failure does not stop the others. Dead jobs, expired OAuth states and leases, and usage counters older than 90 days are removed too. A run within an hour of the cutoff can be re-run but not retried (a retry resumes from its step history).

### Sign-in and tenancy (doc 13 §6, doc 08 §1–3)
- **Logto sign-in** with the tenant the Appblocks apps use (single sign-on). The web client uses the HMS client's single-page app (redirect `http://127.0.0.1:3000/callback` locally); the server uses the Appblocks servers' confidential app to check tokens. Opaque tokens are introspected and resolved with userinfo; JWTs for a configured resource are verified locally. Only the web app's tokens are accepted; results are cached by token hash (5 minutes, never past expiry). Logto unreachable is 502 `auth.unavailable`, never "signed out".
- Users are created on first sign-in. A `WEFT_OWNER_EMAILS` address (verified) claims the default org while it has no owner, so existing data gets one; after that its owners manage it like any org. `WEFT_AUTH` must be set explicitly, and `off` is refused unless the server binds to loopback. Anyone can create orgs (they own them) and switch between theirs; `X-Org-ID` picks the org, membership is checked on every request, and other orgs are 404.
- **Roles** owner, admin, member, viewer, with the doc 08 matrix as a pure function (every cell tested) and an API-level role test. Members delete, edit, reconnect and set limits only on connections they created, and delete only their own flows. Changing a connection's settings needs its credentials again (an OAuth connection signs in again), so a changed base URL never receives stored secrets. Viewers see runs without data or error text, and flows without trigger URLs, test data or action input values. The last owner cannot leave or step down (row-locked). The catalog and recipes need sign-in too.
- **Invitations** by email (ZeptoMail, as the Appblocks servers send it, or SMTP), accepted by signing in with the verified invited address; resend (5-minute cooldown), revoke, decline. An org sends at most 100 invitation emails a day; only someone allowed to give an invitation's role may renew it.
- The web client: sign-in page, onboarding (create an org, accept invitations), org switcher, profile menu, Settings → Users, role-aware buttons, a view-only builder for viewers, and a SHA-256 fallback so sign-in also works on plain-HTTP origins (browsers withhold `crypto.subtle` there). Verified end to end against the real tenant with two test users: sign in, invite, sign out, accept, create an own org, isolation.

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
| JWT access tokens only, for a "Weft API" resource (doc 08 §1) | Opaque tokens are also accepted, introspected with the server's confidential app (as Appblocks does) | The tenant has no Weft API resource yet. Register one and set `WEFT_LOGTO_RESOURCE`: the client then gets JWTs, checked locally without a round trip. |
| `connection.use`: "own and shared" connections (doc 08 §2) | Members use every connection of the org; edit and test any; delete only their own | There is no connection sharing model yet. |
| Roles cached 60 s (doc 08 §1) | Read from the database on every request | Simpler and removal takes effect at once; cache if it shows up in profiles. |
| OAuth sign-ins for connections bound to the user who started them | The callback is tied to the one-time `state` only (10 minutes), and `return_to` must be a page of a configured client origin | Someone could send a colleague in another org their authorize link; the consent would land in the sender's org. Finish the callback through a signed-in request that checks `created_by`. |
| Sign-in only on secure origins (HTTPS, localhost) | Plain-HTTP origins work too: the client supplies SHA-256 for PKCE and checks ID tokens by issuer, audience and times instead of their signature (browsers withhold Web Crypto there) | **🔒 Needs human review.** OpenID Connect Core §3.1.3.7 allows TLS server validation in place of the signature for tokens from the token endpoint, and the server never trusts ID tokens (it checks every access token with Logto). The redirect URI of each such origin must be registered on the Logto app. |
| Trusting Logto's `email_verified` | Invitations and `WEFT_OWNER_EMAILS` bind to the email Logto marks verified | The tenant is shared with the Appblocks apps: only enable sign-in methods there that really verify email addresses. |
| Invitation links with tokens | No token in the link: the invitation binds to the signed-in user's verified email (doc 13 §6) | Accepting needs a verified email at Logto. |
| Flutter client (old doc 10) | React web client | A drag-and-drop node editor is much more mature on the web (React Flow). Doc 10 has been updated. |
| Compile-time checked SQL (`sqlx::query!` + `.sqlx/`) | Runtime `query_as` / `QueryBuilder`, with parameters always bound | Faster to iterate without a live database at build time. Scenario tests cover every query. Switching is a mechanical follow-up (T-gen). |
| In-memory fakes + port contract suites (doc 09) | Scenario tests against real MySQL, plus unit tests in the pure crates | Fakes and contract suites are still worth adding for faster tests. |
| `openapi/` + `fixtures/` generated contract, `just gen`, `cargo xtask` checks | Hand-written contract in `docs/api/README.md` | Backlog T01–T10. |
| Backfill as a background job | One call starts about `limit` records (at most 2,000) inside the HTTP request and returns `next_cursor` to continue. The cursor restarts at the last modified time after every page and uses the offset (Appblocks) or page number (Zoho) only within records sharing one time, so ties are not stuck and edits between calls do not skip records. | Move to a `sync_bulk` job with progress. |
| OAuth refresh across processes | Refreshes are serialised per process. A forced refresh is skipped if the token already changed. Separate `weft api` and `weft worker` processes can still race on single-use refresh tokens. | Use a database lock (`GET_LOCK`) when running split processes. `weft serve` (one process) is not affected. |
| Per-connection limits across processes | The daily budget is shared (database). The per-minute bucket and the concurrency cap are per process, and a limit change reaches other processes within 60 s. | Run one worker per app budget, or move the bucket to the database if several workers share a connection. |
| Script engine reuse (doc 08 §7) | A fresh engine and compile per run; no shared packages or AST cache | A performance optimisation only; scripts are small. |
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
