# weft-engine

Compiles a FlowDefinition into a Plan (IR) and interprets it purely.

- **Owns:** compile(), Plan/Instr, advance(), record_result(), retry decisions, the test Simulator (feature = "testing")
- **May depend on (internal):** weft-domain, weft-expr
- **Must never use:** async, tokio, sqlx, reqwest, axum, or any IO. Changes here need human review.
- **Patterns:** follow `docs/plan/02-architecture.md` §5–§6 and the recipes in `docs/recipes/`.
- **Tests:** see `docs/plan/09-rust-test-strategy.md` §2 for which kind of test belongs here.
