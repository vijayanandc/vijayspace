# weft-http

The production `HttpSender` used by every connector: reqwest (rustls) plus an SSRF guard.

- **Owns:** `ReqwestSender`, `SsrfPolicy` (blocked IP ranges, literal-IP and redirect checks), response size cap.
- **May depend on (internal):** weft-connector-sdk
- **Must never use:** sqlx, axum, weft-app. Changes to the SSRF guard need human review (🔒).
- **Tests:** blocked ranges are table-tested; wiremock tests use `SsrfPolicy::allow_private()`.
