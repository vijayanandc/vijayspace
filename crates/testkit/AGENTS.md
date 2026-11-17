# weft-testkit

Test-only helpers: builders, fakes, contract suites, ManualClock, wiremock helpers.

- **Owns:** TestDeps, Fake* ports, contracts/ (human-reviewed), FlowDefBuilder, fixtures loader
- **May depend on (internal):** weft-domain, weft-expr, weft-engine, weft-connector-sdk, weft-app
- **Must never use:** weft-db (would create dev-dependency cycles)
- **Patterns:** follow `docs/plan/02-architecture.md` §5–§6 and the recipes in `docs/recipes/`.
- **Tests:** see `docs/plan/09-rust-test-strategy.md` §2 for which kind of test belongs here.
