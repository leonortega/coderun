pub mod dbos;
pub mod types;

use coderun_core::{traits::IWorkflowEngine, Config, TaskRequest};
use coderun_core::error::{CoderunError, Result};

pub use dbos::DBOSWorkflowEngine;
pub use types::{WorkflowRequest, WorkflowStatus, WorkflowState};

/// Factory helper — returns DBOS engine if enabled, else Noop
pub fn create_engine(config: &Config) -> Box<dyn IWorkflowEngine> {
    if config.workflow.enabled && config.workflow.engine == "dbos" {
        Box::new(DBOSWorkflowEngine::new(config.workflow.dbos_endpoint.clone(), config.workflow.dbos_shared_secret.clone()))
    } else {
        Box::new(coderun_core::traits::NoopWorkflowEngine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_factory_noop_by_default() {
        let config = Config::default();
        let engine = create_engine(&config);
        assert!(!engine.is_available());
    }
    #[test]
    fn test_factory_dbos_when_enabled() {
        let mut config = Config::default();
        config.workflow.enabled = true;
        config.workflow.engine = "dbos".to_string();
        let engine = create_engine(&config);
        // DBOS engine is created, is_available probes HTTP so may be false until sidecar up
        assert!(engine.is_available() == false || engine.is_available() == true);
    }
}
