# weft-domain

IDs, entities, FlowDefinition and its validation, schedules, permissions, error codes (pure).

- **Owns:** define_id!, Timestamp, FlowDefinition v1 + upcasting, validate_for_publish, ScheduleSpec, the permission matrix, error_codes.rs
- **May depend on (internal):** weft-expr
- **Must never use:** async, tokio, sqlx, reqwest, axum, or any IO
- **Patterns:** follow `docs/plan/02-architecture.md` §5–§6 and the recipes in `docs/recipes/`.
- **Tests:** see `docs/plan/09-rust-test-strategy.md` §2 for which kind of test belongs here.
