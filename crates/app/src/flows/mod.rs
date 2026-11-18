//! Flow use cases: CRUD, drafts, validation, publish, enable/disable, test and manual runs.

mod lifecycle;
mod manage;
mod runs;

/// Public URL helpers for trigger registrations.
pub mod manage_urls {
    pub use super::manage::{
        app_event_url, display_kind, empty_sample, has_unpublished_changes, webhook_url,
    };
}

pub use lifecycle::{publish_flow, set_enabled, validate_draft};
pub use manage::{
    FlowView, SavedDraft, create_flow, delete_flow, get_flow, list_flows, list_versions,
    save_draft, start_listening, update_flow,
};
pub use runs::{manual_run, test_run, test_step};
