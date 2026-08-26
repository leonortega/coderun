// coderun-core: shared types, errors, and configuration for the AI Runtime

pub mod config;
pub mod error;
pub mod ipc;
pub mod secrets;
pub mod traits;

// Re-export commonly used types
pub use config::Config;
pub use error::{CoderunError, ConfigError, CorrelationId, Result};
pub use secrets::{contains_secret, redact_secrets};
pub use traits::{IContextBuilder, IModelGateway, IWorkflowEngine, NoopWorkflowEngine};
pub use ipc::{
    AgentRequest, AgentResponse, CodeFile, ContextHints, ContextPack, HookType,
    KnowledgeEntry, OutputType, RequestPayload, ResponsePayload, RetrievalStatus,
    RewrittenMessageData, RoutingDecision, RoutingScores, SearchResult, SearchResults,
    SkillMatch, TaskRequest, TokenUsage, repository_id_from_path,
};
