-- Trigger registrations, provider subscriptions, trigger events, executions and step runs.
CREATE TABLE trigger_registrations (
  id              BINARY(16)  NOT NULL,
  org_id          BINARY(16)  NOT NULL,
  flow_id         BINARY(16)  NOT NULL,
  kind            VARCHAR(16) NOT NULL,     -- webhook | schedule | poll | instant | manual
  token           CHAR(43)    NOT NULL,     -- URL capability (base64url of 32 random bytes)
  secret          CHAR(32)    NOT NULL,     -- verify token for app events / webhook secret header
  is_active       BOOLEAN     NOT NULL DEFAULT FALSE,
  listening_until DATETIME(6) NULL,
  next_fire_at    DATETIME(6) NULL,         -- schedules, polls and reconciliation
  state           JSON        NOT NULL,     -- poll cursor, last fired slot…
  sample          JSON        NULL,
  created_at      DATETIME(6) NOT NULL,
  updated_at      DATETIME(6) NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_trigger_registrations_token (token),
  UNIQUE KEY uq_trigger_registrations_flow (flow_id),
  KEY ix_trigger_registrations_due (kind, is_active, next_fire_at)
);

CREATE TABLE app_subscriptions (
  id              BINARY(16)   NOT NULL,
  org_id          BINARY(16)   NOT NULL,
  registration_id BINARY(16)   NOT NULL,
  connection_id   BINARY(16)   NOT NULL,
  connector_key   VARCHAR(64)  NOT NULL,
  external_id     VARCHAR(191) NOT NULL,
  data            JSON         NOT NULL,
  status          VARCHAR(16)  NOT NULL,    -- active | failed
  expires_at      DATETIME(6)  NULL,
  renew_after     DATETIME(6)  NULL,
  last_error      TEXT         NULL,
  created_at      DATETIME(6)  NOT NULL,
  updated_at      DATETIME(6)  NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_app_subscriptions_registration (registration_id),
  KEY ix_app_subscriptions_renew (status, renew_after)
);

CREATE TABLE trigger_events (                      -- high volume: no FKs
  id              BINARY(16)   NOT NULL,
  org_id          BINARY(16)   NOT NULL,
  flow_id         BINARY(16)   NOT NULL,
  registration_id BINARY(16)   NOT NULL,
  dedupe_key      VARCHAR(191) NULL,
  status          VARCHAR(16)  NOT NULL,      -- accepted | ignored | sample
  payload         JSON         NOT NULL,
  received_at     DATETIME(6)  NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_trigger_events_dedupe (registration_id, dedupe_key),
  KEY ix_trigger_events_flow (org_id, flow_id, received_at)
);

CREATE TABLE executions (                          -- high volume: no FKs
  id                  BINARY(16)    NOT NULL,
  org_id              BINARY(16)    NOT NULL,
  flow_id             BINARY(16)    NOT NULL,
  flow_version_id     BINARY(16)    NOT NULL,
  trigger_event_id    BINARY(16)    NULL,
  mode                VARCHAR(16)   NOT NULL,
  rerun_of            BINARY(16)    NULL,
  status              VARCHAR(16)   NOT NULL,
  trigger_data        JSON          NOT NULL,
  run_state           JSON          NOT NULL,
  waiting_until       DATETIME(6)   NULL,
  task_count          INT UNSIGNED  NOT NULL DEFAULT 0,
  error_code          VARCHAR(64)   NULL,
  error_message       VARCHAR(2000) NULL,
  failed_step_key     VARCHAR(64)   NULL,
  possible_duplicate  BOOLEAN       NOT NULL DEFAULT FALSE,
  cancel_requested    BOOLEAN       NOT NULL DEFAULT FALSE,
  queued_at           DATETIME(6)   NOT NULL,
  started_at          DATETIME(6)   NULL,
  finished_at         DATETIME(6)   NULL,
  updated_at          DATETIME(6)   NOT NULL,
  PRIMARY KEY (id),
  KEY ix_executions_flow (org_id, flow_id, queued_at),
  KEY ix_executions_org_status (org_id, status, queued_at),
  KEY ix_executions_org_queued (org_id, queued_at)
);

CREATE TABLE step_runs (                           -- high volume: no FKs
  id            BINARY(16)        NOT NULL,
  org_id        BINARY(16)        NOT NULL,
  execution_id  BINARY(16)        NOT NULL,
  seq           INT UNSIGNED      NOT NULL,
  step_key      VARCHAR(64)       NOT NULL,
  kind          VARCHAR(16)       NOT NULL,
  attempt       SMALLINT UNSIGNED NOT NULL,
  status        VARCHAR(20)       NOT NULL,
  input         JSON              NULL,
  output        JSON              NULL,
  error         JSON              NULL,
  warnings      JSON              NULL,
  started_at    DATETIME(6)       NOT NULL,
  finished_at   DATETIME(6)       NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_step_runs_seq (execution_id, seq),
  KEY ix_step_runs_execution (execution_id, step_key)
);

CREATE TABLE usage_counters (
  org_id      BINARY(16)   NOT NULL,
  period      CHAR(7)      NOT NULL,             -- 'YYYY-MM'
  tasks_used  INT UNSIGNED NOT NULL DEFAULT 0,
  executions  INT UNSIGNED NOT NULL DEFAULT 0,
  updated_at  DATETIME(6)  NOT NULL,
  PRIMARY KEY (org_id, period)
);
