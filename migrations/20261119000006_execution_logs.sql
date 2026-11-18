-- The detailed log of a run: every HTTP call its steps made (method, URL with secrets removed, status, time,
-- sizes, and the start of the answer when the app refused), and events that leave no step row (a request held
-- back by a connection's limits, a retry being scheduled, a worker that stopped mid-step).
--
-- Partitioned by day like the rest of run history (a log line takes its run's day), so the retention sweep drops
-- it with the run's partition (crates/db/src/partitions.rs) and the space returns to the operating system.

CREATE TABLE execution_logs (
  id            BINARY(16)        NOT NULL,
  org_id        BINARY(16)        NOT NULL,
  execution_id  BINARY(16)        NOT NULL,
  day           DATE              NOT NULL,
  step_key      VARCHAR(64)       NULL,
  attempt       SMALLINT UNSIGNED NULL,
  level         VARCHAR(8)        NOT NULL,   -- info | warn | error
  kind          VARCHAR(16)       NOT NULL,   -- http | limit | retry | event
  message       VARCHAR(500)      NOT NULL,
  details       JSON              NULL,
  at            DATETIME(6)       NOT NULL,
  PRIMARY KEY (id, day),
  KEY ix_execution_logs_execution (execution_id, at)
)
PARTITION BY RANGE COLUMNS(day) (PARTITION pmax VALUES LESS THAN (MAXVALUE));
