-- OAuth sign-in with the app's own OAuth client ("Any app (OAuth 2.0 sign-in)"): the client secret entered when
-- starting the sign-in is kept here, encrypted, until the provider sends the browser back (at most 10 minutes;
-- expired states are removed by housekeeping). Built-in apps use the server's OAuth clients and leave it NULL.

ALTER TABLE oauth_states ADD COLUMN client_cipher VARBINARY(2048) NULL;
