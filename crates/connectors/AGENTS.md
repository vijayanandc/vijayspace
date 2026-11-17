# weft-connectors

Built-in connectors (native Rust and declarative YAML) and the declarative runtime.

- **Owns:** core.http, core.email, core.formatter, app connectors, builtin_connectors() (the only registration list)
- **May depend on (internal):** weft-connector-sdk, weft-domain, weft-expr
- **Must never use:** sqlx, axum, weft-app
- **Patterns:** follow `docs/plan/02-architecture.md` §5–§6 and the recipes in `docs/recipes/`.
- **Tests:** see `docs/plan/09-rust-test-strategy.md` §2 for which kind of test belongs here.
