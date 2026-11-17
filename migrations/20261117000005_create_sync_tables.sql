-- Sync layer: record links between two connections and short per-record leases.
CREATE TABLE record_links (
  id              BINARY(16)   NOT NULL,
  org_id          BINARY(16)   NOT NULL,
  link_set        VARCHAR(64)  NOT NULL,
  a_connection_id BINARY(16)   NOT NULL,
  a_object        VARCHAR(64)  NOT NULL,
  a_record_id     VARCHAR(191) NOT NULL,
  b_connection_id BINARY(16)   NOT NULL,
  b_object        VARCHAR(64)  NOT NULL,
  b_record_id     VARCHAR(191) NOT NULL,
  b_hash          BINARY(32)   NULL,          -- hash of the values Weft last wrote to side B
  b_written_at    DATETIME(6)  NULL,
  created_at      DATETIME(6)  NOT NULL,
  updated_at      DATETIME(6)  NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_record_links_a (org_id, link_set, a_connection_id, a_object, a_record_id),
  UNIQUE KEY uq_record_links_b (org_id, link_set, b_connection_id, b_object, b_record_id)
);

CREATE TABLE sync_leases (
  org_id       BINARY(16)   NOT NULL,
  lease_key    BINARY(32)   NOT NULL,
  holder       VARCHAR(128) NOT NULL,
  locked_until DATETIME(6)  NOT NULL,
  PRIMARY KEY (org_id, lease_key)
);
