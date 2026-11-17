# 09: Rust test strategy

## 1. Goals and principles

1. **Prove behaviour, not implementation.** Tests come from spec acceptance criteria and invariants, not from reading the code back.
2. **Push logic into the pure core, then test it exhaustively there.** Properties, tables and snapshots over `expr`, `domain` and `engine` give most of the confidence at the lowest cost.
3. **Fakes are trusted only when they are proved.** Every in-memory fake must pass the same **contract suite** as its MySQL counterpart.
4. **Determinism is non-negotiable.** No sleeps, no wall clock, no real randomness, no network. Time, IDs and jitter are injected, and background work is driven explicitly through `drain()`.
5. **Contracts are snapshotted.** That covers the API JSON, OpenAPI, the DB schema, compiled plans and connector manifests. Any change appears in the diff and must be reviewed.
6. **Measure test strength, not just coverage.** Mutation testing finds tests that would pass even if the code were wrong, which is the typical weakness of AI-written tests.
7. **Fast feedback first.** `just test-unit` runs without a database in under 60 seconds and is what agents run in their inner loop.

---

## 2. The pyramid: which kind of test for which code

```
                 ▲  e2e (black-box, docker stack)            few      minutes
                 │  scenario tests (in-process, real MySQL,  dozens   seconds
                 │     ManualClock, wiremock)
                 │  HTTP API tests (router + MySQL)          hundreds  seconds
                 │  DB tests (sqlx::test) + contract suites  hundreds  seconds
                 │  connector tests (wiremock)               hundreds  ms
                 │  use-case tests (fakes)                   hundreds  ms
                 │  unit + property + snapshot (pure core)   thousands ms
```

| Code under test | Primary test kind | Location | Needs MySQL |
|---|---|---|---|
| `weft-expr` filters, rendering, conditions | Table-driven unit tests, property tests, fuzzing | in-file `#[cfg(test)]`, `tests/props.rs`, `fuzz/` | No |
| `weft-domain` validation, schedules, permissions, ID codecs | Unit, property, exhaustive tables | in-file, `tests/` | No |
| `weft-engine` compiler and interpreter | Plan snapshots, invariant property tests, simulator scenarios | in-file, `tests/props.rs`, `tests/snapshots/` | No |
| `weft-app` use cases and runner | Use-case tests against fakes (`TestDeps`) | in-file `#[cfg(test)]` | No |
| Port implementations (`weft-db`) | **Contract suites** plus adapter-specific tests (`sqlx::test`) | `crates/db/tests/db/` | Yes |
| Job queue | Concurrency and lease tests | `crates/db/tests/db/job_queue.rs` | Yes |
| `weft-infra` (crypto, HTTP/SSRF, JWT, mailer) | Unit and property tests; Mailpit for SMTP | in-file, `tests/` | No (Mailpit for one suite) |
| `weft-connectors` | Conformance suite, wiremock action tests | `crates/connectors/tests/` | No |
| `weft-api` routes | HTTP tests (snapshot, RBAC matrix, isolation sweep) | `crates/server/tests/http/` | Yes |
| Whole runtime | Scenario tests (webhook → worker → apps) | `crates/server/tests/scenarios/` | Yes |
| Deployed stack | e2e smoke | `e2e/` | Yes (via docker) |

**Rule for agents:** add the test at the **lowest layer that can prove the acceptance criterion**, plus one test at the layer above **only if** wiring could break it (for example, a new endpoint needs a use-case test *and* one HTTP test).

---

## 3. Tooling

| Tool | Use |
|---|---|
| `cargo-nextest` | Test runner: process-per-test isolation, test groups, JUnit output |
| `rstest` | Table-driven cases and fixtures |
| `proptest` | Property-based tests (engine invariants, expressions, codecs, schedules) |
| `insta` (`json`, `yaml`, `redactions`) | Snapshot tests; `cargo insta review` |
| `sqlx::test` | Fresh MySQL database per test with migrations applied |
| `wiremock` | HTTP mock server for connectors and HTTP actions |
| `tokio` (`test-util`) | `#[tokio::test]`; `time::pause` for the few timer-based components |
| `fake` | Realistic test data where values don't matter |
| `cargo-llvm-cov` | Coverage (with nextest) |
| `cargo-mutants` | Mutation testing |
| `cargo-fuzz` (libFuzzer) | Fuzzing parsers and evaluators |
| `criterion` | Micro-benchmarks for hot pure code |
| `k6` + `cargo xtask bench-worker` | Load and throughput tests |

### nextest configuration (`.config/nextest.toml`)
```toml
[profile.default]
retries = 0                      # flaky tests are fixed or quarantined, never retried silently
fail-fast = false
slow-timeout = { period = "30s", terminate-after = 4 }

[test-groups]
mysql = { max-threads = 8 }      # sqlx::test DBs are isolated; cap parallelism for the server

[[profile.default.overrides]]
filter = 'binary(db) | binary(http) | binary(scenarios)'
test-group = 'mysql'

[profile.ci]
junit = { path = "junit.xml" }
```
`just test-unit` runs `cargo nextest run -E 'not (binary(db) | binary(http) | binary(scenarios))'`.

---

## 4. Layout and naming

- **Unit tests** live in the same file under `#[cfg(test)] mod tests`, below the code.
- **Integration suites** use **one test binary per suite** to keep link times low: `tests/db/main.rs` with `mod flow_repo; mod job_queue; …`, `tests/http/main.rs`, `tests/scenarios/main.rs`.
- **Test names** are sentences: `<unit>_<condition>_<expected>`, for example `publish_flow_with_stale_revision_returns_conflict` or `schedule_daily_0230_ny_shifts_forward_on_spring_dst_gap`.
- **Structure:** Arrange / Act / Assert, separated by blank lines. One behaviour per test. Table-driven tests are fine when only data varies.
- **Snapshots** sit next to their tests in `snapshots/` and are committed. Snapshot names are explicit (`assert_json_snapshot!("publish_flow_ok", body)`).

---

## 5. The pure core (thousands of fast tests)

### 5.1 Table-driven unit tests (`weft-expr`)
```rust
#[rstest]
#[case::typed_number("{{ trigger.n }}",          json!({"n": 5}),     json!(5))]
#[case::text_render("n={{ trigger.n }}",          json!({"n": 5}),     json!("n=5"))]
#[case::lenient_undefined("{{ trigger.nope }}",   json!({}),           json!(null))]
#[case::array_passthrough("{{ trigger.xs }}",     json!({"xs": [1,2]}), json!([1, 2]))]
#[case::filter_chain("{{ trigger.s | trim | upper }}", json!({"s": " a "}), json!("A"))]
fn render_value_cases(#[case] tpl: &str, #[case] trigger: Value, #[case] expected: Value) {
    let scope = Scope::default().with("trigger", trigger);

    let rendered = render_value(tpl, &scope).unwrap();

    assert_eq!(rendered.value, expected);
}
```
Every filter and every condition operator gets a table that covers normal input, empty input, wrong types, Unicode, and boundary numbers.

### 5.2 Property tests (`weft-expr`, `weft-domain`)
| Property | Crate |
|---|---|
| Rendering never panics for arbitrary templates and scopes; it returns `Ok` or a typed error | expr |
| Payload strings containing `{{ … }}` are never evaluated (data stays data) | expr |
| Condition `all([])` is true, `any([])` is false, and `not(not(c)) == c` for generated trees | expr |
| `FlowDefinition` JSON round-trip: `from_json(to_json(d)) == d` | domain |
| ID round-trip: `FlowId::from_str(&id.to_string()) == id`; the wrong prefix is rejected | domain |
| `ScheduleSpec::next_after(t) > t`, monotonic in `t`, and always on the declared local time or the DST-shifted time | domain |
| Permission check: `allows(owner, p)` holds for every `p`; viewer never has write permissions | domain |

### 5.3 Exhaustive tables (`weft-domain`)
- **The RBAC matrix:** every `(Role, Permission)` cell asserted against a literal table copied from doc 08. Adding a permission without a row fails to compile, because the table is a `match` with no wildcard.
- **Schedules:** DST gaps and overlaps (US and EU), Asia/Kolkata (no DST, +05:30), the 31st in short months, February 29, `once` in the past, and cron edge cases.
```rust
#[test]
fn schedule_daily_0230_ny_shifts_forward_on_spring_dst_gap() {
    let spec = ScheduleSpec::daily_at("02:30", "America/New_York");

    let next = spec.next_after(ts("2026-03-07T12:00:00Z")).unwrap();

    // 2026-03-08 02:30 doesn't exist in New York; policy (ADR): shift forward by the gap → 03:30 EDT
    assert_eq!(next, ts("2026-03-08T07:30:00Z"));
}
```

### 5.4 Engine: plan snapshots
```rust
#[rstest]
fn compiled_plans_match_snapshots(#[files("../../fixtures/flows/**/*.json")] path: PathBuf) {
    let def = load_definition(&path);

    let plan = compile(&def).unwrap();

    insta::assert_snapshot!(snapshot_name(&path), plan.to_string());
}
```
The output is the human-readable listing from doc 04 §4. Any compiler change shows up as a readable diff.

### 5.5 Engine: invariant property tests (the most important tests in the codebase)
A pure **`Simulator`** (in `weft-engine` behind `feature = "testing"`) loops `advance` and `record_result`. It feeds action results from a generated **outcome script** (succeed with output / fail retryable / fail permanent) and jumps a fake `now` forward on `Sleep` and `RetryAt`.

```rust
proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    // I1: always terminates within the budget, with a terminal outcome
    #[test]
    fn interpreter_always_terminates(def in arb_definition(4), script in arb_outcomes(64)) {
        let plan = compile(&def).unwrap();
        let run = Simulator::new(&plan, &def, script).run_to_end();
        prop_assert!(run.instructions <= MAX_INSTRUCTIONS);
        prop_assert!(run.outcome.is_terminal());
    }

    // I3: persisting/reloading RunState at arbitrary checkpoints changes nothing
    #[test]
    fn resume_from_any_checkpoint_is_equivalent(
        def in arb_definition(4), script in arb_outcomes(64),
        reload_points in prop::collection::btree_set(0usize..64, 0..6),
    ) {
        let plan = compile(&def).unwrap();
        let straight = Simulator::new(&plan, &def, script.clone()).run_to_end();
        let resumed  = Simulator::new(&plan, &def, script).reload_state_at(&reload_points).run_to_end();
        prop_assert_eq!(straight.actions_emitted, resumed.actions_emitted);
        prop_assert_eq!(straight.outcome, resumed.outcome);
    }

    // I5: a succeeded step is never emitted again; actions follow program order on the taken path
    #[test]
    fn succeeded_steps_never_rerun(def in arb_definition(4), script in arb_outcomes(64)) {
        let plan = compile(&def).unwrap();
        let run = Simulator::new(&plan, &def, script).run_to_end();
        prop_assert!(run.no_duplicate_successes());
        prop_assert!(run.actions_in_program_order(&plan));
    }

    // Retry policy: attempts never exceed max; retry times are increasing; permanent errors never retry
    #[test]
    fn retries_respect_policy(def in arb_definition(3), script in arb_outcomes(64)) { /* … */ }
}
```
Generator notes: `arb_definition(depth)` uses `prop_recursive` (actions, delays and set-variable steps as leaves; decisions with 0–3 branches as nodes), then **renumbers step keys** so they are unique, and only emits templates that reference earlier steps. `arb_outcomes` biases towards success (80%) so runs go deep. Failing cases shrink to a minimal definition, which is then committed as a regression fixture.

---

## 6. Use cases, repositories and the database

### 6.1 Use-case tests against fakes (`weft-app`, no DB)
`weft-testkit::TestDeps` wires `Deps` with fakes, a `ManualClock`, `SequentialIds`, zero-jitter randomness and a capturing `FakeMailer`.
```rust
#[tokio::test]
async fn publish_flow_with_stale_revision_returns_conflict() {
    let t = TestDeps::new();
    let owner = t.owner();
    let flow = t.given_flow(&owner, FlowDefBuilder::webhook().http_post("ping", "https://example.com").build()).await;

    let err = publish_flow(&t.deps, &owner, PublishFlowInput { flow_id: flow.id, expected_revision: flow.draft_revision + 1 })
        .await.unwrap_err();

    assert_eq!(err.code, "flow.revision_conflict");
}
```
Every use case gets, at minimum: a happy path, a permission denied case, a not found / other-org case, every validation rule from its spec, and every conflict path.

### 6.2 Repository contract suites (fake ≡ MySQL)
Fakes make use-case tests fast, but a fake that drifts from real behaviour causes false confidence. To prevent that, each port has **one contract suite**, written once in `weft-testkit::contracts`, which runs against **both** implementations:
```rust
// crates/testkit/src/contracts/flow_repo.rs
pub async fn flow_repo_contract(repo: &dyn FlowRepo, seed: &dyn Seed) {
    get_is_scoped_to_org(repo, seed).await;
    save_draft_increments_revision(repo, seed).await;
    save_draft_with_stale_revision_conflicts(repo, seed).await;
    publish_assigns_next_version_number(repo, seed).await;
    soft_deleted_flows_are_not_listed(repo, seed).await;
    list_paginates_stably_by_updated_at_then_id(repo, seed).await;
}

// crates/testkit/tests/contracts.rs  (fakes)
#[tokio::test]
async fn fake_flow_repo_honours_contract() {
    flow_repo_contract(&FakeFlowRepo::default(), &NoopSeed).await;
}

// crates/db/tests/db/contracts.rs  (MySQL)
#[sqlx::test(migrations = "../../migrations")]
async fn mysql_flow_repo_honours_contract(pool: MySqlPool) {
    flow_repo_contract(&MySqlFlowRepo::new(pool.clone()), &DbSeed::new(pool)).await;
}
```
`Seed` creates prerequisite rows (orgs, users) that MySQL's FKs need; for fakes it does nothing. Contract suites are **CODEOWNED**: an agent must not weaken them to make a fake pass.

### 6.3 Database-specific tests (`sqlx::test`)
`#[sqlx::test]` creates a **fresh database per test**, applies all migrations and drops it afterwards, so tests are fully isolated and parallel. They cover:
- Behaviour that only exists in SQL: unique constraints (webhook token, dedupe keys), `ON DUPLICATE KEY` usage counters, JSON column round-trips, `DATETIME(6)` precision, collation and case-insensitive email uniqueness.
- **Schema snapshot:** migrations applied to an empty DB must produce exactly the committed schema, so every schema change is reviewable.
```rust
#[sqlx::test(migrations = "../../migrations")]
async fn schema_matches_snapshot(pool: MySqlPool) {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables \
         WHERE table_schema = DATABASE() AND table_name <> '_sqlx_migrations' ORDER BY table_name")
        .fetch_all(&pool).await.unwrap();
    let mut ddl = String::new();
    for t in tables {
        let (_, create): (String, String) =
            sqlx::query_as(&format!("SHOW CREATE TABLE `{t}`")).fetch_one(&pool).await.unwrap();
        ddl.push_str(&strip_auto_increment(&create));
        ddl.push_str(";\n\n");
    }
    insta::assert_snapshot!(ddl);
}
```
- **Migrations:** apply all migrations to an empty database (the test above); checksum immutability (`xtask check-migrations`); and in P3 a "migrate from the last release's schema with representative data" test.
- **`cargo sqlx prepare --check`** in CI guarantees the committed `.sqlx/` metadata matches the queries.

### 6.4 Job queue: concurrency, leases and failure
```rust
#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_claimers_never_claim_the_same_job(pool: MySqlPool) {
    let q = MySqlJobQueue::new(pool.clone());
    let now = ts("2026-01-01T00:00:00Z");
    for i in 0..500 { q.enqueue(test_job(i), now).await.unwrap(); }

    let claimed: Vec<JobId> = futures::future::join_all((0..8).map(|w| {
        let q = q.clone();
        async move {
            let mut mine = Vec::new();
            loop {
                let batch = q.claim("executions", &format!("w{w}"), 10, now).await.unwrap();
                if batch.is_empty() { break mine; }
                mine.extend(batch.iter().map(|j| j.id));
            }
        }
    })).await.into_iter().flatten().collect();

    assert_eq!(claimed.len(), 500);
    assert_eq!(claimed.iter().collect::<HashSet<_>>().len(), 500);
}
```
Other cases:
- Jobs not yet due are not claimed.
- An expired lease is reaped and claimed again.
- A heartbeat extends the lease.
- `max_attempts` leads to `dead`.
- Rescheduling sets `run_at`.
- A dedupe key prevents double enqueue.
- Completion deletes the row.
- Two schedulers firing the same slot produce one execution.

---

## 7. HTTP API tests (`crates/server/tests/http/`)

### 7.1 Harness
`TestApp::spawn(pool)` builds the **real** router with real MySQL adapters, plus a `ManualClock`, `SequentialIds`, `FakeMailer`, and an HTTP policy that allows `127.0.0.1` so wiremock is reachable. It also starts an in-process worker that does nothing until `drain()` is called. Requests go through `tower::ServiceExt::oneshot`, with no socket. Helpers: `signup()`, `login()`, `create_flow()`, `publish_and_enable()`, `post_webhook()`.

```rust
#[sqlx::test(migrations = "../../migrations")]
async fn publish_flow_returns_version(pool: MySqlPool) {
    let app = TestApp::spawn(pool).await;
    let owner = app.signup("owner@example.com").await;
    let flow = app.create_flow(&owner, fixtures::flow("webhook_to_http.json")).await;

    let res = app.post(&format!("/api/v1/orgs/{}/flows/{}/publish", owner.org_id, flow.id))
        .bearer(&owner.token).header("If-Match", "\"1\"").send().await;

    assert_eq!(res.status(), 200);
    insta::assert_json_snapshot!("publish_flow_ok", res.json());   // deterministic ids/clock ⇒ no redactions
}
```

### 7.2 What every endpoint is tested for
| Case | Expected |
|---|---|
| Happy path | Status plus a JSON snapshot |
| Malformed or invalid body | 400/422 problem+json with issue paths (snapshot) |
| No or expired token | 401 |
| Role without the permission | 403 (from the matrix, §7.3) |
| A resource in another org | 404 (from the isolation sweep, §7.4) |
| Missing or stale `If-Match` (where required) | 428/412 |

### 7.3 RBAC matrix sweep (generated, not hand-written)
`weft-api` exposes a `ROUTE_PERMISSIONS` table (`method, path → Permission | Public`). Two tests use it:
1. **Completeness:** every operation in `openapi.json` appears in the table, so a new route without a permission fails the build.
2. **Sweep:** for every route and every role, call the route with a valid body fixture and assert 403 exactly when `allows(role, permission)` is false.

### 7.4 Tenant isolation sweep
Create a full set of resources in org A. Then, as an owner of org B, call **every** route with A's IDs. Expect 404, and expect the response body to contain none of A's IDs or names.

### 7.5 OpenAPI drift
`cargo xtask openapi --check` regenerates the spec and diffs it against `openapi/openapi.json`. CI also runs `oasdiff breaking` against `main` (doc 06 §4).

---

## 8. Connector tests (`crates/connectors/tests/`)

### 8.1 Conformance suite (automatic for all connectors)
```rust
#[test]
fn all_connectors_conform() {
    for c in weft_connectors::builtin_connectors() {
        conformance::check(&c).unwrap_or_else(|e| panic!("{}: {e}", c.manifest.key));
    }
}
```
It checks the manifest rules from doc 07 §6, YAML template compilation, and the native "input generated from the spec deserializes into `Input`" rule. New connectors are covered automatically.

### 8.2 Action tests with wiremock
```rust
#[tokio::test]
async fn slack_post_message_sends_expected_request_and_maps_output() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/api/chat.postMessage"))
        .and(header("authorization", "Bearer xoxb-test"))
        .and(body_partial_json(json!({ "channel": "C0123", "text": "hi" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("slack/post_message_ok.json")))
        .expect(1)
        .mount(&server).await;
    let ctx = test_ctx().base_url("slack", server.uri()).bearer("xoxb-test").build();

    let out = PostMessage.run(&ctx, json!({ "channel": "C0123", "text": "hi" })).await.unwrap();

    assert_eq!(out["ts"], "1700000000.000100");
}
```
The **error mapping table test** is required for every connector:

| Response | Expected `ConnectorError` |
|---|---|
| 401, or Slack `{ok:false,error:"invalid_auth"}` | `AuthExpired` |
| 429 with `Retry-After: 30` | `RateLimited{30s}` |
| 500 / 502 / 503 / timeout | `Transient` |
| 400 / 422 | `InvalidInput` |
| 404 | `NotFound` |

### 8.3 OAuth
Using wiremock as the provider, test: the code exchange (PKCE verifier sent), a state that is expired or reused (rejected), refresh before expiry, refresh on 401, **concurrent refresh** (two tasks result in exactly one refresh call), and `invalid_grant` leading to `needs_reauth`.

### 8.4 Live smoke tests (nightly only)
`tests/live/*.rs` are compiled only with `--features live` and read sandbox credentials from the environment. They run one cheap action per connector. Failures open an issue automatically; they never block PRs.

---

## 9. Scenario tests: the whole runtime in-process (`crates/server/tests/scenarios/`)

Real router, real MySQL, real runner and worker, connectors pointed at wiremock, and **time controlled by `ManualClock`**. `worker.drain()` processes every job due at `clock.now()` until none remain, so there are no sleeps and no flakiness.

```rust
#[sqlx::test(migrations = "../../migrations")]
async fn order_alert_demo(pool: MySqlPool) {
    let app = TestApp::spawn(pool).await;
    let slack = app.mock_connector("slack").await;            // connector base URL → wiremock
    slack.fail_once_then_ok("/api/chat.postMessage", 500, fixture("slack/post_message_ok.json")).await;
    let reviews = app.mock_host("reviews.example.com").await; // test HttpSender rewrites this host → wiremock
    reviews.ok("/api/requests", json!({ "id": "r1" })).await;
    let owner = app.signup("owner@example.com").await;
    let hook = app.publish_and_enable(&owner, fixtures::flow("order_alert.json")).await;

    app.post_webhook(&hook, json!([{ "amount": 250, "email": "a@x.io", "name": "ann lee" },
                                   { "amount": 20,  "email": "b@x.io", "name": "bo chen" }]))
        .await.assert_status(202);
    app.worker.drain().await;                                // Slack 500 → retry scheduled +1m
    app.clock.advance(Duration::minutes(1));  app.worker.drain().await;
    app.clock.advance(Duration::days(1));     app.worker.drain().await;   // delay elapses

    let runs = app.list_executions(&owner).await;
    assert_eq!(runs.len(), 2);
    assert!(runs.iter().all(|r| r.status == "succeeded"));
    assert_eq!(app.mailer.sent().len(), 1);                  // only the small order got an email
    assert_eq!(reviews.received("/api/requests").await.len(), 2); // both ran after their delay
    insta::assert_yaml_snapshot!("order_alert_timelines", app.timelines(&runs).await);
}
```

**Scenario catalogue (MVP):**
- The webhook array splits into N runs.
- Webhook idempotency key gives one run.
- A disabled flow's events are ignored.
- The schedule fires once per slot, including with two workers.
- The misfire policy after downtime.
- Delay-for and delay-until.
- Decision with nested branches.
- Set-variable scope.
- `on_error: continue`.
- Retry exhaustion leads to failure and a notification email.
- Retry from the failed step reuses earlier outputs.
- Re-run uses the latest version.
- Cancelling a waiting run.
- The quota is exceeded.
- A test run uses the draft snapshot.
- Editing the draft does not affect a running execution.

### 9.1 Fault injection (crash safety)
`weft-app` defines `FaultPoint`s that are compiled in only with `feature = "fault-injection"`, which only the test harness enables: `BeforeStepStart`, `AfterActionBeforeCheckpoint`, `AfterCheckpointBeforeReschedule`, `DuringFinish`. Scenario tests trigger a "crash" (the handler future is aborted and the lease left in place), advance the clock past the lease, then `reap_and_drain()`. They assert that:
- every execution still reaches the same terminal state;
- an action is duplicated **only** at `AfterActionBeforeCheckpoint`, where the history shows `possible_duplicate`;
- no step that had succeeded is re-executed at any other fault point.

---

## 10. Security tests (mapped from doc 08 §10)
- **SSRF guard** (table-driven): every blocked CIDR, IPv6 literal forms, IPv4-mapped IPv6, decimal and octal IP encodings in URLs, a redirect from a public host to `127.0.0.1` (wiremock under a public-looking resolver stub), a DNS name that resolves to a private address (stub resolver), and the `file://` and `gopher://` schemes.
- **Crypto:** encrypt/decrypt round-trip (property test); wrong AAD fails; tampered ciphertext fails; key rotation re-encrypts.
- **Redaction:** `format!("{:?}", connection_with_secret)` and the captured tracing output contain no secret, using a capturing subscriber in the test.
- **JWT:** expired, not-yet-valid, wrong key, `alg: none`, tampered claims.
- **Refresh rotation:** reuse revokes the family.
- **Webhook:** 5 MB + 1 byte gives 413; rate limit gives 429; HMAC valid, invalid and replayed (P2).
- **Templates:** fuel exhaustion returns an error, not a hang; a payload with `{{ 7*7 }}` is passed through literally.
- **Script sandbox (P2):** a memory bomb, `while(true){}`, and attempts to reach `require`, `fetch` or `std` all fail cleanly within their limits.

---

## 11. Cross-language contract tests (Rust ↔ Dart)
- `fixtures/flows/**/*.json` are canonical `FlowDefinition` documents (valid and invalid ones, every step type, every historical schema version).
- **Rust:** each valid fixture deserializes, re-serializes **byte-identically** (canonical form: sorted keys, 2-space indentation), validates and compiles. Each invalid fixture produces the expected issue codes, which are snapshotted.
- **Dart:** the same files are parsed into `weft_flow_model` and re-serialized to the identical canonical JSON. The `client` branch copies them into `contract/` with `just sync-contract`.
- The JSON Schema `openapi/flow-definition.schema.json` is validated against every fixture in both languages.
- Error codes: `openapi/error-codes.json` must be fully mapped in Dart (an exhaustive test).

---

## 12. Fuzzing (`fuzz/`, nightly, 10 minutes per target)
| Target | Input |
|---|---|
| `expr_render` | Arbitrary template string plus a JSON scope |
| `condition_eval` | Arbitrary condition JSON plus a scope |
| `definition_compile` | Arbitrary bytes, then JSON, then `FlowDefinition`, then validate and compile, then simulate a few steps |
| `declarative_spec` | Arbitrary YAML connector spec, then validate |
| `webhook_body` | Arbitrary content type and body, then normalized trigger output |
| `cron_parse` | Arbitrary cron expression plus time zone, then `next_after` |

The corpus is committed. Every crash becomes a regular regression test before it is fixed.

---

## 13. Performance and load
- **Criterion benchmarks** (pure crates): template render, condition evaluation, compile, `advance` per instruction. They run nightly; a regression above 10% (critcmp) opens an issue.
- **Worker throughput:** `cargo xtask bench-worker --executions 10000 --steps 5` runs against a local MySQL and a wiremock app. It reports executions per second, p50/p95/p99 trigger-to-finish latency, and queue lag.
- **Ingress load:** `perf/k6/webhook.js` reports webhook accept latency and the error rate at increasing request rates.
- **MVP targets (one 4-vCPU API node, one 4-vCPU worker, MySQL with 8 vCPU):** webhook accept p99 < 150 ms at 200 req/s; worker ≥ 100 action steps/s; trigger-to-first-step p95 < 2 s; zero lost executions across a forced worker restart during load.

---

## 14. Coverage and mutation testing
| Crate group | Line coverage gate | Mutation score target |
|---|---|---|
| `expr`, `domain`, `engine` | ≥ 90% | ≥ 90% killed (nightly); **no new surviving mutants in a PR diff** (`cargo mutants --in-diff`) |
| `app` | ≥ 85% | ≥ 80% (weekly) |
| `connector-sdk`, `connectors`, `infra` | ≥ 80% | Informational |
| `db`, `api`, `worker` | ≥ 75% (from integration suites) | Informational |

Coverage comes from `cargo llvm-cov nextest` across all suites. **Coverage is a signal, not a goal.** A mutation that survives in pure code is treated as a bug in the tests.

---

## 15. Determinism and flakiness policy
- **Injected everything:** `Clock` (ManualClock in tests), `IdGen` (SequentialIds), `Rng` (zero jitter), `HttpSender` (wiremock-only policy), `Mailer` (FakeMailer or Mailpit).
- **Banned in tests:** `tokio::time::sleep` (use `drain()` or `time::pause()`), real network hosts, `Utc::now()`, order-dependent assertions on unordered collections, shared mutable globals.
- `nextest` retries are **0**. A nightly "flake hunter" runs the full suite 5 times; any test that fails intermittently is quarantined within 24 h (moved to a `quarantine` nextest profile, with an issue opened) and fixed within a week. Silent `#[ignore]` is banned by `xtask`.

---

## 16. `weft-testkit` contents
```
crates/testkit/src/
├── lib.rs
├── clock.rs        # ManualClock (advance, set)
├── ids.rs          # SequentialIds (deterministic UUIDv7-shaped)
├── rng.rs          # ZeroJitter, SeededRng
├── deps.rs         # TestDeps: Deps wired with fakes; owner()/member()/viewer() actors
├── fakes/          # FakeFlowRepo, FakeExecutionRepo, FakeTriggerRepo, FakeConnectionRepo,
│                   # FakeJobQueue, FakeMailer (captures), FakeSecretBox, FakeHttp
├── contracts/      # one contract suite per port (CODEOWNED)
├── builders/       # FlowDefBuilder, StepBuilder, Cond, ExecutionBuilder, ActorBuilder
├── fixtures.rs     # fixtures::flow("order_alert.json"), fixture("slack/…")
└── wiremock.rs     # mock_connector(), fail_once_then_ok(), json_ok(), request assertions
```
```rust
let def = FlowDefBuilder::webhook()
    .decision("is_big", |d| d
        .branch("big", Cond::gt("{{ trigger.body.amount }}", 100), |s| s
            .action("notify", "slack", "post_message", json!({ "channel": "C1", "text": "hi" })))
        .otherwise(|s| s
            .action("thank", "core.email", "send", json!({ "to": "{{ trigger.body.email }}", "subject": "Thanks", "body": "…" }))))
    .delay_for("wait", "P1D")
    .build();
```

---

## 17. CI pipeline

| Stage | When | Jobs | Time budget |
|---|---|---|---|
| **1 · Fast** | Every push | `fmt --check`, `clippy -D warnings`, `xtask check-*`, `just test-unit`, `cargo deny check` | ≤ 5 min |
| **2 · Integration** | Every PR | A MySQL 8.4 service container: `db`, `http` and `scenarios` suites; `sqlx prepare --check`; OpenAPI, contract and map drift; `oasdiff breaking`; `cargo mutants --in-diff` on pure crates | ≤ 15 min |
| **3 · E2E** | Merge to `backend` | `docker compose up` (api, worker, mysql, mailpit, wiremock); the `e2e/` crate | ≤ 20 min |
| **Client** (on the `client` branch) | Every PR to `client` | Contract drift against `origin/backend`; Flutter format, analyze and test; integration smoke tests against a backend image | ≤ 15 min |
| **Nightly** | Schedule | Fuzzing, full mutation run, coverage report and trend, live connector smoke tests, load baseline, flake hunter, `cargo audit` | — |

Caching: `Swatinem/rust-cache` keyed by `Cargo.lock` plus the toolchain. The `.sqlx/` offline data means stage 1 needs no database.

---

## 18. Rules for AI agents writing tests (copied into AGENTS.md)
1. Each acceptance criterion maps to at least one test, **named in the spec**. A PR is incomplete if an AC has no test.
2. Write the test **first**, run it, and confirm it fails for the right reason. Mention that in the report.
3. Assert on **observable behaviour** (outputs, persisted state, HTTP responses, emitted requests), never on private calls or log lines.
4. Never edit a test to make it pass unless the spec changed. Say which AC changed.
5. Never update snapshots blindly. Run `cargo insta review` and describe each changed snapshot in one line in the PR.
6. Never weaken contract suites, conformance checks or property-test generators.
7. Use builders and fixtures from `weft-testkit`; add new builders there instead of hand-building large structs in tests.
8. Do not use `sleep`, wall-clock time, real hosts or `#[ignore]`.
9. For bugs: reproduce with a failing test at the lowest possible layer, then fix.
10. Proptest failures: commit the shrunk case as a named regression test, plus the `proptest-regressions` file.

---

## 19. Worked example: spec ACs to tests

`docs/specs/F-014-delay-step.md`:
| AC | Behaviour | Test(s) | Layer |
|---|---|---|---|
| AC1 | `delay for P1D` reschedules the execution 24 h later and resumes at the next step | `advance_delay_for_returns_sleep_until_now_plus_duration` | engine unit |
| | | `delay_for_resumes_next_step_after_clock_advance` | scenario |
| AC2 | `delay until` uses a mapped datetime; a past datetime continues immediately | `advance_delay_until_past_time_continues_immediately` | engine unit |
| AC3 | Duration > 30 days is rejected at publish | `validate_delay_longer_than_30_days_is_error` | domain unit |
| AC4 | An unparseable mapped datetime fails the step with `delay.invalid_datetime` | `advance_delay_until_unparseable_fails_step` | engine unit |
| AC5 | A waiting execution can be cancelled immediately | `cancel_waiting_execution_removes_job_and_marks_cancelled` | use case (fakes) + scenario |
| Invariant | Delays never break resume equivalence | covered by `resume_from_any_checkpoint_is_equivalent` (the generator includes delays) | property |
