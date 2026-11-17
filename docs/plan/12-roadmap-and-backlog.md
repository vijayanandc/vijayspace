# 12: Roadmap and backlog

## 1. Phases at a glance

| Phase | Theme | Rough duration (1–2 engineers driving agents) | Exit demo |
|---|---|---|---|
| **P0** Foundations | Scaffold, guardrails, golden slice | 2 weeks | Folders CRUD works end to end (API, DB, Flutter), CI green, recipes written |
| **P1** MVP | Zoho CRM ↔ Appblocks (FSM, HMS): multi-tenant with Logto, record connectors, sync layer, recipes R1–R6 (**doc 13**) | 10–12 weeks | Doc 13 §12: Deal won → FSM request, job done → Deal updated, backfill with no duplicates, no loops, crash-safe, two isolated tenants |
| **P1.1** Autodesk | APS OAuth, BuildingConnected and ACC connectors, recipe R7 | 3–4 weeks | BuildingConnected opportunity → Zoho Deal; Deal won → ACC project |
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

### P1: MVP (re-planned)
**The Phase 1 backlog is now tasks M01–M39 in [doc 13 §11](13-mvp-zoho-crm-appblocks.md).** The generic T11–T48 tasks from the first plan are folded in, renumbered, or deferred:
- Identity (T11, T12, T14) is replaced by Logto: M01, M02, M04.
- The engine tasks (T15–T30) become M05–M16.
- The Slack and Telegram connectors (T34, T35) move to P2.
- Polling and instant trigger frameworks (from T49 and T50) move into the MVP as M20.

### P2: Connect apps
| ID | Task | Size |
|---|---|---|
| T49 | RSS connector on the polling framework (the framework itself moved to MVP M20) | S |
| T50 | GitHub connector (HMAC-verified instant trigger) on the M20 framework; Slack and Telegram connectors (from the original MVP) | M |
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
| T63 | Sharing connections and flows between members; fine-grained per-connection permissions (members and invitations moved to MVP M02) | M 🔒 |
| T64 | Audit log (write points plus UI) | M |
| T65 | API keys (`wft_…`, Appblocks-style: SHA-256 hash, scopes, expiry, rate limit) plus the public API and a docs site; Appblocks-embedded mode (SSO, org auto-link) | M 🔒 |
| T66 | Subflows: `CallSubflow` (wait or don't wait), `Return data`, recursion guard | L 🔒 |
| T67 | Templates gallery (seeded templates, "use template" wizard mapping connections) | M |
| T68 | Plans, quotas and billing (Stripe), usage dashboard, 80%/100% alerts | L |
| T69 | Distributed (cross-worker) exact rate limiting, per-flow `concurrency: 1`, weighted fair scheduling (basic per-org caps moved to MVP M15/M16) | M 🔒 |
| T70 | OpenTelemetry, dashboards, alert rules; partitioning ADR and migration; read replica for history | L 🔒 |
| T71 | Mobile app (monitoring, toggle, rerun) plus push notifications on failure | L |
| T72 | Payload masking for all flows, retention settings, org export and delete (2FA and SSO come from Logto) | M 🔒 |
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
See doc 13 §11. In short:
```
T00–T10 → M01 → M02 → M03
M05 → M06 → M09 → M10
M13 → M14 → M15 → M16 → M20 → M22 / M23 → M24 → M25 → M26 → M27 → M35
M18 → M19 → M23
```
**Parallel work:**
- Engine: M05–M10.
- Connectors: M11, M12, M17–M19.
- Flutter, against fixtures: M04, M30, M31.
- The Appblocks-side changes AB-1 to AB-9, in their own repos.

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
