//! Compiled plans cached per (immutable) flow version.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use weft_domain::{ids::FlowVersionId, model::FlowVersion};
use weft_engine::{Plan, compile};

use crate::AppError;

const MAX_ENTRIES: usize = 1_000;

/// A compiled plan with its version number (versions are immutable, so both stay valid).
pub type CachedPlan = (Arc<Plan>, Option<u32>);

/// Process-local cache; versions never change, so entries never go stale.
#[derive(Debug, Default)]
pub struct PlanCache {
    plans: Mutex<HashMap<FlowVersionId, CachedPlan>>,
}

impl PlanCache {
    /// The plan for a version, compiling it on first use.
    pub fn get(&self, version: &FlowVersion) -> Result<Arc<Plan>, AppError> {
        if let Some((plan, _)) = self.cached(version.id) {
            return Ok(plan);
        }
        let plan = Arc::new(
            compile(&version.definition)
                .map_err(|e| AppError::internal(format!("compile {}: {e}", version.id)))?,
        );
        if let Ok(mut map) = self.plans.lock() {
            if map.len() >= MAX_ENTRIES {
                map.clear();
            }
            map.insert(version.id, (plan.clone(), version.version));
        }
        Ok(plan)
    }

    /// A compiled plan and its version number, without loading the version (each slice of a run asks).
    pub fn cached(&self, id: FlowVersionId) -> Option<CachedPlan> {
        self.plans.lock().ok().and_then(|m| m.get(&id).cloned())
    }
}
