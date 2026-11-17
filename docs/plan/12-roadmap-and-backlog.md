# 12: Roadmap and backlog

## 1. Phases at a glance

| Phase | Theme | Rough duration (1–2 engineers driving agents) | Exit demo |
|---|---|---|---|
| **P0** Foundations | Scaffold, guardrails, golden slice | 2 weeks | Folders CRUD works end to end (API, DB, Flutter), CI green, recipes written |
| **P1** MVP | Webhook/schedule → logic → 5 connectors, durable engine, history | 8–10 weeks | The "Order alert" demo (doc 01 §3) with retries, delay and worker crash recovery |
| **P2** Connect apps | App triggers, declarative and custom connectors, loops, scripts | 8 weeks | "New Google Sheet row → loop line items → Airtable upsert → Slack summary"; a custom app built in the UI |
| **P3** Teams and scale | Orgs and roles, audit, public API, templates, subflows, billing, mobile | 8 weeks | A team of 3 shares connections; a subflow is reused by 2 flows; the public API creates a flow; a mobile push arrives on failure |
| **P4** Advanced | On-prem agent, AI building, MCP, approvals, environments | Ongoing | "Describe your flow" produces a valid draft; an on-prem MySQL query through the agent |

Sizes in the backlog: **S** fits one agent session (≤ ~400 changed lines), **M** takes 1–2 sessions, **L** needs to be split into several prompts using the spec's AC groups. 🔒 means human review is required (doc 03 §3.4).

---

## 2. Backlog

### P0: Foundations
| ID | Task | Depends | Size | Done when |
|---|---|---|---|---|
| T00 | Scaffold the `backend` branch: workspace `Cargo.toml` with members, shared deps and lints; empty crates with `lib.rs` and `AGENTS.md`; `rust-toolchain.toml`; `justfile`; `deploy/docker-compose.yml` (MySQL, Mailpit); `.config/nextest.toml`; `deny.toml`. **Done except `deny.toml`.** The `client` branch exists but is empty. | — | M 🔒 | `just check` passes on the empty workspace |
| T01 | `xtask`: `check-arch`, `check-file-size`, `check-sql`, `check-migrations`, `check-docs`, `map` | T00 | M 🔒 | Deliberate violations fail the checks |
| T02 | CI (GitHub Actions): stages 1 and 2 from doc 09 §17, cache, MySQL service | T00 | S | PRs show both stages |
| T03 | `weft-server` skeleton: figment settings, tracing (JSON), clap roles `api/worker/all/migrate`, `/healthz`, `/readyz`, graceful shutdown | T00 | M | `weft all` boots; readiness checks the DB |
| T04 | Errors: `AppError`, `error_codes.rs`, problem+json mapping, request-ID middleware | T03 | S | Snapshot test of a problem response |
| T05 | Domain primitives: `define_id!`, prefixed codec, `Timestamp` (µs), `Clock`/`IdGen`/`Rng` ports, testkit `ManualClock`, `SequentialIds` | T00 | S 🔒 | Property tests for the ID round-trip |
| T06 | First migrations (identity tables), sqlx setup, `.sqlx` offline, schema snapshot test | T03 | S 🔒 | `just db-reset` works; schema snapshot committed |
| T07 | OpenAPI pipeline (utoipa, Scalar), `xtask openapi`, contract files; on the `client` branch, `just sync-contract` and Dart client generation into `packages/weft_api`; drift checks on both branches | T04 | M | Changing a DTO without `just gen` fails CI |
| T08 | Flutter shell (on the `client` branch): pub workspace, app, go_router, theme tokens (`weft_ui`), env config, API provider with auth interceptor (stub) | T07 | M | Web app boots and calls `/healthz` |
| T09 | **Golden slice: Folders CRUD**. Migration, domain, port, MySQL repository, fake, **contract suite**, 4 use cases, 4 endpoints, HTTP tests (with RBAC and isolation harness skeletons), Flutter list and create screen | T05–T08 | L 🔒 | Exemplary code, reviewed line by line by a human |
| T10 | Write the recipes (doc 03 §2.2) and ADRs 001–013 *from* the golden slice | T09 | M | Recipes reference real files |

### P1: MVP
**Identity**
| ID | Task | Depends | Size |
|---|---|---|---|
| T11 | Signup, login, refresh (rotation and reuse detection), logout, `Actor` extractor; personal org created on signup | T09 | M 🔒 |
| T12 | Email verification and password reset (`user_tokens`, Mailpit tests) | T11 | S |
| T13 | Permissions matrix (domain) plus `actor.require`; `ROUTE_PERMISSIONS` table; RBAC sweep and tenant isolation sweep tests | T11 | M 🔒 |
| T14 | Flutter: auth screens, session management (web cookie refresh, native secure storage), route guards | T11, T08 | M |

**Flow model and expressions**
| ID | Task | Depends | Size |
|---|---|---|---|
| T15 | `weft-expr`: template parsing and rendering (typed passthrough, lenient/strict), sandbox limits, the MVP filter set; tables, property tests, fuzz target | T05 | L 🔒 |
| T16 | `weft-expr`: condition tree and operators with coercion rules | T15 | M |
| T17 | `FlowDefinition` v1 types, JSON Schema export, canonical JSON, `fixtures/flows/`, validation and scope analysis (`validate_for_publish`) | T16 | L 🔒 |
| T18 | `weft_flow_model` (Dart) plus shared-fixture round-trip tests | T17 | M |
| T19 | `ScheduleSpec` with `next_after` (time zones, DST, cron via croner) plus exhaustive tests | T05 | M |

**Flows**
| ID | Task | Depends | Size |
|---|---|---|---|
| T20 | Flow migrations; create, get, list, update and delete; draft save with revision and `If-Match` | T17, T13 | M |
| T21 | Publish (versions), enable and disable, versions list; trigger registration sync | T20 | M |
| T22 | `GET /variables` (variable tree from samples and output schemas) | T21, T25 | S |

**Engine and runtime**
| ID | Task | Depends | Size |
|---|---|---|---|
| T23 | `weft-engine` compiler (IR) plus plan snapshots | T17 | M 🔒 |
| T24 | `weft-engine` interpreter (`advance`, `record_result`, retry policy), the `Simulator`, invariant property tests I1–I5 | T23 | L 🔒 |
| T25 | Connector SDK: traits, manifest, field schema, `ConnectorError` with `from_http`, registry, conformance suite | T05 | M 🔒 |
| T26 | `weft-infra` HTTP sender with the SSRF resolver, plus the security test table | T25 | M 🔒 |
| T27 | Job queue (MySQL): enqueue, claim, lease, heartbeat, reap, reschedule, dead; concurrency tests | T06 | M 🔒 |
| T28 | Execution persistence: executions and step runs repositories, atomic methods, contract suites | T27 | M |
| T29 | Runner `run_slice` plus `weft-worker` loops (queues, semaphores, drain, shutdown); fault-injection points | T24, T28 | L 🔒 |
| T30 | Usage counters and quota check; failure notification job and email | T29 | S |

**Connectors and connections**
| ID | Task | Depends | Size |
|---|---|---|---|
| T31 | `core.http.request`, `core.formatter.*` (text, number, date), `core.email.send` (lettre, Mailpit) | T25, T26 | M |
| T32 | Connections: CRUD, `SecretBox` (AES-GCM with AAD, key rotation), test connection, API key/basic/bearer auth | T25 | M 🔒 |
| T33 | OAuth2 framework (start, callback, PKCE, state, refresh with row lock, `needs_reauth`) | T32 | L 🔒 |
| T34 | Slack connector (post message, list channels option source) with wiremock tests | T33 | M |
| T35 | Telegram connector (send message, bot token) | T32 | S |

**Triggers and executions**
| ID | Task | Depends | Size |
|---|---|---|---|
| T36 | Webhook ingress: content types, normalization, array split, idempotency key, limits, listening mode and sample | T21, T29 | M |
| T37 | Scheduler loop: due registrations, slot dedupe, misfire policy | T19, T29 | M |
| T38 | Manual run, test run (draft snapshot), single-step test | T29, T31 | M |
| T39 | History API: list and filter, detail with timeline; retry-from-failed, re-run, cancel; retention sweep | T29 | M |
| T40 | Scenario test suite covering the doc 09 §9 catalogue, including the **Order alert** demo | T36–T39, T34 | L |

**Flutter MVP**
| ID | Task | Depends | Size |
|---|---|---|---|
| T41 | Flows list: create, rename, delete, toggle, folders | T20, T14 | M |
| T42 | Builder canvas: spine, cards, insert, delete, move, decision branches; immutable model with undo/redo; autosave and conflict banner | T18, T41 | L |
| T43 | Dynamic form renderer (all MVP field types) plus golden tests | T25 | L |
| T44 | Mapping field with chips, variable picker, condition builder | T22, T43 | L |
| T45 | Step panel: app, action, connection, fields, test step; trigger panels (webhook listen, schedule editor) | T42–T44, T36–T38 | L |
| T46 | Connections screens: forms, OAuth web redirect, reconnect | T33 | M |
| T47 | History list and detail (timeline, JSON viewer, retry, re-run, cancel); usage widget | T39 | M |
| T48 | E2E: the `e2e/` crate plus a Flutter integration smoke test; load test baseline (`bench-worker`, k6) | T40, T47 | M |

### P2: Connect apps
| ID | Task | Size |
|---|---|---|
| T49 | Polling trigger framework (baseline, cursor, dedupe, backoff, `needs_reauth`) plus the RSS connector | L 🔒 |
| T50 | Instant trigger framework (subscribe/unsubscribe jobs, verify, parse) plus GitHub (HMAC) | L |
| T51 | Declarative YAML runtime (requests, auth injection, pagination, error mapping, option sources, polling triggers) plus conformance | L 🔒 |
| T52 | YAML connectors, one prompt each: Trello, Airtable, Notion, Stripe, Twilio, Mailchimp, HubSpot, Jira, Asana, ClickUp | S each |
| T53 | Google OAuth plus Sheets (row added trigger, add/update/find rows) and Gmail (send, new email trigger) native connectors | L |
| T54 | Fetch/search actions with `create_if_missing` | M |
| T55 | Custom Apps: `custom_connectors` storage, versioning, API, Flutter editor (form plus YAML) and test console | L |
| T56 | Loops: IR `LoopEnter`/`LoopNext`, iteration keys, limits, property tests; builder UI | L 🔒 |
| T57 | Custom functions in Rhai, split into several prompts:<br>• the `weft-script` crate (sandbox limits, helpers shared with `weft-expr`, AST cache, hostile-script tests, fuzz target)<br>• the `ScriptRunner` port, with the pooled runner in `weft-infra`<br>• the `core.script.run` action<br>• a syntax check on publish<br>• the Flutter code editor | L 🔒 |
| T58 | Flow folders, search, duplicate, export and import | M |
| T59 | Bulk re-run; per-flow retry policy settings; in-app notifications | M |
| T60 | SSE live execution updates (`execution_events` tail) | M |
| T61 | MySQL/Postgres query connectors (outbound only; SSRF policy applies) | M |
| T62 | Desktop builds (macOS, Windows, Linux) | S |

### P3: Teams and scale
| ID | Task | Size |
|---|---|---|
| T63 | Invitations, members, role management UI; sharing connections | L 🔒 |
| T64 | Audit log (write points plus UI) | M |
| T65 | API keys plus the public API (same endpoints, scoped keys), docs site | M 🔒 |
| T66 | Subflows: `CallSubflow` (wait or don't wait), `Return data`, recursion guard | L 🔒 |
| T67 | Templates gallery (seeded templates, "use template" wizard mapping connections) | M |
| T68 | Plans, quotas and billing (Stripe), usage dashboard, 80%/100% alerts | L |
| T69 | Per-org fairness and concurrency caps, per-flow `concurrency: 1`, rate limits | M 🔒 |
| T70 | OpenTelemetry, dashboards, alert rules; partitioning ADR and migration; read replica for history | L 🔒 |
| T71 | Mobile app (monitoring, toggle, rerun) plus push notifications on failure | L |
| T72 | 2FA (TOTP), OIDC SSO, session management; payload masking; retention settings; org export and delete | L 🔒 |
| T73 | Version diff and restore UI; synchronous webhook response step; error-handler paths; reusable org-level custom functions (Rhai) with a guarded `http_request` helper | M |

### P4: Advanced
| ID | Task |
|---|---|
| T74 | On-prem agent: a Rust binary that keeps an outbound WebSocket to the cloud; local DB and HTTP connectors routed through it; mTLS and enrolment tokens 🔒 |
| T75 | AI flow builder: natural language → `FlowDefinition` draft, validated by `validate_for_publish`; repair loop on validation errors; human review before publish |
| T76 | AI step connector (prompt, extract, classify, summarize) with a provider abstraction; AI explanation of failures in history |
| T77 | MCP server exposing flows as tools, and flow management as tools |
| T78 | Approvals (wait for a callback: email links and in-app approve/deny with timeout) |
| T79 | Environments (dev/prod), promotion, and environment-specific connections |
| T80 | Data stores (key-value tables for flows) |
| T81 | Email-in trigger (inbound mail) |

---

## 3. Critical path for the MVP
```
T00 → T05 → T06 → T09 (golden slice) → T11 → T13
            ↘ T15 → T16 → T17 → T23 → T24 ─┐
            T25 → T26 → T31 ───────────────┤
            T27 → T28 ─────────────────────┴→ T29 → T36/T37/T38/T39 → T40 (demo)
            T32 → T33 → T34 ───────────────────────────────────────↗
Flutter: T08 → T14 → T41 → T42 → T43 → T44 → T45 → T47 → T48
```
**Parallel work:** after T09, the engine track (T15–T24), the connector track (T25–T35) and the Flutter track (T41–T44 using fixtures) can run as **independent agent sessions** in separate branches. Crate boundaries keep their conflicts small.

---

## 4. Risk register
| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Engine correctness (lost or duplicated steps) | Medium | High | Pure interpreter, property tests I1–I5, fault injection, human review 🔒 |
| MySQL queue throughput ceiling | Low (MVP) | Medium | Deleting completed jobs, indexes, batching claims; `JobQueue` port allows a later swap |
| Builder UX complexity in Flutter | High | Medium | A vertical builder (no free-form canvas), an immutable model, golden tests, web first |
| OAuth app approvals (Google verification, Slack review) take weeks | High | Medium | Start registrations in P0; use test or dev modes until approved |
| AI-written code drifting from the patterns | High | Medium | Golden slice, recipes, xtask checks, CODEOWNERS, mutation testing |
| Connector maintenance burden (API changes) | High | Medium | Declarative YAML, nightly live smoke tests, per-connector error dashboards |
| SSRF or credential leak | Low | Critical | Doc 08 controls, security tests, CODEOWNERS on crypto and HTTP |
| Scope creep toward Zoho's 850+ apps | High | Medium | Ship a breadth of YAML connectors in P2–P3; custom apps let users fill gaps |

---

## 5. Milestone checklist per phase (the "definition of done" for a phase)
- [ ] All tasks in the phase are merged with `just check` green.
- [ ] The exit demo is recorded, and it is automated as a scenario or e2e test.
- [ ] Doc 01's parity matrix is updated (phase column, implemented checkmarks).
- [ ] Load baseline re-measured and compared with the previous phase.
- [ ] The security checklist (doc 08 §10) passes for every newly exposed surface.
- [ ] Recipes and AGENTS.md reflect any new pattern; the `docs/generated/map.md` drift check is green.
