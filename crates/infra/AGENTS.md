# weft-infra

Non-database adapters: crypto, password hashing, JWT, mailer, SSRF-guarded HTTP, clock, IDs.

- **Owns:** SecretBox (AES-256-GCM), argon2, JWT issuer, lettre mailer, the reqwest sender with the SSRF resolver, SystemClock
- **May depend on (internal):** weft-app, weft-domain, weft-connector-sdk
- **Must never use:** sqlx, axum
- **Patterns:** follow `docs/plan/02-architecture.md` §5–§6 and the recipes in `docs/recipes/`.
- **Tests:** see `docs/plan/09-rust-test-strategy.md` §2 for which kind of test belongs here.
