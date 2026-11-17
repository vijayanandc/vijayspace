# weft-db

MySQL implementations of the repository ports, the job queue and migration embedding.

- **Owns:** MySql*Repo adapters, MySqlJobQueue (SKIP LOCKED), codecs, a testing feature with fixtures. All SQL lives here.
- **May depend on (internal):** weft-app, weft-domain
- **Must never use:** axum, reqwest, NOW() in SQL, SELECT *
- **Patterns:** follow `docs/plan/02-architecture.md` §5–§6 and the recipes in `docs/recipes/`.
- **Tests:** see `docs/plan/09-rust-test-strategy.md` §2 for which kind of test belongs here.
