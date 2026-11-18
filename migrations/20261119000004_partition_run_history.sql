-- Run history is day-partitioned, as the Appblocks servers do with request_log and their queue history: dropping
-- a day's partition unlinks its file and returns the space to the operating system at once, while a DELETE
-- never shrinks an InnoDB tablespace. History is kept at most 30 days (the platform cap); the retention sweep
-- drops older partitions and keeps day partitions ready a week ahead (crates/db/src/partitions.rs).
--
-- * `day` is the UTC day the run was queued (a step run takes its run's day, so a run and its steps always
--   share a partition; a trigger event takes the day it arrived).
-- * MySQL requires the partition key in every unique key: primary keys become (id, day), and step runs keep
--   (execution_id, seq) unique through their run's day. Trigger-event dedupe must hold across days (a
--   reconciliation poll can deliver an event version again days later), so it moves to its own small table.
-- * `pmax` is the MAXVALUE catch-all: a row always has a partition even if maintenance lapses. The sweep
--   reorganises it into day partitions; it is never dropped.
-- * No foreign keys: partitioned tables cannot have them (none of these tables did).

CREATE TABLE trigger_dedupe (
  registration_id BINARY(16)   NOT NULL,
  dedupe_key      VARCHAR(191) NOT NULL,
  event_id        BINARY(16)   NOT NULL,
  created_at      DATETIME(6)  NOT NULL,
  PRIMARY KEY (registration_id, dedupe_key),
  KEY ix_trigger_dedupe_event (event_id),
  KEY ix_trigger_dedupe_created (created_at)
);

INSERT IGNORE INTO trigger_dedupe (registration_id, dedupe_key, event_id, created_at)
  SELECT registration_id, dedupe_key, id, received_at FROM trigger_events WHERE dedupe_key IS NOT NULL;

ALTER TABLE executions ADD COLUMN day DATE NULL;
UPDATE executions SET day = DATE(queued_at);
ALTER TABLE executions MODIFY day DATE NOT NULL, DROP PRIMARY KEY, ADD PRIMARY KEY (id, day)
  PARTITION BY RANGE COLUMNS(day) (PARTITION pmax VALUES LESS THAN (MAXVALUE));

ALTER TABLE step_runs ADD COLUMN day DATE NULL;
UPDATE step_runs s JOIN executions e ON e.id = s.execution_id SET s.day = e.day;
UPDATE step_runs SET day = DATE(started_at) WHERE day IS NULL;
ALTER TABLE step_runs MODIFY day DATE NOT NULL, DROP PRIMARY KEY, ADD PRIMARY KEY (id, day),
  DROP INDEX uq_step_runs_seq, ADD UNIQUE KEY uq_step_runs_seq (execution_id, seq, day)
  PARTITION BY RANGE COLUMNS(day) (PARTITION pmax VALUES LESS THAN (MAXVALUE));

ALTER TABLE trigger_events ADD COLUMN day DATE NULL;
UPDATE trigger_events SET day = DATE(received_at);
ALTER TABLE trigger_events MODIFY day DATE NOT NULL, DROP PRIMARY KEY, ADD PRIMARY KEY (id, day),
  DROP INDEX uq_trigger_events_dedupe, ADD KEY ix_trigger_events_dedupe (registration_id, dedupe_key)
  PARTITION BY RANGE COLUMNS(day) (PARTITION pmax VALUES LESS THAN (MAXVALUE));
