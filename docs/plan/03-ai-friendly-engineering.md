# 03: AI-friendly engineering

The whole product will be written mostly by AI coding agents, across hundreds of separate sessions. Each session starts with **no memory**, sees only part of the code, and tends to:

| Agent failure mode | What it looks like | Countermeasure in this plan |
|---|---|---|
| Context loss | Re-invents a helper that already exists, or uses a different pattern | Vertical slices, a golden slice to copy, the generated `docs/generated/map.md` inventory, per-crate `AGENTS.md` |
| Pattern drift | Every feature ends up structured differently | One canonical shape per kind of file (doc 02 §6), recipes, review against the recipe |
| Silent breakage | Changes a shared type, and something far away breaks at runtime | Strong types, exhaustive enums, compile-time SQL, generated client, snapshot tests, contract tests |
| Over-reach | "While I was here I refactored…" | Specs with an explicit file list, CODEOWNERS on critical paths, a size budget per task |
| Weak or tautological tests | Tests assert what the code does, not what it should do | Specs with acceptance criteria mapped to test names, mutation testing, property tests |
| Test tampering | Edits the test until it passes | A rule ("tests change only when the spec changes"), snapshot diffs that must be reviewed, CODEOWNERS on contract tests |
| Hallucinated APIs | Calls crate functions that do not exist | Popular, boring crates; pinned versions; the compiler as the first reviewer |
| Long files rot | A 2,000-line file nobody, human or AI, can safely edit | File size lint (warn above 400 lines, fail above 800), one use case per file |

---

## 1. The 15 rules (also summarised in `AGENTS.md`)

1. **Locality.** A feature change should touch one slice folder per layer. When a change needs edits in more than 3 crates, the spec has to say why.
2. **One way to do things.** Copy the golden slice. If a new pattern is needed, write an ADR first.
3. **The compiler is the first reviewer.** Use newtypes for IDs and constrained strings, enums instead of strings, no `_ =>` wildcard arms on domain enums (`clippy::wildcard_enum_match_arm`), and `#[must_use]` on results.
4. **The pure core stays pure.** `expr`, `domain` and `engine` have no async and no IO. `xtask check-arch` enforces this.
5. **Small units.** Files ≤ 400 lines (excluding tests), functions ≤ ~60 lines, at most 3 levels of nesting.
6. **Explicit wiring.** No global state, no `lazy_static` singletons, no auto-registration magic (`inventory`, `ctor`). Registries are plain lists in one file.
7. **No clever macros.** The only declarative macro allowed is `define_id!`. No procedural macros of our own.
8. **Generated code is never edited by hand.** Generated files carry a `// @generated — do not edit; run `just gen`` header, and CI regenerates them and diffs.
9. **Tests come with every change**, at the lowest layer that can prove the behaviour (see doc 09 §2).
10. **Bug fixes start with a failing regression test.**
11. **Tests change only when behaviour or the spec changes.** The PR description must say which acceptance criterion changed.
12. **Contracts are snapshotted.** That covers the OpenAPI spec, the DB schema, connector manifests, compiled plans and API responses. Snapshot updates are reviewed and are never accepted automatically in CI.
13. **Migrations are append-only.** Never edit a merged migration; CI verifies checksums.
14. **No new dependency without a one-line justification** in `docs/dependencies.md`. `cargo-deny` enforces the bans.
15. **`just check` must pass before a task counts as done.** It runs exactly what CI runs.

---

## 2. Documentation system (written for agents first, humans second)

| Artifact | Location | Purpose | Size budget |
|---|---|---|---|
| Root `AGENTS.md` (`CLAUDE.md` symlink) | repo root | Map, commands, rules, definition of done | ≤ 200 lines |
| Crate `AGENTS.md` | each `crates/*/` | What the crate owns, its allowed dependencies, its local patterns, its pitfalls | ≤ 80 lines |
| Module docs `//!` | top of every file | First line: what this file is. Second: the related spec, if any. | 1–5 lines |
| Recipes | `docs/recipes/*.md` | Numbered steps plus the list of files to touch for common changes | 1–2 pages |
| Specs | `docs/specs/F-xxx-*.md` | Behaviour, API, data, acceptance criteria → test names | 1–3 pages |
| ADRs | `docs/adr/NNN-*.md` | Why a decision was made, and what was rejected | 1 page |
| Generated map | `docs/generated/map.md` | Every route, use case, port, connector, error code and table, with file paths | generated |
| Glossary | `docs/plan/01-product-scope.md` §1 | One word per concept | — |

### 2.1 `cargo xtask map`
This generates `docs/generated/map.md`, an index agents read *instead of* grepping:

```markdown
## HTTP routes
| Method | Path | Handler | Use case |
| POST | /api/v1/flows/{flow_id}/publish | crates/api/src/routes/flows/publish_flow.rs | crates/app/src/flows/publish_flow.rs |
## Error codes
| flow.revision_conflict | Conflict | crates/domain/src/error_codes.rs:41 |
## Connectors
| slack | oauth2 | actions: post_message, … | crates/connectors/src/slack/ |
## Tables
| flows | migrations/20261201090000_flows.sql |
```
CI fails when the committed map is stale, so it is always current.

### 2.2 Recipes to write in Phase 0 (from the golden slice)

| Recipe | Covers |
|---|---|
| `add-use-case.md` | A new `app/<feature>/<use_case>.rs`, its port methods, fakes and tests |
| `add-endpoint.md` | Handler, DTOs, router line, OpenAPI registration, HTTP test, `just gen` |
| `add-migration.md` | Naming, forward-only rules, the sqlx prepare step, the schema snapshot |
| `add-repository-method.md` | Port method, MySQL implementation, fake implementation, contract test |
| `add-native-connector.md` | Folder layout, manifest, actions, wiremock tests, conformance |
| `add-yaml-connector.md` | Spec file, fixtures, conformance |
| `add-connector-field-type.md` | SDK enum, validation, Flutter widget, fixture |
| `add-error-code.md` | `error_codes.rs`, its mapping, the Dart mapping |
| `add-flutter-screen.md` | Feature folder, provider, route, tests |
| `fix-a-bug.md` | Reproduce with a failing test at the lowest layer, then fix, then add a regression note |
| `add-step-type.md` | **Engine change, needs human review.** IR, compiler, interpreter, property tests. |

#### Example recipe: `docs/recipes/add-endpoint.md` (abridged)
```markdown
# Recipe: add a REST endpoint
Prereq: the use case exists (see add-use-case.md).
1. Create `crates/api/src/routes/<feature>/<use_case>.rs` by copying
   `crates/api/src/routes/folders/rename_folder.rs` (the golden example).
2. Add request/response DTOs to `routes/<feature>/dto.rs`; derive `Serialize/Deserialize/ToSchema`;
   implement `From<DomainType> for Dto`. Never serialize domain types directly
   (exception: `FlowDefinition`, which is a versioned contract on its own).
3. Annotate the handler with `#[utoipa::path(...)]`, listing every error status it can return.
4. Register it: one line in `routes/<feature>/mod.rs::router()`.
5. Add the path to `crates/api/src/openapi.rs`.
6. Test: add a case to `crates/server/tests/http/<feature>.rs`: happy path (snapshot),
   validation error, unauthorized (401), wrong org (404), role without permission (403).
7. Run `just gen` (OpenAPI, contract files, map), then `just check`.
8. Client follow-up (a separate task on the `client` branch): `just sync-contract`, then use the new endpoint.
Files you should have touched: 3–5 Rust files, openapi.json, map.md.
```

### 2.3 Spec template: `docs/specs/F-000-template.md`
```markdown
# F-0xx · <Feature name>
Status: draft | approved | in-progress | done      Phase: MVP | P2 | …
Related: ADR-…, F-…

## Problem / user story
As a <persona> I want <capability> so that <outcome>.

## Behaviour rules
R1. …
R2. …

## API changes
| Method | Path | Request | Response | Errors (codes) |

## Data changes
New tables or columns; migration name.

## Acceptance criteria (each → ≥1 test; test name in brackets)
AC1. Given …, when …, then … [publish_flow_with_missing_trigger_returns_validation_error]
AC2. …

## Out of scope
## Test plan (layer → tests)
## Expected files to change (agents: stay inside this list or explain why not)
```

### 2.4 Initial ADRs (write in Phase 0)
| ADR | Title |
|---|---|
| 001 | Modular monolith in a Cargo workspace with compiler-enforced boundaries |
| 002 | MySQL-backed job queue with `FOR UPDATE SKIP LOCKED` |
| 003 | sqlx compile-time-checked queries, no ORM |
| 004 | Pure engine: compile the definition to IR, interpret purely, imperative runner |
| 005 | `Arc<dyn Port>` dependency injection through a `Deps` struct |
| 006 | UUIDv7 `BINARY(16)` IDs with prefixed external encoding |
| 007 | minijinja templates and JSON condition trees for data mapping |
| 008 | RFC 9457 problem+json with stable error codes |
| 009 | OpenAPI-first client generation for Dart |
| 010 | Everything is a connector; the engine knows only control flow |
| 011 | At-least-once action execution with idempotency keys |
| 012 | Flutter: Riverpod, go_router, pub workspace |
| 013 | Rhai (pure-Rust, sandboxed) for custom functions (P2), behind a `ScriptRunner` port; JavaScript possible later |

---

## 3. Guardrails (mechanical, so we don't rely on agent discipline)

### 3.1 Workspace lints (`Cargo.toml`)
```toml
[workspace.lints.rust]
unsafe_code = "forbid"
missing_debug_implementations = "warn"

[workspace.lints.clippy]
all = { level = "deny", priority = -1 }
pedantic = { level = "warn", priority = -1 }
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
todo = "deny"
unimplemented = "deny"
dbg_macro = "deny"
print_stdout = "deny"
print_stderr = "deny"
wildcard_enum_match_arm = "deny"
large_futures = "warn"
module_name_repetitions = "allow"
missing_errors_doc = "allow"
```
`clippy.toml`: `allow-unwrap-in-tests = true`, `allow-expect-in-tests = true`. The `weft-testkit` crate also allows these lints at crate level.

### 3.2 `cargo xtask` checks (all run inside `just check`)
| Check | Fails when |
|---|---|
| `check-arch` | A crate dependency edge breaks the table in doc 02 §4, or a pure crate uses a banned crate |
| `check-file-size` | A non-test source file is over 800 lines (warns over 400) |
| `check-sql` | The SQL contains `NOW()`, `CURRENT_TIMESTAMP` or `SELECT *`, or a query string sits outside `weft-db` |
| `check-migrations` | A merged migration file changed (the hash list is in `migrations/.checksums`) |
| `check-generated` | Regenerating the OpenAPI spec, the contract files or the map produces a diff |
| `check-error-codes` | Error codes are duplicated, or a code is missing from the Dart mapping |
| `check-docs` | A use case file has no `//!` header, or a crate has no `AGENTS.md` |

### 3.3 `just` commands (the only commands agents need)
```
just setup        # install tools (sqlx-cli, nextest, insta, llvm-cov, just, deny, machete)
just db-up        # docker compose up mysql + mailpit
just db-reset     # drop, create, migrate
just dev          # weft all with auto-reload
just gen          # openapi/*.json + fixtures → docs/generated/map.md → sqlx prepare
just test-unit    # nextest, no DB needed (< 60 s)
just test-db      # DB, HTTP and integration suites (needs MySQL)
just test         # both of the above
just e2e          # docker compose stack + e2e crate
just check        # fmt + clippy + xtask checks + test + sqlx prepare --check + deny  ← CI
# on the client branch: just sync-contract, just flutter-check (dart format/analyze + flutter test)
```

### 3.4 CODEOWNERS (human review required)
```
/migrations/                         @humans
/crates/engine/                      @humans
/crates/domain/src/flow/definition*  @humans
/crates/domain/src/permissions*      @humans
/crates/infra/src/crypto*            @humans
/crates/infra/src/http/ssrf*         @humans
/crates/api/src/extract/actor*       @humans
/crates/testkit/src/contracts/       @humans
/deny.toml /Cargo.toml /.github/     @humans
```

### 3.5 PR template checklist
- [ ] The spec ID is linked, and the ACs implemented are listed.
- [ ] `just check` passes locally.
- [ ] Snapshot changes are explained, one line each.
- [ ] There are no edits to merged migrations, and none to generated files except through `just gen`.
- [ ] Recipes or AGENTS.md are updated if a pattern changed.

---

## 4. How to prompt (the session protocol)

### 4.1 Standard task prompt
```text
Read AGENTS.md, then docs/specs/F-014-delay-step.md, then docs/recipes/add-use-case.md.
Task: implement F-014 acceptance criteria AC1–AC5 only.
Constraints:
- Touch only the files listed in the spec's "Expected files to change". If you must go
  outside that list, stop and explain why first.
- No new dependencies. No edits to merged migrations.
Process:
1. Write down which files you will change and which tests you will add (with names).
2. Write the failing tests first and run them to confirm they fail.
3. Implement until they pass.
4. Run `just gen` if any API changed, then `just check`. Fix everything it reports.
5. Update the spec status. Update a recipe if you introduced or changed a pattern.
Report: a summary, any deviations from the spec, and follow-ups.
```

### 4.2 Task sizing
- **One prompt = one vertical slice**: typically 5–15 files and under ~800 changed lines including tests.
- If a spec has more than ~6 ACs, split it into several prompts (AC1–3, then AC4–6).
- Engine, crypto, auth and migration tasks get a **human review** before merge, whatever their size.

### 4.3 Prompt playbook (examples)
| Situation | Prompt skeleton |
|---|---|
| New connector | "Read AGENTS.md and docs/recipes/add-native-connector.md. Implement the `trello` connector per docs/specs/F-061-trello.md. Use wiremock fixtures; no live calls. The conformance suite must pass." |
| Bug | "Read docs/recipes/fix-a-bug.md. Bug: delays scheduled across a DST change fire one hour early (issue #123). First add a failing test in `weft-domain` schedule tests reproducing it, then fix." |
| Refactor | "Refactor only `crates/app/src/runner/` to extract retry handling into `retry.rs`. Behaviour must not change: no test files may change except moved `mod` paths. Run `just check`." |
| Review | "Review the diff on this branch against AGENTS.md rules and the spec F-0xx. List violations with file:line. Do not change code." |
| Explore | "Using docs/generated/map.md, explain how a webhook request becomes an execution. Cite file:line. Do not change code." |

### 4.4 Building the codebase in an order agents can follow
1. **Phase 0 golden slice:** a human (or an agent under close supervision) builds *Folders CRUD* end to end: migration, domain, port, MySQL repo, fake, contract test, use cases, endpoints, OpenAPI, Dart client and a Flutter list screen. It is polished until it is exemplary. Every recipe is written *from* this slice.
2. After that, every feature prompt says "copy the shape of the folders slice".
3. Re-polish the golden slice whenever a pattern changes, and update the recipes in the same PR.

---

## 5. Banned patterns (written in AGENTS.md as "Never")
- `unwrap()`/`expect()` outside tests; `panic!` for control flow.
- Business logic in HTTP handlers or SQL adapters.
- `Utc::now()`, `rand::thread_rng()` or `Uuid::now_v7()` called directly; use the `Clock`/`IdGen` ports.
- `NOW()` in SQL; string-concatenated SQL; `SELECT *`.
- New global state, singletons, or `static` with interior mutability.
- `tokio::spawn` outside `weft-worker`/`weft-server`.
- Catch-all `_ =>` arms on domain enums.
- Serializing domain types straight to API responses (`FlowDefinition` is the one exception).
- `#[ignore]`, `sleep`-based waits or time-dependent assertions in tests.
- Editing generated files, merged migrations or snapshot files by hand.
- Renaming or moving files that the current task does not need.

---

## 6. Development environment for agents
Agents (in local CLIs or cloud sandboxes) need: the Rust toolchain from `rust-toolchain.toml`, `just`, `sqlx-cli`, `cargo-nextest`, `cargo-insta`, and **a reachable MySQL 8.4** (through `docker compose`, or `apt`-installed in sandboxes without Docker). For client tasks they also need the Flutter SDK.
- With no database, `SQLX_OFFLINE=true` still builds the workspace from the committed `.sqlx/` data, and `just test-unit` still runs.
- DB suites live in dedicated test binaries (`tests/db/`, `tests/http/`), so `just test-unit` excludes them by nextest filter instead of letting them fail confusingly.
- **Any change to SQL needs a live database** to run `cargo sqlx prepare`. Specs that change SQL say so, so they are not handed to a sandbox without MySQL.
- Provide a setup script for cloud agent environments that installs MySQL and the tools, so every session starts ready.
