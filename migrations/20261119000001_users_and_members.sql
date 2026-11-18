-- Sign-in (Logto) and tenancy (doc 13 §6): users created on first sign-in, their memberships and
-- roles per org, open invitations by email, and who created each flow and connection (members may
-- delete only their own).
CREATE TABLE users (
  id              BINARY(16)    NOT NULL,
  logto_subject   VARCHAR(191)  NOT NULL,
  email           VARCHAR(320)  NULL,              -- lower-cased
  email_verified  BOOLEAN       NOT NULL DEFAULT FALSE,
  name            VARCHAR(200)  NULL,
  created_at      DATETIME(6)   NOT NULL,
  updated_at      DATETIME(6)   NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_users_subject (logto_subject),
  KEY ix_users_email (email)
);

CREATE TABLE org_members (
  org_id      BINARY(16)   NOT NULL,
  user_id     BINARY(16)   NOT NULL,
  role        VARCHAR(16)  NOT NULL,               -- owner | admin | member | viewer
  created_at  DATETIME(6)  NOT NULL,
  updated_at  DATETIME(6)  NOT NULL,
  PRIMARY KEY (org_id, user_id),
  KEY ix_org_members_user (user_id),
  CONSTRAINT fk_org_members_org FOREIGN KEY (org_id) REFERENCES orgs(id) ON DELETE CASCADE,
  CONSTRAINT fk_org_members_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE TABLE org_invitations (
  id          BINARY(16)    NOT NULL,
  org_id      BINARY(16)    NOT NULL,
  email       VARCHAR(320)  NOT NULL,              -- lower-cased
  role        VARCHAR(16)   NOT NULL,
  invited_by  BINARY(16)    NULL,
  created_at  DATETIME(6)   NOT NULL,
  expires_at  DATETIME(6)   NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_org_invitations_email (org_id, email),
  KEY ix_org_invitations_email (email),
  CONSTRAINT fk_org_invitations_org FOREIGN KEY (org_id) REFERENCES orgs(id) ON DELETE CASCADE
);

ALTER TABLE flows ADD COLUMN created_by BINARY(16) NULL;
ALTER TABLE connections ADD COLUMN created_by BINARY(16) NULL;
ALTER TABLE oauth_states ADD COLUMN created_by BINARY(16) NULL;
