//! Typed ID generation from the `IdGen` port.

use weft_domain::ids::{
    ConnectionId, ExecutionId, FlowId, FlowVersionId, JobId, LinkId, OrgId, RegistrationId,
    StepRunId, SubscriptionId, TriggerEventId,
};

use crate::Deps;

impl Deps {
    #[must_use]
    pub fn new_user_id(&self) -> weft_domain::ids::UserId {
        weft_domain::ids::UserId::from_uuid(self.ids.uuid())
    }
    #[must_use]
    pub fn new_invitation_id(&self) -> weft_domain::ids::InvitationId {
        weft_domain::ids::InvitationId::from_uuid(self.ids.uuid())
    }
    #[must_use]
    pub fn new_org_id(&self) -> OrgId {
        OrgId::from_uuid(self.ids.uuid())
    }
    #[must_use]
    pub fn new_flow_id(&self) -> FlowId {
        FlowId::from_uuid(self.ids.uuid())
    }
    #[must_use]
    pub fn new_version_id(&self) -> FlowVersionId {
        FlowVersionId::from_uuid(self.ids.uuid())
    }
    #[must_use]
    pub fn new_connection_id(&self) -> ConnectionId {
        ConnectionId::from_uuid(self.ids.uuid())
    }
    #[must_use]
    pub fn new_registration_id(&self) -> RegistrationId {
        RegistrationId::from_uuid(self.ids.uuid())
    }
    #[must_use]
    pub fn new_event_id(&self) -> TriggerEventId {
        TriggerEventId::from_uuid(self.ids.uuid())
    }
    #[must_use]
    pub fn new_execution_id(&self) -> ExecutionId {
        ExecutionId::from_uuid(self.ids.uuid())
    }
    #[must_use]
    pub fn new_step_run_id(&self) -> StepRunId {
        StepRunId::from_uuid(self.ids.uuid())
    }
    #[must_use]
    pub fn new_job_id(&self) -> JobId {
        JobId::from_uuid(self.ids.uuid())
    }
    #[must_use]
    pub fn new_link_id(&self) -> LinkId {
        LinkId::from_uuid(self.ids.uuid())
    }
    #[must_use]
    pub fn new_subscription_id(&self) -> SubscriptionId {
        SubscriptionId::from_uuid(self.ids.uuid())
    }

    /// 32 random bytes as hex (from two UUIDv4-style draws of the ID generator).
    #[must_use]
    pub fn random_hex32(&self) -> String {
        let a = self.ids.uuid().simple().to_string();
        let b = uuid::Uuid::new_v4().simple().to_string();
        format!("{}{}", &a[16..], &b[..16])
    }
}
