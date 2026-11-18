# Operating Weft

How to configure, run, scale and look after a Weft installation. What is built is in [`STATUS.md`](STATUS.md);
the REST contract is in [`api/README.md`](api/README.md).

## Configuration: `config.properties` with environment overrides

Weft reads its settings the way the Appblocks servers do:

- **The file.** `WEFT_CONFIG_PATH` names a properties file; without it, `./config.properties` is read when it
  exists. Each `WEFT_FOO_BAR` variable is written in the file as `foo_bar = value` (`DATABASE_URL` →
  `database_url`). The format is Java-style: `key = value` or `key: value`, with `#` and `!` comments.
- **Precedence.** A set (non-empty) environment variable always wins over the file, so one file can be shared
  and a single value changed per server.
- **Startup checks.** A key that is set twice, or a line that is not `key = value`, stops the server. A key
  that no setting reads (usually a typo) is logged as a warning.
- **Secrets.** The file may hold secrets (`master_keys`, `logto_app_secret`, `zeptomail_api_key`): keep it
  readable by the server's user only (`chmod 600`). The server warns when every user can read it.
  `config.properties` is in `.gitignore`.

Every setting is listed in the README's configuration table. Logging uses `WEFT_LOG`
(default `info,sqlx=warn`; `RUST_LOG` wins). Set `WEFT_LOG_FORMAT=json` for one JSON object per line, for log
shipping.

## Deploy

**Files.**
- `Dockerfile` builds the `weft` image: one binary for `serve`, `api`, `worker` and `migrate`.
- The web client's image is built on the `client` branch (`docker build -t weft-web .`). It holds Caddy, the
  client's files, and the reverse proxy rules for `/api`, `/hooks`, `/oauth`, `/healthz` and `/readyz`.
- `deploy/production/compose.yml` puts them together on one host:
  - a one-shot `weft-migrate`;
  - 2 `weft-api` and 2 `weft-worker` replicas;
  - Caddy with automatic TLS for `WEFT_SITE_ADDRESS`.

**Steps.**
1. MySQL 8.4, preferably managed (backups, binary logs, `max_connections` sized as below).
   - Create the database and a user.
   - Over a network, use `?ssl-mode=verify-identity` in `database_url`.
2. `deploy/production/config.properties.example` → `config.properties` (chmod 600).
   - Set the database, the URLs, fresh master keys, Logto, `owner_emails`, `alert_emails` and the mail provider.
   - Generate a key with `openssl rand -base64 32`.
3. In Logto, register:
   - the web client's redirect URI `https://<host>/callback` and its post-sign-out URI;
   - the server app used for introspection.
4. Build the images, then `WEFT_SITE_ADDRESS=flow.example.com WEFT_LOGTO_ENDPOINT=https://tenant.logto.app docker compose up -d`
   in `deploy/production/`.
5. Sign in as an `owner_emails` address: that person owns the default org and is staff. Create each customer's
   org and invite its people.
6. Watch `GET /api/v1/ops/status` (or the Platform page). Scale with `docker compose up -d --scale weft-worker=N`.

**Upgrades.** Pull or build the new images and run `docker compose up -d`. `weft-migrate` applies new migrations
before the servers start. Migrations are additive, so the old servers keep working while the new ones start.

**Kubernetes or systemd.** Run the same commands as separate units: one `weft migrate` per release, N × `weft api`
behind the load balancer, M × `weft worker`. Each receives SIGTERM on stop and gets 40 s to finish.

## Processes

`weft serve` runs the API and a worker in one process (small installs). For production, run them separately:

| Command | Does | Scale by |
|---|---|---|
| `weft api` | HTTP API, webhooks, OAuth callbacks | more instances behind the load balancer |
| `weft worker` | runs, polls, schedules, subscription renewals, the retention sweep | more instances (no load balancer) |
| `weft migrate` | applies migrations and exits | run once per release, before the new servers start |

Every process stops cleanly on SIGTERM or Ctrl+C:

- The API stops accepting requests and finishes the ones in flight.
- A worker stops claiming jobs and gives running ones 30 s to finish.
- A job that is still running after that keeps its lease until it expires (90 s), and then another worker
  runs it again.

## Scaling out

When runs wait in the queue, start another `weft worker` against the same database. Nothing else needs to change.

**How work is shared between servers:**

- **Jobs** are claimed from MySQL with `FOR UPDATE SKIP LOCKED` and a lease, so each job runs on one worker.
  A worker that dies loses its lease, and the job is run again elsewhere.
- **Schedules, polls and subscription renewals** are claimed the same way. A schedule slot has a dedupe key, so
  it fires once however many workers race for it.
- **The retention sweep and partition maintenance** run on one server at a time, under a database lease.
- **OAuth token refreshes** take a per-connection lease in the database. A server that finds a refresh already
  running waits for that server's new token (up to 15 s) instead of spending the refresh token again. This
  matters because single-use refresh tokens would otherwise invalidate each other.
- **Connection limits:**
  - `requests_per_minute` and `daily_requests` are counted in the database, so they hold across every server.
  - `max_concurrent` is split between the live servers that run jobs (`worker` or `serve`): each allows
    `max_concurrent / those servers` at once, and always at least 1. An API-only server uses the same share
    for its few app calls (test steps, pickers); if that briefly goes over the app's own limit, the app's
    429 is retried.
- **Heartbeats.** Every process writes a heartbeat to the `instances` table every 15 s. A process missing
  for 45 s no longer counts; one that shuts down cleanly leaves at once.
- **Webhook rate limit.** Each URL is limited to 600 requests a minute per API instance. This guards that
  instance, so the total grows as you add API instances.

**Database connections.** Each process opens up to `WEFT_DB_POOL_SIZE` connections (default: worker concurrency
+ 16). The pools of all servers together must stay below MySQL's `max_connections` (the MySQL default is 151).
Otherwise new connections fail with "Too many connections": runs then stall until their leases expire, and they
are retried. Each server compares its pool with MySQL's free connections at startup and logs a warning when they
do not fit.

- **Example:** 4 workers and 2 API servers at concurrency 16 can open 6 × 32 = 192 connections.
- **What to do:** raise `max_connections` (e.g. 500), or lower `WEFT_DB_POOL_SIZE` (at least concurrency + 4).

**Watching the lag.** `GET /api/v1/ops/status` is for owners and admins of the default org. It shows:

- the live servers;
- per queue, how many jobs are due and how long the oldest has waited (`lag_seconds`);
- the counts of running and dead jobs.

Add a worker when `lag_seconds` keeps growing.

### Measured (2026-11-18)

`scripts/scale_out.py` sends the same burst of runs through one API instance and lets 1, 2 and then 4 workers
drain it. Each run makes one HTTP call. The setup:

- an unoptimised (debug) build;
- MySQL 8.4;
- one 4-core container;
- worker concurrency 8;
- 1,200 runs;
- a mock app that answers in 250 ms.

| Workers | Drain time | Runs/s | Average wait in queue | Worst wait | Failed |
|---|---|---|---|---|---|
| 1 | 42.0 s | 28.6 | 20.3 s | 40.1 s | 0 |
| 2 | 21.4 s | 56.0 | 9.9 s | 19.2 s | 0 |
| 4 | 11.5 s | 104.6 | 5.2 s | 9.8 s | 0 |

**When the app is slow (the usual case),** each worker adds the same throughput, and the queue lag halves each
time the number of workers doubles.

**When the app answers at once** (no latency, concurrency 16, 2,000 runs), the limit moves:

- One worker already keeps up with what one API instance takes in, about 170–200 runs/s.
- More workers still cut the worst wait from 6.5 s to 3.8 s.
- Past that point, add API instances for intake.

## Alerts and what to do

Every 5 minutes one server checks the platform, as the Appblocks tenant monitor does. New conditions are logged
(`WARN`) and emailed to `WEFT_ALERT_EMAILS`. A condition that persists is repeated once an hour. Staff see the
current list in `GET /api/v1/ops/status` and on the client's Platform page.

| Alert | What to do |
|---|---|
| No worker is running (critical) | Start `weft worker` (or check why it exits: its log, the database, `WEFT_DB_POOL_SIZE`). |
| Jobs in a queue have waited N min | Start another `weft worker`. If workers are idle, look for errors in their log (database, lease timeouts). |
| N jobs stopped retrying | Their worker kept failing on them. Check the server logs around that time; the affected runs failed and can be re-run. |
| N runs failed in an org | Open History in that org, filter by Failed. An app outage clears on its own (retries); a mapping error needs the flow fixed. |
| A connection needs signing in again | Reconnect it under Connections (the app revoked or expired its token). Its runs fail until then. |

A step that fails with `app.outcome_unknown` timed out on a change that is not safe to repeat. Check the app: if the
change is missing, use **Retry from failed step**; if it is there, re-run is not needed.

## Run history and retention

History is kept for at most 30 days, the platform cap. Each org can choose fewer (1–30); `WEFT_RETENTION_DAYS`
is the default for orgs without a setting.

**Day partitions.**

- `executions`, `step_runs` and `trigger_events` are partitioned by day, as the Appblocks servers do with
  `request_log`. A run's steps always sit in the run's own partition.
- The sweep runs every 15 minutes, under a lease. Each time, it:
  1. creates day partitions a week ahead, by splitting the `pmax` catch-all;
  2. drops the partitions older than 30 days. Dropping a partition deletes its file, so the space goes back to
     the operating system at once (a `DELETE` never shrinks an InnoDB table).
- A run that is still unfinished when its partition is about to be dropped is first failed with
  `execution.expired`, and its jobs are removed.
- The sweep warns when rows sit in `pmax`. That only happens if maintenance stopped for more than a week.

**Orgs that keep fewer than 30 days.** Their older finished runs are deleted in batches. That space is reused
by new rows, and it returns to the operating system when the day's partition is dropped.

**Other cleanup.** Housekeeping also removes:

- dead jobs;
- expired OAuth states, leases and invitations;
- old usage and send counters;
- dedupe keys and unused test snapshots older than 30 days;
- server instances not seen for a day.

## Backups

- Back up the whole MySQL database, for example with a nightly `mysqldump --single-transaction --routines` or
  a managed snapshot, plus binary logs for point-in-time recovery. The job queue lives in the same database, so
  a restore brings back queued work too.
- Keep the master keys (`WEFT_MASTER_KEYS`) separately from the backups. Connection secrets are encrypted with
  them, and a backup without its keys cannot decrypt any connection.
