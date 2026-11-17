# AGENTS.md: rules for AI agents working in this repository
<!-- Source template: docs/plan/templates/AGENTS.md. CLAUDE.md is a symlink to this file. Keep ≤ 200 lines. -->

## What this is
Weft is a Zoho Flow–style integration platform. Users build **flows**: one trigger (webhook, schedule, app, manual) followed by steps (actions, decision, delay, set variable, …). Every run is an **execution** with step-by-step history.
- Backend: Rust workspace (`crates/`) with MySQL 8.4, which is also the job queue.
- Client: Flutter, on the separate **`client` branch** of this repository. It is not in this tree. The backend publishes the API contract in `openapi/` and `fixtures/`, and the client syncs it.
- Plan and architecture: `docs/plan/`. Inventory of routes, use cases, connectors, error codes and tables: `docs/generated/map.md` (**read this before searching**).

## Current status (update this as backlog tasks land)
- **Done:** backlog T00 scaffold. The workspace has 12 crates with empty `lib.rs` files, the `AGENTS.md` per crate, lints, a pinned toolchain, the `justfile`, the docker-compose file and the nextest config.
- **Available now:** `just db-up`, `just db-down`, `just fmt`, `just lint`, `just test-unit`, `just test`, `just check`.
- **Not yet available:** `just db-reset`, `just gen`, the `xtask` checks, `docs/generated/map.md`, `docs/recipes/`, `docs/specs/` and the golden slice. Those arrive with T01–T10 (see `docs/plan/12-roadmap-and-backlog.md`). Until then, follow `docs/plan/` directly.

## Map
| Need to… | Go to |
|---|---|
| Change business rules or flow definition types | `crates/domain/` (pure; no async, no IO) |
| Change templates or conditions | `crates/expr/` (pure) |
| Change how flows execute | `crates/engine/` (pure, 🔒 human review) and `crates/app/src/runner/` |
| Add or modify a use case | `crates/app/src/<feature>/<use_case>.rs` (one per file) |
| Add a port method | `crates/app/src/ports/` + `crates/db/src/` + `crates/testkit/src/fakes/` + `crates/testkit/src/contracts/` |
| Add or modify an endpoint | `crates/api/src/routes/<feature>/<use_case>.rs` (same file stem as the use case) |
| Add a connector | `crates/connectors/src/<app>/` (native) or `crates/connectors/src/declarative/specs/<app>.yaml` |
| Background loops | `crates/worker/` |
| Wiring and config | `crates/server/` |
| Test helpers | `crates/testkit/` |
| SQL schema | `migrations/` (append-only, 🔒) |
| Flutter feature | The `client` branch: `app/lib/features/<feature>/{data,application,presentation}` |

Crate dependency rules are in `docs/plan/02-architecture.md` §4 and are enforced by `cargo xtask check-arch`.

## Commands
```
just db-up         # start MySQL + Mailpit
just db-reset      # recreate DB and migrate
just gen           # regenerate openapi/*.json, fixtures, docs/generated/map.md, .sqlx
just test-unit     # fast tests, no DB (run often)
just test          # all tests (needs MySQL)
just check         # EVERYTHING CI runs. Must pass before you finish.
```
On the `client` branch: `just sync-contract` and `just flutter-check`.

## How to work
1. Read the spec (`docs/specs/F-xxx-*.md`) and the matching recipe (`docs/recipes/*.md`).
2. List the files you will change and the tests you will add, by name. Stay within the spec's "Expected files".
3. Write failing tests first and confirm they fail for the right reason.
4. Implement by copying the shape of the golden slice (`folders`) and the canonical shapes in `docs/plan/02-architecture.md` §6.
5. Run `just gen` if you touched the API, SQL or connectors, then run `just check` and fix everything.
6. Update the spec status. If you introduced or changed a pattern, update the recipe in the same change.
7. Report: what changed, the tests added, any deviations from the spec, and follow-ups.

## Must
- Check permissions first in every use case (`actor.require(...)`), and scope every query by `org_id`.
- Use `deps.clock.now()` and `deps.ids.*`. Bind time into SQL (`?`); never use `NOW()`.
- Make two writes atomic by adding **one** repository method named for the business operation.
- Return typed errors with stable codes from `crates/domain/src/error_codes.rs`.
- Keep files ≤ 400 lines and functions ≤ ~60 lines. Every file starts with a `//!` line saying what it is.
- Test at the lowest layer that proves the behaviour. Every acceptance criterion needs a named test.
- Add new test builders to `weft-testkit` rather than hand-building large structs.

## Never
- `unwrap`/`expect`/`panic!` outside tests; `unsafe`; `todo!`; `dbg!`; `println!`.
- Business logic in handlers, SQL adapters or Flutter widgets.
- `Utc::now()`, `rand::thread_rng()` or `Uuid::now_v7()` called directly; global state; `tokio::spawn` outside `worker`/`server`.
- `_ =>` wildcard arms on domain enums.
- `SELECT *`, string-built SQL, or SQL outside `crates/db`.
- Editing merged migrations, generated files (`openapi/`, `.sqlx/`, `docs/generated/`) or snapshot files by hand.
- Weakening contract suites, conformance checks, property-test generators or RBAC/isolation sweeps.
- Changing a test to make it pass when the spec did not change. Sleeps, real network or wall-clock time in tests. `#[ignore]`.
- Adding dependencies without a line in `docs/dependencies.md`.
- Renaming, moving or reformatting files the task doesn't need.

## Definition of done
- [ ] `just check` passes.
- [ ] If the API changed, a follow-up task for the `client` branch is listed in the report.
- [ ] Every acceptance criterion has at least one named test, and the tests failed before the fix.
- [ ] Snapshot changes are reviewed and explained, one line each.
- [ ] `just gen` has been run if the API, SQL or connectors changed, and the generated files are committed.
- [ ] The spec status is updated; recipes and AGENTS.md are updated if a pattern changed.

## When unsure
Stop and ask, or write your question into the report, instead of guessing. This applies especially to anything marked 🔒: engine, migrations, auth, crypto, the SSRF guard, the permission matrix and contract suites.
