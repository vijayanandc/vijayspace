# Dependencies

One line per third-party crate: what it is for. Add a line here in the same change that adds a dependency (AGENTS.md rule). Versions are pinned in the workspace `Cargo.toml`.

| Crate | Used by | Why |
|---|---|---|
| serde, serde_json (`preserve_order`) | all | JSON definitions, API bodies, connector payloads; key order is kept for stable output and hashing |
| thiserror | all | Typed error enums |
| anyhow | server | Error plumbing in the binary only |
| uuid (v4, v7) | domain, infra | Time-ordered IDs (v7) through the `Ids` port; v4 in tests |
| chrono, chrono-tz | expr, domain, app | Time values and IANA time zones for schedules and date filters |
| croner | domain | Cron expression parsing and next-fire computation |
| minijinja (`json`, `urlencode`, `fuel`, `loop_controls`) | expr | Sandboxed templates with a fuel limit |
| sha2, hmac, hex, base64 | expr, app, api, connector-sdk, infra | Hash/HMAC filters, sync value hashes, PKCE challenges, encodings |
| rand | connector-sdk | Random tokens (OAuth state, PKCE verifier, webhook tokens) |
| regex | expr, connectors | `regex_replace` filter, `matches_regex` operator, text extraction |
| async-trait | app, connector-sdk, connectors, db | Object-safe async ports and connectors |
| futures | worker, server (tests) | `join_all` in concurrency tests; declared in the worker for stream helpers |
| tokio, tokio-util | app, worker, server | Runtime, `Notify`/`Semaphore`; cancellation tokens for graceful shutdown |
| axum, tower, tower-http | api | HTTP server, middleware (CORS, body limit, tracing) |
| http-body-util | server (tests) | Reading response bodies in scenario tests |
| sqlx (`mysql`, `json`, `chrono`, `uuid`, `migrate`) | db | MySQL access and embedded migrations |
| reqwest (`rustls`) | http | Outbound HTTP behind the SSRF guard |
| aes-gcm | infra | Encrypting connection secrets (nonces from its `OsRng`) |
| lettre | infra | SMTP for the email action |
| tracing, tracing-subscriber | all | Structured logs |
| clap | server | `weft` subcommands |
| url, urlencoding | http, app, connector-sdk, connectors | URL parsing and encoding |
| proptest | engine (tests) | Property tests for the interpreter |
| wiremock | connectors, server (tests) | Mock third-party APIs in tests |
