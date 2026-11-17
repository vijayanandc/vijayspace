# Weft: backend

A Zoho Flow–style integration platform. This branch (`backend`) holds the Rust workspace (backed by MySQL 8.4) and the project plan. The Flutter app lives on the `client` branch.

> `weft` is a placeholder codename. Rename it with one search-and-replace (`weft`, `Weft`, `WEFT_`).

- **Plan:** [`docs/plan/README.md`](docs/plan/README.md). Start here.
- **Rules for AI agents and contributors:** [`AGENTS.md`](AGENTS.md)
- **Backlog:** [`docs/plan/12-roadmap-and-backlog.md`](docs/plan/12-roadmap-and-backlog.md)

## Quick start
```sh
cargo install --locked just cargo-nextest   # once
just db-up                                  # MySQL 8.4 + Mailpit (Docker)
just check                                  # fmt, clippy, unit tests
```

## Layout
```
crates/
  expr  domain  engine          pure core (no IO, no async)
  connector-sdk  connectors     connector traits + built-in connectors
  app                           use cases, ports, execution runner
  db  infra                     adapters (MySQL, crypto/HTTP/mail)
  api  worker                   delivery (axum HTTP, background loops)
  server                        composition root, `weft` binary
  testkit                       test-only helpers
migrations/  deploy/  docs/plan/
```
