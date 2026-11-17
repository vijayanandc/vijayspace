# weft-expr

Template rendering, typed expressions, condition trees and filters (pure).

- **Owns:** minijinja wrapper, typed passthrough, the condition tree evaluator, the filter set
- **May depend on (internal):** none
- **Must never use:** async, tokio, sqlx, reqwest, axum, or any IO
- **Patterns:** follow `docs/plan/02-architecture.md` §5–§6 and the recipes in `docs/recipes/`.
- **Tests:** see `docs/plan/09-rust-test-strategy.md` §2 for which kind of test belongs here.
