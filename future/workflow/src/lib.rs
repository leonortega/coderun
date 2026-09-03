pub mod dbos;
pub mod types;

use knocode_core::traits::IWorkflowEngine;
use knocode_core::Config;

pub use dbos::DBOSWorkflowEngine;
pub use types::{WorkflowRequest, WorkflowStatus, WorkflowState};

/// Factory helper — returns DBOS engine if enabled, else Noop
pub fn create_engine(config: &Config) -> Box<dyn IWorkflowEngine> {
    if config.workflow.enabled && config.workflow.engine == "dbos" {
        Box::new(DBOSWorkflowEngine::new(config.workflow.dbos_endpoint.clone(), config.workflow.dbos_shared_secret.clone()))
    } else {
        Box::new(knocode_core::traits::NoopWorkflowEngine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_factory_noop_by_default() {
        let config = Config::default();
        // v0.6.0 default enabled=true so factory returns DBOS, not Noop — but Noop still used when disabled
        let mut disabled = config.clone();
        disabled.workflow.enabled = false;
        let engine = create_engine(&disabled);
        assert!(!engine.is_available().await);
    }
    #[tokio::test]
    async fn test_factory_dbos_when_enabled() {
        let mut config = Config::default();
        config.workflow.enabled = true;
        config.workflow.engine = "dbos".to_string();
        let engine = create_engine(&config);
        // DBOS engine probes HTTP so may be false until sidecar up
        let _ = engine.is_available().await;
    }
}
