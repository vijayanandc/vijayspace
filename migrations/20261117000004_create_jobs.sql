-- The MySQL job queue (claimed with FOR UPDATE SKIP LOCKED under READ COMMITTED).
CREATE TABLE jobs (
  id            BINARY(16)   NOT NULL,
  queue         VARCHAR(32)  NOT NULL,       -- executions | executions_test | triggers
  kind          VARCHAR(32)  NOT NULL,       -- run_execution | poll_trigger
  org_id        BINARY(16)   NULL,
  payload       JSON         NOT NULL,
  status        VARCHAR(16)  NOT NULL,       -- ready | running | dead
  run_at        DATETIME(6)  NOT NULL,
  attempts      INT UNSIGNED NOT NULL DEFAULT 0,
  max_attempts  INT UNSIGNED NOT NULL DEFAULT 25,
  locked_by     VARCHAR(128) NULL,
  locked_until  DATETIME(6)  NULL,
  last_error    TEXT         NULL,
  dedupe_key    VARCHAR(191) NULL,
  created_at    DATETIME(6)  NOT NULL,
  updated_at    DATETIME(6)  NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_jobs_dedupe (dedupe_key),
  KEY ix_jobs_claim (queue, status, run_at),
  KEY ix_jobs_lease (status, locked_until)
);
