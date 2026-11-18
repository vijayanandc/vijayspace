-- Per-connection request limits (overrides of the connector's defaults) and daily usage counters.
ALTER TABLE connections ADD COLUMN limits JSON NULL;

CREATE TABLE connection_usage (
  org_id         BINARY(16)   NOT NULL,
  connection_id  BINARY(16)   NOT NULL,
  day            DATE         NOT NULL,      -- UTC day
  requests       INT UNSIGNED NOT NULL DEFAULT 0,
  refused        INT UNSIGNED NOT NULL DEFAULT 0,
  updated_at     DATETIME(6)  NOT NULL,
  PRIMARY KEY (connection_id, day),
  KEY ix_connection_usage_org_day (org_id, day)
);
