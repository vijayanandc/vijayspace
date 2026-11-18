> **Moved.** The server now lives in [`vijayanandc/appblocks_flow`](https://github.com/vijayanandc/appblocks_flow) (`main`), with this branch's history. This branch is no longer updated; make changes there.

# Weft: backend

A Zoho Flow–style integration platform. This branch (`backend`) holds the Rust workspace (backed by MySQL 8.4), the REST contract and the project plan. The React web client lives on the `client` branch.

> `weft` is a placeholder codename. Rename it with one search-and-replace (`weft`, `Weft`, `WEFT_`).

- **What works today, and known gaps:** [`docs/STATUS.md`](docs/STATUS.md)
- **REST contract (used by the client):** [`docs/api/README.md`](docs/api/README.md)
- **Running it in production (config file, scaling out, retention, backups):** [`docs/operations.md`](docs/operations.md)
- **Plan:** [`docs/plan/README.md`](docs/plan/README.md). The current MVP is [doc 13](docs/plan/13-mvp-zoho-crm-appblocks.md).
- **Rules for AI agents and contributors:** [`AGENTS.md`](AGENTS.md)

## Run it locally
```sh
cargo install --locked just cargo-nextest   # once
just db-up                                  # MySQL 8.4 + Mailpit (Docker)
just serve                                  # migrates, then API + worker on :8080
```
Then start the web client from a checkout of the `client` branch:
```sh
npm ci
npm run dev                                 # http://localhost:5173 (proxies /api and /hooks to :8080)
```
Open http://localhost:5173, create a flow with a **Webhook** trigger, add steps, **Publish**, switch it **On**, and send a request to the webhook URL shown in the trigger panel:
```sh
curl -X POST http://localhost:8080/hooks/<token> -H 'content-type: application/json' -d '{"amount": 150, "name": "Ann"}'
```
The run appears under **History** with each step's input and output.

### Configuration (environment variables)
| Variable | Default | Purpose |
|---|---|---|
| `WEFT_DATABASE_URL` | `mysql://weft:weft@127.0.0.1:3306/weft` | MySQL database (also the job queue) |
| `WEFT_BIND` | `0.0.0.0:8080` | Listen address |
| `WEFT_PUBLIC_URL` | `http://localhost:8080` | Base of webhook and OAuth callback URLs |
| `WEFT_CLIENT_ORIGINS` | `http://127.0.0.1:3000,http://localhost:5173` | CORS origins (comma-separated) |
| `WEFT_AUTH` | `logto` when `WEFT_LOGTO_ENDPOINT` is set; required otherwise | `off` turns sign-in off: every request acts as an owner (**development only**; refused unless `WEFT_BIND` is a loopback address). `just serve` uses `off` on `127.0.0.1:8080`. |
| `WEFT_LOGTO_ENDPOINT` | — | The Logto tenant, e.g. `https://<tenant>.logto.app` (the one the Appblocks apps use) |
| `WEFT_LOGTO_APP_ID`, `WEFT_LOGTO_APP_SECRET` | — | This server's confidential (traditional web) Logto app, used to introspect opaque tokens. **The secret only in the environment.** |
| `WEFT_LOGTO_CLIENT_APP_ID` | — | The web client's single-page Logto app (only its tokens are accepted). Its redirect URI is `<client origin>/callback`, e.g. `http://127.0.0.1:3000/callback` |
| `WEFT_LOGTO_RESOURCE` | none | An API resource: the client then gets JWTs, verified locally (JWKS, `iss`, `aud`); without it, opaque tokens are introspected |
| `WEFT_LOGTO_SCOPES` | `openid,profile,email` | Scopes an opaque token must carry (the client also asks for `offline_access`) |
| `WEFT_LOGTO_CACHE_SECS` | `300` | How long a checked token is trusted before Logto is asked again (never past its expiry) |
| `WEFT_OWNER_EMAILS` | none | Verified emails that become owners of the default org when they sign in (comma-separated). Owners and admins of the default org are the operator's **staff**. |
| `WEFT_ALERT_EMAILS` | none | Staff who get alert emails (comma-separated): a worker missing, queue lag, dead jobs, failing orgs, connections to reconnect. Without it, alerts are only logged. |
| `WEFT_ORG_CREATION` | `staff` | Who may create organisations: `staff`, or `anyone` signed in. The Logto tenant is shared with other apps, so keep `staff`: staff set up each customer's org and invite its people. |
| `WEFT_APP_URL` | `http://127.0.0.1:3000` | Where people open Weft (links in invitation emails) |
| `WEFT_MASTER_KEYS`, `WEFT_ACTIVE_KEY_ID` | development key on a loopback `WEFT_BIND` only | `id:base64key,…` for encrypting connection secrets. Required otherwise: the server does not start without them. |
| `WEFT_ALLOW_PRIVATE_NETWORKS` | `false` | Let HTTP steps reach private/loopback addresses (local testing only) |
| `WEFT_HTTP_PROXY` | none | Egress proxy for calls to apps (`HTTP(S)_PROXY` is ignored on purpose). With a proxy set, Weft no longer checks the addresses that hostnames resolve to (only literal IPs and redirects), so the proxy must block private destinations itself. |
| `WEFT_ZEPTOMAIL_API_KEY`, `WEFT_ZEPTOMAIL_API_URL` | — | Email through ZeptoMail (as the Appblocks servers send it; key only in the environment). Takes precedence over SMTP. |
| `WEFT_SMTP_URL`, `WEFT_MAIL_FROM` | log only | Email (invitations and the email step) over SMTP (Mailpit: `smtp://localhost:1025`). `WEFT_MAIL_FROM` is `Name <address>`, also used for ZeptoMail |
| `WEFT_WORKER_CONCURRENCY` | `16` | Jobs processed in parallel (1–256) |
| `WEFT_DB_POOL_SIZE` | worker concurrency + 16 | Database connections per process (at least worker concurrency + 4) |
| `WEFT_RETENTION_DAYS` | `30` | Days finished runs are kept (orgs can set their own); 1–30: history is kept 30 days at most |
| `WEFT_SCRIPT_ISOLATION` | `process` | Run custom functions in child processes under OS memory/CPU limits; `thread` runs them inside the server (development only) |
| `WEFT_SCRIPT_MEMORY_MB` | `512` | Memory a custom-function process may use (64–65536; its 64 MB stack comes on top) |
| `WEFT_OAUTH_ZOHO_CRM_CLIENT_ID` / `_SECRET` | — | Zoho CRM OAuth app (redirect URI: `<public url>/oauth/callback`) |
| `WEFT_OAUTH_AUTODESK_FORMA_CLIENT_ID` / `_SECRET` | — | Autodesk APS app (same redirect URI) |
| `WEFT_ZOHO_ACCOUNTS_URL` | Zoho's data centres | **Testing only:** the one Zoho accounts server for sign-in and tokens, e.g. `scripts/zoho_mock.py` on `http://127.0.0.1:9200`. Must be HTTPS unless on this machine (it receives the client secret). |

The binary has subcommands: `weft serve` (API + worker), `weft api`, `weft worker` and `weft migrate`.

Sign-in is through Logto (doc 13 §6): every request carries a Logto access token and acts in the org named by its `X-Org-ID` header. `WEFT_AUTH=off` turns sign-in off for local development only: every request then acts as an owner, and the server refuses to start unless it listens on a loopback address. See [`docs/operations.md`](docs/operations.md) for running it in production.

## Develop
```sh
just check      # fmt, clippy (-D warnings), unit tests
just test       # all tests, including MySQL scenario tests (needs `just db-up`)
```
Scenario tests (`crates/server/tests/scenarios/`) create a fresh database per test, drive the real router, drain the worker and use a manual clock.

## Layout
```
crates/
  expr  domain  engine          pure core (no IO, no async)
  connector-sdk  connectors     connector traits + built-in connectors (HTTP, Formatter, Email,
                                Zoho CRM, Appblocks, Autodesk Forma)
  http                          SSRF-guarded HTTP sender
  app                           use cases, ports, execution runner, sync layer, recipes
  db  infra                     adapters (MySQL; clock, ids, crypto, mail)
  api  worker                   delivery (axum HTTP, background loops)
  server                        composition root, `weft` binary
  testkit                       test-only helpers
migrations/  deploy/  docs/
```
