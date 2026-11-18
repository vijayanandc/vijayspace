-- The retention sweep deletes an org's old test snapshots; this index starts with the org.
CREATE INDEX ix_flow_versions_org_kind ON flow_versions (org_id, kind, created_at);
