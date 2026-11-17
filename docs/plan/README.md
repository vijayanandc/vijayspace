# Weft: master plan for a Zoho Flow–style integration platform

> **Codename:** `weft`. The weft is the thread that weaves across the warp, which is what this product does with apps.
> It is a placeholder. Rename it with one search-and-replace before you scaffold (`weft`, `Weft`, `WEFT_`).

| | |
|---|---|
| Backend | Rust (stable, edition 2024), axum, sqlx, tokio |
| Database | MySQL 8.4 LTS. It is the **only** stateful dependency and also serves as the job queue. |
| Client | Flutter: web first, then desktop and mobile |
| Who writes the code | Mostly AI coding agents, reviewed by a small human team |
| Plan version | v1 (2026-11-17) |

> Nothing in this plan depends on the current contents of this repository; the repository only carries the plan.
> When you create the real repo, copy `docs/plan/` into it and copy `docs/plan/templates/AGENTS.md` to the repo root.

---

## Reading order

| # | Document | What it covers |
|---|---|---|
| 01 | [Product scope](01-product-scope.md) | What Zoho Flow does, which features we clone and in what order, the MVP |
| 02 | [Architecture](02-architecture.md) | System shape, Cargo workspace, crate boundaries, canonical code shapes |
| 03 | [AI-friendly engineering](03-ai-friendly-engineering.md) | How to keep AI-written code from rotting: docs, recipes, guardrails, prompts |
| 04 | [Workflow engine](04-workflow-engine.md) | Flow definition, compiler and interpreter, triggers, job queue, retries, guarantees |
| 05 | [Data model](05-data-model.md) | MySQL conventions and DDL |
| 06 | [API design](06-api-design.md) | REST conventions, errors, endpoint catalog, client generation |
| 07 | [Connectors](07-connectors.md) | Connector SDK, declarative (YAML) connectors, OAuth, custom apps |
| 08 | [Security](08-security.md) | Tenant isolation, secrets, SSRF, sandboxing, RBAC |
| 09 | [Rust test strategy](09-rust-test-strategy.md) | The full testing plan for the backend |
| 10 | [Flutter client](10-flutter-client.md) | Client architecture, builder UX, client testing |
| 11 | [Operations](11-operations.md) | Deployment, observability, scaling, SLOs |
| 12 | [Roadmap and backlog](12-roadmap-and-backlog.md) | Phases, exit criteria, about 80 ordered tasks sized for AI sessions |
| — | [templates/AGENTS.md](templates/AGENTS.md) | Root instructions for AI agents, ready to drop into the repo |

---

## Repository layout: two branches in one repository

| Branch | Contains | Root |
|---|---|---|
| `backend` | The Rust workspace, migrations, the generated contract files (`openapi/`, `fixtures/flows/`), and these docs | A Cargo workspace (see doc 02 §3) |
| `client` | The Flutter app and its packages | A Dart pub workspace (see doc 10 §2) |

The two branches share no history. Feature work branches off one of them and merges back into it.
- **The contract flows one way, from backend to client.** `just gen` on `backend` writes `openapi/openapi.json`, `openapi/flow-definition.schema.json`, `openapi/error-codes.json` and `fixtures/flows/`.
- `just sync-contract` on `client` copies those files from `origin/backend` and regenerates the Dart API client.
- **An API change is two PRs:** backend first, then client.
- **The cost of this split:** a single commit can no longer change the server and the client together. Client CI compensates by failing when its synced contract is behind `backend`.

---

## Executive summary

1. **Product.** This is an iPaaS. Users build *flows*: one *trigger* (webhook, schedule, app event or manual) followed by *steps*. Steps are app actions, decision/if-else, delay, set variable and, later, loops, custom functions and subflows. Every run is recorded step by step with its inputs and outputs. Transient failures retry automatically, and any failed run can be re-run by hand.
2. **Shape.** A **modular monolith**: one Rust binary that runs in an `api` role and a `worker` role, both sharing one MySQL database. There is no Redis or Kafka. MySQL 8's `FOR UPDATE SKIP LOCKED` provides the job queue, so creating an execution and enqueuing it happen **in the same transaction**. A run can never be lost or created twice.
3. **Engine.** Flow definitions are versioned JSON documents and become immutable once published. Each one compiles to a small instruction set. A **pure** interpreter (no IO, no async) decides what happens next; a separate runner carries that out and saves a checkpoint after every step. After a crash, a run resumes from its last checkpoint. Because the most critical code is pure, it can be property-tested.
4. **Extensibility.** The engine only knows control flow. *Everything else* (HTTP, email, the formatter, Slack, Google Sheets) is a **connector action**, implemented either behind one Rust trait or in one YAML file. Adding features almost never touches the engine, which is the most valuable property an AI-built codebase can have.
5. **AI-friendliness** comes from several things together:
   - crate boundaries that the compiler enforces
   - vertical feature slices with one use case per file
   - uniform code shapes copied from a hand-polished "golden slice"
   - SQL checked at compile time
   - a generated API client that is never edited by hand
   - snapshot tests that make every contract change show up in the diff
   - one verification command, `just check`
   - a written recipe for every common kind of change
6. **Testing** (see doc 09) layers several kinds of test:
   - pure unit tests and property tests for the engine, expressions and domain
   - use-case tests on in-memory fakes, with shared contract tests that **prove** each fake behaves like its MySQL version
   - real-MySQL tests via `sqlx::test`
   - HTTP tests with JSON snapshots
   - connector tests against `wiremock`
   - deterministic end-to-end runs driven by a manual clock and a `drain()` call instead of sleeps
   - fuzzing, and mutation testing to catch weak AI-written tests
7. **The MVP (Phase 1)** includes:
   - webhook, schedule and manual triggers
   - HTTP, email, formatter, Slack and Telegram actions
   - decision, if-else, delay and set-variable steps
   - encrypted connections, including OAuth2
   - versioned flows
   - durable execution with automatic retry
   - history with each step's inputs and outputs, plus retry and re-run
   - a Flutter flow builder

   Phases 2–4 add app triggers, declarative and user-defined connectors, loops, scripts, teams, templates, subflows, a public API, an on-prem agent and AI-assisted flow building.

---

## The ten decisions that matter most

| # | Decision | Why |
|---|---|---|
| 1 | **Modular monolith** in one Cargo workspace (no microservices) | One deploy. The compiler enforces modularity through crate boundaries, so refactors are safe. |
| 2 | **MySQL is the only infrastructure**, including the job queue (`SKIP LOCKED`) | Enqueue happens inside the transaction, with fewer moving parts. The queue sits behind a `JobQueue` port, so it can be swapped later. |
| 3 | **Hexagonal design with a functional core**: the `domain`, `expr` and `engine` crates are pure, and IO lives only in adapters | Most logic can be tested without a database or network. |
| 4 | **Vertical slices, one use case per file, one uniform shape** | Agents find and copy patterns easily, and diffs stay small and local. |
| 5 | **sqlx with compile-time-checked SQL**, no ORM | Models write SQL well, and the compiler catches column typos and type mismatches. |
| 6 | **Popular, "boring" crates** (tokio, axum, serde, sqlx, reqwest, tracing, thiserror) | Models have seen these the most, so fewer APIs get hallucinated. |
| 7 | The **flow definition is a versioned JSON document with a JSON Schema**. Published versions are immutable, and every execution is pinned to one version. | Edits are safe, re-runs are exact, and AI can generate flows later. |
| 8 | **Everything is a connector**: the engine only knows control flow | New features never destabilise the engine. |
| 9 | **Contract-first API**: OpenAPI is generated from the Rust code (utoipa), the Dart client is generated from that on the `client` branch, and CI checks for drift | Server and client cannot drift apart silently. |
| 10 | **One gate**: `just check` runs exactly what CI runs, and no agent task is finished until it passes | The definition of "done" is mechanical. |

---

## Assumptions I made (if any is wrong, parts of the plan change)

- The product is a **multi-tenant SaaS** with organisations, and it should also be **self-hostable** as one binary plus MySQL.
- The **web client is the main place to build flows**. Mobile is mainly for monitoring, toggling flows and handling failures.
- One region. The UI is English first but ready for i18n.
- A team of 1–3 humans reviews and steers, and AI agents write most of the code.
- By the end of Phase 3, scale is up to ~1k orgs and ~1M executions/day, which fits on one well-sized MySQL primary.
- Custom functions use **Rhai**, a sandboxed scripting language written in Rust with JavaScript-like syntax, in the role of Zoho's Deluge. JavaScript can be added later behind the same `ScriptRunner` port.
- Billing and payments are not part of the MVP; usage is counted from day one.

## Open questions for you

1. Product name and branding. This decides the crate prefix and domain names.
2. SaaS only, or self-hosted / on-prem distribution as well? This affects licensing, telemetry and the upgrade path.
3. Which 10 apps matter most to your target users? This sets the order of connectors in Phase 2.
4. Is a web-only MVP client acceptable? I recommend it.
5. Any compliance or data-residency needs (GDPR, SOC 2, HIPAA)? These affect retention, encryption and audit work.
6. Pricing model: tasks per month like Zoho, or per execution?
7. For the Phase 4 AI features, which model provider, and is sending customer payloads to it acceptable?
