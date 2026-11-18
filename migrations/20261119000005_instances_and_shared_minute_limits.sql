-- Several Weft servers can run against one database (scale out by starting more `weft worker` processes).
--
-- * `instances`: every server process writes a heartbeat here every 15 seconds. Servers share per-connection
--   concurrency caps by the number of live instances, and operators see who is running and since when.
--   Rows not seen for a day are removed by the retention sweep.
-- * `connection_usage` gains a per-minute window, so `requests_per_minute` holds across every server (it was
--   counted per process). The window is the current UTC minute; the same row lock already serialises the
--   daily count, so it costs no extra statement.

CREATE TABLE instances (
  id          BINARY(16)   NOT NULL,
  kind        VARCHAR(16)  NOT NULL,   -- api | worker | all
  host        VARCHAR(191) NOT NULL,
  version     VARCHAR(64)  NOT NULL,
  started_at  DATETIME(6)  NOT NULL,
  seen_at     DATETIME(6)  NOT NULL,
  PRIMARY KEY (id),
  KEY ix_instances_seen (seen_at)
);

ALTER TABLE connection_usage
  ADD COLUMN minute_start    DATETIME     NULL,
  ADD COLUMN minute_requests INT UNSIGNED NOT NULL DEFAULT 0;
