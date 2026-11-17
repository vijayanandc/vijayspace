//! Ports: Trigger registrations, received events and app subscriptions.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use weft_domain::{
    ids::{ConnectionId, FlowId, OrgId, RegistrationId, SubscriptionId},
    model::TriggerRegistration,
};

use super::Res;

/// A provider-side webhook subscription Weft registered.
#[derive(Debug, Clone, PartialEq)]
pub struct Subscription {
    pub id: SubscriptionId,
    pub org_id: OrgId,
    pub registration_id: RegistrationId,
    pub connection_id: ConnectionId,
    pub connector: String,
    pub external_id: String,
    pub data: Value,
    pub status: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub renew_after: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[async_trait]
pub trait TriggerRepo: Send + Sync {
    async fn get_registration_by_flow(&self, flow: FlowId) -> Res<Option<TriggerRegistration>>;
    async fn get_registration_by_token(&self, token: &str) -> Res<Option<TriggerRegistration>>;
    async fn get_registration(&self, id: RegistrationId) -> Res<Option<TriggerRegistration>>;
    /// Updates kind, activity, next fire time and state.
    async fn update_registration(&self, reg: &TriggerRegistration, now: DateTime<Utc>) -> Res<()>;
    async fn set_listening(
        &self,
        id: RegistrationId,
        until: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Res<()>;
    async fn set_sample(&self, id: RegistrationId, sample: &Value, now: DateTime<Utc>) -> Res<()>;
    async fn set_state(&self, id: RegistrationId, state: &Value, now: DateTime<Utc>) -> Res<()>;
    /// Claims due registrations of `kinds` (SKIP LOCKED). Each claimed row's `next_fire_at` is moved
    /// to `lease_until` so a crash cannot lose it; the returned rows carry the original due time.
    async fn take_due(
        &self,
        kinds: &[&str],
        now: DateTime<Utc>,
        lease_until: DateTime<Utc>,
        limit: u32,
    ) -> Res<Vec<TriggerRegistration>>;
    async fn set_next_fire(
        &self,
        id: RegistrationId,
        next: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Res<()>;
    async fn upsert_subscription(&self, sub: &Subscription, now: DateTime<Utc>) -> Res<()>;
    async fn get_subscription(&self, registration: RegistrationId) -> Res<Option<Subscription>>;
    async fn delete_subscription(&self, registration: RegistrationId) -> Res<()>;
    /// Claims subscriptions due for renewal: their `renew_after` moves to `claim_until`, so another
    /// worker does not renew the same channel; a successful renewal then sets the real next time.
    async fn claim_due_subscriptions(
        &self,
        now: DateTime<Utc>,
        claim_until: DateTime<Utc>,
        limit: u32,
    ) -> Res<Vec<Subscription>>;
}
