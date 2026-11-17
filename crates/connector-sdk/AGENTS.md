# weft-connector-sdk

Connector, Action, Trigger and OptionSource traits, manifests, field schema, ConnectorError.

- **Owns:** the connector traits, ActionSpec/Field builders, ConnectorError::from_http, the HttpSender trait, the conformance suite
- **May depend on (internal):** weft-domain
- **Must never use:** sqlx, axum, weft-app
- **Patterns:** follow `docs/plan/02-architecture.md` §5–§6 and the recipes in `docs/recipes/`.
- **Tests:** see `docs/plan/09-rust-test-strategy.md` §2 for which kind of test belongs here.
