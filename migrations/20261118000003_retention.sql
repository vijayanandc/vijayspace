-- Run history retention: an org setting (NULL = the server default) and indexes for the sweep.
ALTER TABLE orgs ADD COLUMN retention_days INT UNSIGNED NULL;
CREATE INDEX ix_executions_finished ON executions (org_id, finished_at);
CREATE INDEX ix_executions_version ON executions (flow_version_id);
CREATE INDEX ix_trigger_events_received ON trigger_events (org_id, received_at);
