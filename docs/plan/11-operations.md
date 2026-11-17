# 11: Operations

## 1. Environments
| Environment | Purpose | Setup |
|---|---|---|
| Local | Development and agent sessions | `just db-up` (docker compose: MySQL 8.4 and Mailpit); `just dev` runs `weft all` with auto-reload |
| CI | Tests | GitHub Actions with a MySQL 8.4 service container; compose for e2e |
| Staging | Pre-production; receives every `main` merge | The same images as production; seeded demo org; nightly live connector tests |
| Production | Customers | See §2 |

`deploy/docker-compose.yml` (dev and e2e): `mysql:8.4`, `axllent/mailpit`, `wiremock` (e2e only), `weft api`, `weft worker`.

## 2. Production deployment (initial)
- **One container image**: a multi-stage build that produces one release binary (`weft`) on a distroless or slim base. It runs as non-root on a read-only filesystem.
- **Process roles:** `weft api` with 2 or more replicas behind a load balancer (TLS terminated there), `weft worker` with 2 or more replicas, and `weft migrate` run as a pre-deploy job.
- **MySQL:** a managed MySQL 8.4 (RDS, Cloud SQL, Azure, or a self-run primary plus replica), with binlog-based point-in-time recovery, daily snapshots and 14-day retention. Avoid Vitess/PlanetScale-style sharded MySQL in early phases, because we rely on FKs in configuration tables, `SKIP LOCKED` and multi-statement transactions.
- **Hostnames:** `app.<domain>` (Flutter web, served from a CDN or static bucket), `api.<domain>` (REST and OAuth callbacks), `hooks.<domain>` (webhook ingress, routed to the same API pods but with separate rate-limit and WAF rules).
- **Zero-downtime deploys:** migrations must be backward compatible with the previous release (expand, then migrate, then contract; doc 05 §5). Workers drain gracefully on SIGTERM: they stop claiming, finish in-flight jobs (30 s grace period), and leases cover anything unfinished.
- **Self-hosted edition:** `weft all` plus MySQL, configured only through environment variables, with a Helm chart and a compose file.

## 3. Configuration (environment variables, typed and validated at startup)
```
WEFT__SERVER__ROLE=api|worker|all
WEFT__SERVER__PUBLIC_API_URL=https://api.example.com
WEFT__SERVER__PUBLIC_HOOKS_URL=https://hooks.example.com
WEFT__SERVER__ALLOWED_CLIENT_ORIGINS=https://app.example.com
WEFT__DATABASE__URL=mysql://…            WEFT__DATABASE__MAX_CONNECTIONS=32
WEFT__AUTH__JWT_SECRET=…                 WEFT__AUTH__ACCESS_TTL_SECS=900
WEFT__CRYPTO__MASTER_KEYS={"k2026a":"base64…"}   WEFT__CRYPTO__ACTIVE_KEY_ID=k2026a
WEFT__MAIL__SMTP_URL=smtps://…           WEFT__MAIL__FROM="Weft <no-reply@example.com>"
WEFT__WORKER__QUEUES=executions:32,executions_test:8,triggers:8,notifications:4,maintenance:1
WEFT__OAUTH__SLACK__CLIENT_ID=…          WEFT__OAUTH__SLACK__CLIENT_SECRET=…
WEFT__HTTP__ALLOW_PRIVATE_NETWORKS=[]
WEFT__TELEMETRY__LOG_FORMAT=json         WEFT__TELEMETRY__METRICS_ADDR=0.0.0.0:9100
```

## 4. Observability
### 4.1 Logs
JSON logs through `tracing`, with the request span carrying `request_id`, `org_id`, `user_id`, `route` and `status`, and the job span carrying `job_id`, `kind`, `execution_id`, `flow_id`, `step_key` and `attempt`. Secrets are redacted by a layer (doc 08 §4). Payload bodies are never logged.

### 4.2 Metrics (Prometheus, internal port)
| Metric | Type | Labels |
|---|---|---|
| `weft_http_requests_total` / `_duration_seconds` | counter / histogram | route, method, status |
| `weft_webhook_ingress_total` | counter | outcome (accepted, ignored, rejected) |
| `weft_jobs_claimed_total`, `weft_job_duration_seconds` | counter / histogram | queue, kind, outcome |
| `weft_queue_ready_depth`, `weft_queue_oldest_ready_age_seconds` | gauge | queue |
| `weft_executions_finished_total` | counter | status, mode |
| `weft_trigger_to_first_step_seconds` | histogram | trigger kind |
| `weft_step_duration_seconds` | histogram | connector, action, outcome |
| `weft_connector_errors_total` | counter | connector, class |
| `weft_db_pool_in_use`, `weft_db_query_duration_seconds` | gauge / histogram | pool, query name |

### 4.3 Tracing (P3)
OpenTelemetry export, with spans from webhook ingress through enqueue, claim, step and the connector's HTTP call, linked by `execution_id`.

### 4.4 Alerts (initial)
| Alert | Condition |
|---|---|
| Queue lag | `oldest_ready_age > 60 s` for 5 min (executions) |
| Dead jobs | Any `dead` job in 15 min |
| Execution failure spike | Failed / finished > 20% over 15 min (excluding test mode) |
| Webhook 5xx | > 1% for 5 min |
| Connector error spike | One connector's `Transient`/`AuthExpired` errors × 5 over baseline |
| DB | Replication lag, connections > 80%, slow queries |
| Health | `/readyz` failing on any pod for 2 min |

## 5. Capacity planning (rule of thumb)
- Each action step costs about **4 writes** (step_run insert, step_run update, execution update, usage upsert) plus job claim and update. At 100 steps/s that is about 600 writes/s, comfortable for a mid-size MySQL instance (8 vCPU, NVMe).
- Storage: an average execution with 5 steps and 2 KB of input/output per step is about 15 KB including indexes. 1M executions/day is about 15 GB/day, so retention sweeps and (P3) partitioning matter. Tune per plan.
- Scale order:
  1. More worker replicas and queue concurrency.
  2. A bigger MySQL instance.
  3. A read replica for history queries.
  4. Partition the hot tables.
  5. Batch the usage counters.
  6. Only then, consider an external queue behind the `JobQueue` port.

## 6. SLOs (initial)
| SLO | Target |
|---|---|
| API availability (non-5xx) | 99.9% monthly |
| Webhook acceptance availability | 99.95% monthly |
| Webhook accept latency | p99 < 150 ms |
| Trigger to first step | p95 < 2 s (webhook, schedule) |
| Schedule punctuality | p99 fired within 10 s of the slot |
| Durability | 0 accepted triggers lost |

## 7. Backup, disaster recovery and runbooks
- RPO ≤ 5 min (binlogs), RTO ≤ 1 h for a single-region restore. Restores are drilled quarterly into staging.
- Runbooks in `docs/runbooks/`: queue lag, dead jobs, master key rotation, OAuth app credential rotation, revoking a compromised connection, replaying a webhook backlog, restoring from backup, an org's data export or deletion.
