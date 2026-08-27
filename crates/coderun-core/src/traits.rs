use crate::error::Result;
use crate::ipc::{ContextPack, RoutingDecision, TaskRequest};
use crate::config::Config;

/// IContextBuilder — in-process | daemon | remote (spec §2 portability, ARCHITECTURE.md:209-241)
/// Reference implementation: Rust daemon (`ContextEngine`) via UDS+MessagePack.
/// Swappable behind this trait; concrete reason for Rust choice remains embedded tree-sitter crates.
#[async_trait::async_trait]
pub trait IContextBuilder: Send + Sync {
    async fn build_context(&self, task: &TaskRequest) -> Result<(ContextPack, RoutingDecision)>;
    fn to_yaml(pack: &ContextPack) -> Result<String>
    where
        Self: Sized;
}

/// IModelGateway — default LiteLLM (spec §2, §3 Model Router)
/// Unifies 100+ providers behind one shape; routing/fallback built in.
/// Heuristic tiering stays deterministic; no LLM call decides tier.
pub trait IModelGateway: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    fn select_model(
        &self,
        message: &str,
        file_count: usize,
        symbol_count: usize,
        knowledge_entries: usize,
        skills_matched: usize,
        token_count: usize,
        model_override: Option<&str>,
    ) -> RoutingDecision;

    fn tier_to_model(&self, tier: &str) -> String;
}

/// IWorkflowEngine — required since v0.6.0 (SQLite+Litestream, native async)
/// Single-node durability; Temporal deleted. No `block_on_in_thread` — async directly on Tokio.
/// `NoopWorkflowEngine` kept only for #[cfg(test)].
#[async_trait::async_trait]
pub trait IWorkflowEngine: Send + Sync {
    async fn start_workflow(&self, task: &TaskRequest, config: &Config) -> Result<String>;
    async fn get_status(&self, workflow_id: &str) -> Result<String>;
    async fn is_available(&self) -> bool {
        false
    }
}

/// No-op workflow engine — kept for tests only since v0.6.0
pub struct NoopWorkflowEngine;

#[async_trait::async_trait]
impl IWorkflowEngine for NoopWorkflowEngine {
    async fn start_workflow(&self, _task: &TaskRequest, _config: &Config) -> Result<String> {
        Err(crate::error::CoderunError::InvalidRequest(
            "Workflow engine not configured — enable DBOS (workflow.enabled=true) since v0.6.0".to_string(),
        ))
    }
    async fn get_status(&self, _workflow_id: &str) -> Result<String> {
        Err(crate::error::CoderunError::InvalidRequest(
            "Workflow engine not configured".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_workflow_unavailable() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let engine = NoopWorkflowEngine;
            assert!(!engine.is_available().await);
            assert!(engine
                .start_workflow(
                    &crate::ipc::TaskRequest {
                        message: "test".to_string(),
                        session_id: "s".to_string(),
                        context_hints: None,
                        repository_id: String::new(),
                        repository_path: None,
                        expected_files: None,
                    },
                    &crate::config::Config::default()
                )
                .await
                .is_err());
        });
    }
}
