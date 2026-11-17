# weft-server

Composition root and the weft binary: config, wiring, CLI, telemetry.

- **Owns:** Settings (figment), wiring of Deps, clap roles, tracing setup; HTTP, scenario and isolation test suites in tests/
- **May depend on (internal):** weft-api, weft-worker, weft-db, weft-infra, weft-connectors, weft-app, weft-domain
- **Must never use:** business logic
- **Patterns:** follow `docs/plan/02-architecture.md` §5–§6 and the recipes in `docs/recipes/`.
- **Tests:** see `docs/plan/09-rust-test-strategy.md` §2 for which kind of test belongs here.
