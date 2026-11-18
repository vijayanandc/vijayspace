-- Strict review: the periodic housekeeping and renewal queries use indexes. (Emails keep their collation: the
-- queries that match an email add an exact `COLLATE utf8mb4_bin` comparison, see crates/db/src/tenancy.rs.)

-- Housekeeping deletes rows past a time; without these they scan whole tables.
ALTER TABLE sync_leases ADD KEY ix_sync_leases_locked_until (locked_until);
ALTER TABLE org_invitations ADD KEY ix_org_invitations_expires (expires_at);
ALTER TABLE connection_usage ADD KEY ix_connection_usage_day (day);
ALTER TABLE send_counters ADD KEY ix_send_counters_day (day);

-- Subscription renewal orders by renew_after across statuses.
ALTER TABLE app_subscriptions ADD KEY ix_app_subscriptions_renew_after (renew_after);
