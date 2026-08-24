use serde::{Deserialize, Serialize};

use crate::error::CorrelationId;

// ── Hook Types ──────────────────────────────────────────────────────────

/// Types of hooks that agents can register
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum HookType {
    PreGeneration,
    PreToolCall,
}

// ── Request Types ───────────────────────────────────────────────────────

/// Message sent from agent to daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    pub correlation_id: CorrelationId,
    pub hook_type: HookType,
    pub payload: RequestPayload,
}

/// Payload variants for incoming requests
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "PascalCase")]
pub enum RequestPayload {
    /// Pre-generation: enrich a user message with context
    MessageRewrite {
        session_id: String,
        message: String,
        context_hints: Option<ContextHints>,
    },
    /// Pre-tool: compress tool output before sending to LLM
    ToolOutput {
        tool_name: String,
        output_type: OutputType,
        content: String,
        context: Option<String>,
    },
}

// ── Response Types ──────────────────────────────────────────────────────

/// Message sent from daemon to agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub correlation_id: CorrelationId,
    pub hook_type: HookType,
    pub payload: ResponsePayload,
    pub latency_ms: u64,
    pub error: Option<String>,
}

/// Data for RewrittenMessage response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewrittenMessageData {
    pub original: String,
    pub rewritten: String,
    pub context_pack: Option<ContextPack>,
    pub routing_decision: Option<RoutingDecision>,
}

/// Payload variants for outgoing responses
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "PascalCase")]
pub enum ResponsePayload {
    /// Enriched message with context pack and routing decision
    RewrittenMessage(Box<RewrittenMessageData>),
    /// Compressed tool output
    CompressedOutput {
        original: String,
        compressed: String,
        original_tokens: usize,
        compressed_tokens: usize,
    },
    /// Pass-through on error/timeout (fail-open)
    OriginalPassthrough {
        original: String,
        reason: String,
    },
}

// ── Supporting Types ────────────────────────────────────────────────────

/// Hints from the agent about the current context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextHints {
    pub files_mentioned: Option<Vec<String>>,
    pub language: Option<String>,
}

/// Output type of a tool call
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum OutputType {
    FileRead,
    SearchResult,
    ShellOutput,
    Other,
}

/// The assembled context pack returned to agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPack {
    pub behavioral_skills: String,
    pub docs_context: String,
    pub code_context: String,
    pub token_usage: TokenUsage,
}

/// Token usage breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub total_tokens: usize,
    pub budget_remaining: usize,
    pub by_source: std::collections::HashMap<String, usize>,
}

/// Model routing decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub model: String,
    pub tier: String,
    pub scores: RoutingScores,
    pub reasoning: String,
}

/// Breakdown of routing scores
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingScores {
    pub structural: f64,
    pub semantic: f64,
    pub scope: f64,
    pub final_score: f64,
}

// ── Task Types ──────────────────────────────────────────────────────────

/// A task request for context building
#[derive(Debug, Clone)]
pub struct TaskRequest {
    pub message: String,
    pub session_id: String,
    pub context_hints: Option<ContextHints>,
}

// ── Search Types ────────────────────────────────────────────────────────

/// A single search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub path: String,
    pub line: usize,
    pub content: String,
    pub score: f64,
}

/// Collection of search results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    pub results: Vec<SearchResult>,
    pub total_count: usize,
}

// ── Knowledge Types ─────────────────────────────────────────────────────

/// A knowledge entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    pub id: Option<i64>,
    pub category: String,
    pub key: String,
    pub value: String,
    pub confidence: f64,
    pub source: String,
    pub relevance_score: Option<f64>,
}

// ── Skill Types ─────────────────────────────────────────────────────────

/// A skill match result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMatch {
    pub skill_name: String,
    pub match_score: f64,
    pub instructions: String,
    pub examples: Vec<String>,
    pub constraints: Vec<String>,
}

// ── Code Types ──────────────────────────────────────────────────────────

/// A code file in the context pack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeFile {
    pub path: String,
    pub content: String,
    pub language: String,
    pub line_range: (usize, usize),
    pub token_count: usize,
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_request_serialization() {
        let req = AgentRequest {
            correlation_id: CorrelationId::new(),
            hook_type: HookType::PreGeneration,
            payload: RequestPayload::MessageRewrite {
                session_id: "sess_123".to_string(),
                message: "implement auth".to_string(),
                context_hints: Some(ContextHints {
                    files_mentioned: Some(vec!["src/auth.rs".to_string()]),
                    language: Some("rust".to_string()),
                }),
            },
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: AgentRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.correlation_id, parsed.correlation_id);
        assert_eq!(req.hook_type, parsed.hook_type);
    }

    #[test]
    fn test_response_payload_passthrough() {
        let resp = ResponsePayload::OriginalPassthrough {
            original: "hello".to_string(),
            reason: "timeout".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: ResponsePayload = serde_json::from_str(&json).unwrap();
        match parsed {
            ResponsePayload::OriginalPassthrough { original, reason } => {
                assert_eq!(original, "hello");
                assert_eq!(reason, "timeout");
            }
            _ => panic!("Expected OriginalPassthrough"),
        }
    }

    #[test]
    fn test_output_type_serialization() {
        let types = [
            OutputType::FileRead,
            OutputType::SearchResult,
            OutputType::ShellOutput,
            OutputType::Other,
        ];
        for t in &types {
            let json = serde_json::to_string(t).unwrap();
            let parsed: OutputType = serde_json::from_str(&json).unwrap();
            assert_eq!(*t, parsed);
        }
    }

    #[test]
    fn test_context_pack_serialization() {
        let pack = ContextPack {
            behavioral_skills: "skills section".to_string(),
            docs_context: "docs section".to_string(),
            code_context: "code section".to_string(),
            token_usage: TokenUsage {
                total_tokens: 8000,
                budget_remaining: 4000,
                by_source: [
                    ("skills".to_string(), 1000),
                    ("docs".to_string(), 2000),
                    ("code".to_string(), 5000),
                ]
                .into(),
            },
        };
        let json = serde_json::to_string(&pack).unwrap();
        let parsed: ContextPack = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.token_usage.total_tokens, 8000);
    }
}
