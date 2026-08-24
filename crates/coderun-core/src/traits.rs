use crate::error::Result;
use crate::ipc::{ContextPack, RoutingDecision, TaskRequest};
use crate::config::Config;

/// IContextBuilder — in-process | daemon | remote (spec §2 portability, ARCHITECTURE.md:209-241)
/// Reference implementation: Rust daemon (`ContextEngine`) via UDS+MessagePack.
/// Swappable behind this trait; concrete reason for Rust choice remains embedded tree-sitter crates.
pub trait IContextBuilder: Send + Sync {
    fn build_context(&self, task: &TaskRequest) -> Result<(ContextPack, RoutingDecision)>;
    fn to_yaml(pack: &ContextPack) -> Result<String>
    where
        Self: Sized;
}

/// IModelGateway — default LiteLLM (spec §2, §3 Model Router)
/// Unifies 100+ providers behind one shape; routing/fallback built in.
/// Heuristic tiering stays deterministic; no LLM call decides tier.
pub trait IModelGateway: Send + Sync {
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

/// IWorkflowEngine — external, optional (spec §5)
/// Not a runtime dependency. Separate product consuming `IContextBuilder` API.
/// Two viable choices when needed: Temporal (single-node SQLite+Litestream) or DBOS Transact.
pub trait IWorkflowEngine: Send + Sync {
    fn start_workflow(&self, task: &TaskRequest, config: &Config) -> Result<String>;
    fn get_status(&self, workflow_id: &str) -> Result<String>;
    fn is_available(&self) -> bool {
        false
    }
}

/// No-op workflow engine — v1 returns unavailable
pub struct NoopWorkflowEngine;

impl IWorkflowEngine for NoopWorkflowEngine {
    fn start_workflow(&self, _task: &TaskRequest, _config: &Config) -> Result<String> {
        Err(crate::error::CoderunError::InvalidRequest(
            "Workflow engine not configured — external orchestrator is a separate product (spec §5)".to_string(),
        ))
    }
    fn get_status(&self, _workflow_id: &str) -> Result<String> {
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
        let engine = NoopWorkflowEngine;
        assert!(!engine.is_available());
        assert!(engine
            .start_workflow(
                &crate::ipc::TaskRequest {
                    message: "test".to_string(),
                    session_id: "s".to_string(),
                    context_hints: None,
                },
                &crate::config::Config::default()
            )
            .is_err());
    }
}
