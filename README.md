# Weft: backend

A Zoho Flow–style integration platform. This branch (`backend`) holds the Rust workspace (backed by MySQL 8.4), the REST contract and the project plan. The React web client lives on the `client` branch.

> `weft` is a placeholder codename. Rename it with one search-and-replace (`weft`, `Weft`, `WEFT_`).

- **What works today, and known gaps:** [`docs/STATUS.md`](docs/STATUS.md)
- **REST contract (used by the client):** [`docs/api/README.md`](docs/api/README.md)
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
| `WEFT_CLIENT_ORIGINS` | `http://localhost:5173` | CORS origins (comma-separated) |
| `WEFT_MASTER_KEYS`, `WEFT_ACTIVE_KEY_ID` | development key | `id:base64key,…` for encrypting connection secrets. **Set in production.** |
| `WEFT_ALLOW_PRIVATE_NETWORKS` | `false` | Let HTTP steps reach private/loopback addresses (local testing only) |
| `WEFT_HTTP_PROXY` | none | Egress proxy for calls to apps (`HTTP(S)_PROXY` is ignored on purpose). With a proxy set, Weft no longer checks the addresses that hostnames resolve to (only literal IPs and redirects), so the proxy must block private destinations itself. |
| `WEFT_SMTP_URL`, `WEFT_MAIL_FROM` | log only | Email step delivery (Mailpit: `smtp://localhost:1025`) |
| `WEFT_WORKER_CONCURRENCY` | `16` | Jobs processed in parallel |
| `WEFT_RETENTION_DAYS` | `30` | Days finished runs are kept (orgs can set their own, 1–3650) |
| `WEFT_SCRIPT_ISOLATION` | `process` | Run custom functions in child processes under OS memory/CPU limits; `thread` runs them inside the server (development only) |
| `WEFT_SCRIPT_MEMORY_MB` | `512` | Memory a custom-function process may use (64–65536; its 64 MB stack comes on top) |
| `WEFT_OAUTH_ZOHO_CRM_CLIENT_ID` / `_SECRET` | — | Zoho CRM OAuth app (redirect URI: `<public url>/oauth/callback`) |
| `WEFT_OAUTH_AUTODESK_FORMA_CLIENT_ID` / `_SECRET` | — | Autodesk APS app (same redirect URI) |

The binary has subcommands: `weft serve` (API + worker), `weft api`, `weft worker` and `weft migrate`.

Authentication is not wired yet (Logto is planned, doc 13 §6). Every request runs as the default org, or as the org in the `X-Org-ID` header. **Do not expose this server to the internet.**

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
