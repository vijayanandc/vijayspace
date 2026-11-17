# weft-app

Use cases, ports (traits), Deps, Actor, AppError and the execution runner.

- **Owns:** one use case per file under <feature>/, ports/, the runner (run_slice) that drives weft-engine
- **May depend on (internal):** weft-domain, weft-expr, weft-engine, weft-connector-sdk
- **Must never use:** sqlx, axum, reqwest, lettre, tokio::spawn
- **Patterns:** follow `docs/plan/02-architecture.md` §5–§6 and the recipes in `docs/recipes/`.
- **Tests:** see `docs/plan/09-rust-test-strategy.md` §2 for which kind of test belongs here.
