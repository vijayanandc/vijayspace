//! Use case: resolve the org of a request (no authentication yet: header or default org).

use weft_domain::{error_codes, ids::OrgId, model::Org};

use crate::{AppError, Deps};

/// The org named by `X-Org-ID`, or the default org (created on first use).
pub async fn resolve_org(deps: &Deps, header: Option<&str>) -> Result<Org, AppError> {
    let now = deps.clock.now();
    match header.map(str::trim).filter(|h| !h.is_empty()) {
        None => {
            deps.store
                .ensure_default_org(deps.config.default_org_id, now)
                .await
        }
        Some(raw) => {
            let id: OrgId = raw.parse().map_err(|_| {
                AppError::validation(error_codes::VALIDATION_FAILED, "Invalid X-Org-ID")
            })?;
            if id == deps.config.default_org_id {
                return deps.store.ensure_default_org(id, now).await;
            }
            deps.store.get_org(id).await?.ok_or_else(|| {
                AppError::not_found(error_codes::NOT_FOUND, "Organisation not found")
            })
        }
    }
}
