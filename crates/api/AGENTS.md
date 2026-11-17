# weft-api

axum router, handlers, DTOs, extractors, problem+json mapping and OpenAPI.

- **Owns:** routes/<feature>/<use_case>.rs (thin handlers), dto.rs files, ROUTE_PERMISSIONS, openapi.rs
- **May depend on (internal):** weft-app, weft-domain
- **Must never use:** sqlx, weft-db, weft-infra, business logic in handlers
- **Patterns:** follow `docs/plan/02-architecture.md` §5–§6 and the recipes in `docs/recipes/`.
- **Tests:** see `docs/plan/09-rust-test-strategy.md` §2 for which kind of test belongs here.
