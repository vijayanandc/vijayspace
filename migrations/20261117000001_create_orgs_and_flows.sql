-- Tenants, flows and immutable flow versions.
CREATE TABLE orgs (
  id          BINARY(16)   NOT NULL,
  name        VARCHAR(120) NOT NULL,
  timezone    VARCHAR(64)  NOT NULL DEFAULT 'UTC',
  is_default  BOOLEAN      NOT NULL DEFAULT FALSE,
  created_at  DATETIME(6)  NOT NULL,
  updated_at  DATETIME(6)  NOT NULL,
  PRIMARY KEY (id),
  KEY ix_orgs_default (is_default)
);

CREATE TABLE flows (
  id                   BINARY(16)   NOT NULL,
  org_id               BINARY(16)   NOT NULL,
  name                 VARCHAR(200) NOT NULL,
  description          TEXT         NULL,
  is_enabled           BOOLEAN      NOT NULL DEFAULT FALSE,
  draft_definition     JSON         NOT NULL,
  draft_revision       INT UNSIGNED NOT NULL DEFAULT 1,
  published_version_id BINARY(16)   NULL,
  created_at           DATETIME(6)  NOT NULL,
  updated_at           DATETIME(6)  NOT NULL,
  deleted_at           DATETIME(6)  NULL,
  PRIMARY KEY (id),
  KEY ix_flows_org_updated (org_id, deleted_at, updated_at),
  CONSTRAINT fk_flows_org FOREIGN KEY (org_id) REFERENCES orgs(id)
);

CREATE TABLE flow_versions (
  id          BINARY(16)   NOT NULL,
  org_id      BINARY(16)   NOT NULL,
  flow_id     BINARY(16)   NOT NULL,
  kind        VARCHAR(16)  NOT NULL,          -- published | test_snapshot
  version     INT UNSIGNED NULL,              -- 1,2,3… for published; NULL for test snapshots
  definition  JSON         NOT NULL,
  created_at  DATETIME(6)  NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_flow_versions_number (flow_id, version),
  KEY ix_flow_versions_kind (kind, created_at),
  CONSTRAINT fk_flow_versions_flow FOREIGN KEY (flow_id) REFERENCES flows(id)
);
