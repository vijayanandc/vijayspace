-- Connections (encrypted secrets) and short-lived OAuth states.
CREATE TABLE connections (
  id                BINARY(16)       NOT NULL,
  org_id            BINARY(16)       NOT NULL,
  connector_key     VARCHAR(64)      NOT NULL,
  name              VARCHAR(200)     NOT NULL,
  config            JSON             NOT NULL,     -- non-secret settings (base URL, data centre, region…)
  secret_ciphertext VARBINARY(16384) NOT NULL,     -- nonce ‖ AES-256-GCM ciphertext ‖ tag
  secret_key_id     VARCHAR(32)      NOT NULL,
  status            VARCHAR(16)      NOT NULL,     -- active | needs_reauth | error
  created_at        DATETIME(6)      NOT NULL,
  updated_at        DATETIME(6)      NOT NULL,
  deleted_at        DATETIME(6)      NULL,
  PRIMARY KEY (id),
  KEY ix_connections_org (org_id, connector_key, deleted_at),
  CONSTRAINT fk_connections_org FOREIGN KEY (org_id) REFERENCES orgs(id)
);

CREATE TABLE oauth_states (
  state_hash       BINARY(32)       NOT NULL,      -- SHA-256 of the random state
  org_id           BINARY(16)       NOT NULL,
  connector_key    VARCHAR(64)      NOT NULL,
  connection_id    BINARY(16)       NULL,          -- set when re-authorizing
  connection_name  VARCHAR(200)     NOT NULL,
  config           JSON             NOT NULL,      -- non-secret fields chosen before redirect
  verifier_cipher  VARBINARY(512)   NULL,          -- encrypted PKCE verifier
  return_to        VARCHAR(2048)    NOT NULL,
  expires_at       DATETIME(6)      NOT NULL,
  created_at       DATETIME(6)      NOT NULL,
  PRIMARY KEY (state_hash),
  KEY ix_oauth_states_expiry (expires_at)
);
