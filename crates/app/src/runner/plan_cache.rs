//! Compiled plans cached per (immutable) flow version.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use weft_domain::{ids::FlowVersionId, model::FlowVersion};
use weft_engine::{Plan, compile};

use crate::AppError;

const MAX_ENTRIES: usize = 1_000;

/// Process-local cache; versions never change, so entries never go stale.
#[derive(Debug, Default)]
pub struct PlanCache {
    plans: Mutex<HashMap<FlowVersionId, Arc<Plan>>>,
}

impl PlanCache {
    /// The plan for a version, compiling it on first use.
    pub fn get(&self, version: &FlowVersion) -> Result<Arc<Plan>, AppError> {
        if let Some(plan) = self
            .plans
            .lock()
            .ok()
            .and_then(|m| m.get(&version.id).cloned())
        {
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
            map.insert(version.id, plan.clone());
        }
        Ok(plan)
    }
}
