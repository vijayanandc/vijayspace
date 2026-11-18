-- Invitation emails counted per day (per org and per inviting user), and OAuth sign-ins for
-- connections that finish through a signed-in request: the provider's callback is kept here
-- (encrypted) with a one-time finish token for the browser that came back from the provider.
CREATE TABLE send_counters (
  scope  VARCHAR(64)   NOT NULL,                  -- org:<uuid> | user:<uuid>
  day    DATE          NOT NULL,
  sent   INT UNSIGNED  NOT NULL DEFAULT 0,
  PRIMARY KEY (scope, day)
);

ALTER TABLE oauth_states
  ADD COLUMN callback_cipher VARBINARY(8192) NULL,
  ADD COLUMN finish_hash     BINARY(32)      NULL;
