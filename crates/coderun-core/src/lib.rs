// coderun-core: shared types, errors, and configuration for the AI Runtime

pub mod config;
pub mod error;
pub mod ipc;

// Re-export commonly used types
pub use config::Config;
pub use error::{CoderunError, ConfigError, CorrelationId, Result};
pub use ipc::{
    AgentRequest, AgentResponse, CodeFile, ContextHints, ContextPack, HookType,
    KnowledgeEntry, OutputType, RequestPayload, ResponsePayload, RewrittenMessageData,
    RoutingDecision, RoutingScores, SearchResult, SearchResults, SkillMatch, TaskRequest,
    TokenUsage,
};
