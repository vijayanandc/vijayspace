# weft-worker

Background loops: job claiming, scheduler, poller, reaper, retention, graceful shutdown.

- **Owns:** queue loops with semaphores, drain() for tests, cancellation and lease handling
- **May depend on (internal):** weft-app, weft-domain
- **Must never use:** sqlx, axum
- **Patterns:** follow `docs/plan/02-architecture.md` §5–§6 and the recipes in `docs/recipes/`.
- **Tests:** see `docs/plan/09-rust-test-strategy.md` §2 for which kind of test belongs here.
