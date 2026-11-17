-- Debounce looks up the queued execution of a burst through its trigger event.
CREATE INDEX ix_executions_trigger_event ON executions (trigger_event_id);
