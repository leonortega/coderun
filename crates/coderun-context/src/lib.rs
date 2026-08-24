use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use coderun_core::{ContextHints, ContextPack, RoutingDecision, TaskRequest, TokenUsage};
use coderun_events::{EventBus, RuntimeEvent, TokenCounts};
use coderun_knowledge::KnowledgeHub;
use coderun_repo_intel::RepositoryIntelligence;
use coderun_router::{ModelRouter, RouterConfig, RoutingRequest};
use tracing::{debug, info};

// ── Configuration ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ContextConfig {
    pub max_tokens: usize,
    pub max_files: usize,
    pub max_lines_per_file: usize,
    pub cache_order: Vec<String>,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_tokens: 12000,
            max_files: 20,
            max_lines_per_file: 500,
            cache_order: vec![
                "behavioral_skills".to_string(),
                "docs_context".to_string(),
                "code_context".to_string(),
            ],
        }
    }
}

// ── Context Engine ──────────────────────────────────────────────────────

pub struct ContextEngine {
    repo_intel: Arc<Mutex<RepositoryIntelligence>>,
    knowledge_hub: Arc<Mutex<KnowledgeHub>>,
    model_router: ModelRouter,
    event_bus: EventBus,
    config: ContextConfig,
    /// Session fingerprints for deduplication (session_id → set of content hashes)
    session_fingerprints: Arc<Mutex<HashMap<String, HashSet<String>>>>,
}

impl ContextEngine {
    /// Create a new Context Engine
    pub fn new(
        repo_intel: RepositoryIntelligence,
        knowledge_hub: KnowledgeHub,
        event_bus: EventBus,
        config: ContextConfig,
    ) -> Self {
        let router_config = RouterConfig::default();
        let model_router = ModelRouter::new(router_config);

        Self {
            repo_intel: Arc::new(Mutex::new(repo_intel)),
            knowledge_hub: Arc::new(Mutex::new(knowledge_hub)),
            model_router,
            event_bus,
            config,
            session_fingerprints: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Build context for a task — the main entry point
    pub fn build_context(
        &self,
        request: &TaskRequest,
    ) -> Result<(ContextPack, RoutingDecision), String> {
        let start = Instant::now();
        let correlation_id = coderun_core::CorrelationId::new();

        debug!(
            correlation_id = %correlation_id,
            message = %request.message,
            "Building context"
        );

        // Initialize token budget
        let mut token_budget = self.config.max_tokens;
        let mut token_usage_by_source: HashMap<String, usize> = HashMap::new();

        // Step 1: Search code via Repository Intelligence
        let code_context = self.search_code(&request.message, &request.context_hints)?;

        // Step 2: Retrieve knowledge via Knowledge Hub
        let knowledge_context = self.retrieve_knowledge(&request.message)?;

        // Step 3: Match skills via Knowledge Hub → Skill Engine
        let skills_context = self.match_skills(&request.message)?;

        // Step 4: Assemble context pack with cache-aware ordering
        let (context_pack, total_tokens) = self.assemble_context_pack(
            &skills_context,
            &knowledge_context,
            &code_context,
            &mut token_budget,
            &mut token_usage_by_source,
        );

        // Step 5: Select model via Model Router
        let routing_decision = self.select_model(
            &request.message,
            &code_context,
            &knowledge_context,
            &skills_context,
            total_tokens,
        );

        // Step 6: Emit ContextBuilt event
        let latency_ms = start.elapsed().as_millis() as u64;
        self.event_bus.emit(RuntimeEvent::ContextBuilt {
            correlation_id: correlation_id.clone(),
            token_counts: TokenCounts {
                total: total_tokens,
                by_source: token_usage_by_source.clone(),
            },
            file_count: context_pack.code_context.lines().count(),
            latency_ms,
        });

        info!(
            correlation_id = %correlation_id,
            total_tokens = total_tokens,
            latency_ms = latency_ms,
            "Context built"
        );

        Ok((context_pack, routing_decision))
    }

    /// Search code via Repository Intelligence
    fn search_code(
        &self,
        query: &str,
        context_hints: &Option<ContextHints>,
    ) -> Result<String, String> {
        let repo_intel = self.repo_intel.lock().map_err(|e| format!("Lock error: {}", e))?;

        let mut results = Vec::new();

        // Search by query
        if let Ok(search_results) = repo_intel.search_text(query, None, 10) {
            for result in &search_results.results {
                // Get file content with limited lines
                if let Ok(content) = repo_intel.get_file_content(
                    &result.path,
                    Some((result.line.saturating_sub(5), result.line + 10)),
                ) {
                    results.push(format!("// {}:{}\n{}", result.path, result.line, content));
                }
            }
        }

        // Also search for files mentioned in context hints
        if let Some(hints) = context_hints {
            if let Some(files) = &hints.files_mentioned {
                for file in files {
                    if let Ok(content) = repo_intel.get_file_content(file, Some((1, self.config.max_lines_per_file))) {
                        results.push(format!("// {}\n{}", file, content));
                    }
                }
            }
        }

        Ok(results.join("\n\n"))
    }

    /// Retrieve knowledge via Knowledge Hub
    fn retrieve_knowledge(&self, query: &str) -> Result<String, String> {
        let knowledge_hub = self.knowledge_hub.lock().map_err(|e| format!("Lock error: {}", e))?;

        let entries = knowledge_hub.retrieve_knowledge(query, None, 10)?;

        let formatted: Vec<String> = entries
            .iter()
            .map(|e| format!("// [{}] {}: {}", e.category, e.key, e.value))
            .collect();

        Ok(formatted.join("\n"))
    }

    /// Match skills via Knowledge Hub
    fn match_skills(&self, query: &str) -> Result<String, String> {
        let knowledge_hub = self.knowledge_hub.lock().map_err(|e| format!("Lock error: {}", e))?;

        let matches = knowledge_hub.match_skills(query, 5);

        let formatted: Vec<String> = matches
            .iter()
            .map(|m| {
                format!(
                    "# {}\n{}\n\nExamples:\n{}\n\nConstraints:\n{}",
                    m.skill_name,
                    m.instructions,
                    m.examples.join("\n"),
                    m.constraints.join("\n")
                )
            })
            .collect();

        Ok(formatted.join("\n\n---\n\n"))
    }

    /// Assemble the context pack with cache-aware ordering
    fn assemble_context_pack(
        &self,
        skills_context: &str,
        knowledge_context: &str,
        code_context: &str,
        token_budget: &mut usize,
        token_usage_by_source: &mut HashMap<String, usize>,
    ) -> (ContextPack, usize) {
        let mut total_tokens = 0;

        // Section 1: behavioral_skills (20% budget)
        let skills_budget = (*token_budget as f64 * 0.20) as usize;
        let skills_tokens = estimate_tokens(skills_context);
        let (skills_content, skills_used) = if skills_tokens <= skills_budget {
            (skills_context.to_string(), skills_tokens)
        } else {
            truncate_to_tokens(skills_context, skills_budget)
        };
        token_usage_by_source.insert("behavioral_skills".to_string(), skills_used);
        total_tokens += skills_used;

        // Section 2: docs_context (15% budget)
        let docs_budget = (*token_budget as f64 * 0.15) as usize;
        let docs_tokens = estimate_tokens(knowledge_context);
        let (docs_content, docs_used) = if docs_tokens <= docs_budget {
            (knowledge_context.to_string(), docs_tokens)
        } else {
            truncate_to_tokens(knowledge_context, docs_budget)
        };
        token_usage_by_source.insert("docs_context".to_string(), docs_used);
        total_tokens += docs_used;

        // Section 3: code_context (55% budget)
        let code_budget = (*token_budget as f64 * 0.55) as usize;
        let code_tokens = estimate_tokens(code_context);
        let (code_content, code_used) = if code_tokens <= code_budget {
            (code_context.to_string(), code_tokens)
        } else {
            truncate_to_tokens(code_context, code_budget)
        };
        token_usage_by_source.insert("code_context".to_string(), code_used);
        total_tokens += code_used;

        // Remaining budget for metadata
        let metadata_used = *token_budget - total_tokens;
        token_usage_by_source.insert("metadata".to_string(), metadata_used);

        let context_pack = ContextPack {
            behavioral_skills: skills_content,
            docs_context: docs_content,
            code_context: code_content,
            token_usage: TokenUsage {
                total_tokens,
                budget_remaining: token_usage_by_source.get("metadata").copied().unwrap_or(0),
                by_source: token_usage_by_source.clone(),
            },
        };

        (context_pack, total_tokens)
    }

    /// Select model via Model Router
    fn select_model(
        &self,
        message: &str,
        code_context: &str,
        knowledge_context: &str,
        skills_context: &str,
        token_count: usize,
    ) -> RoutingDecision {
        let request = RoutingRequest {
            message: message.to_string(),
            file_count: code_context.lines().count(),
            symbol_count: 0, // Would need repo-intel integration
            knowledge_entries: knowledge_context.lines().count(),
            skills_matched: skills_context.matches("---").count() + 1,
            token_count,
            model_override: None,
        };

        self.model_router.select_model(&request)
    }

    /// Clear session fingerprint (e.g., on daemon restart)
    pub fn clear_session_fingerprint(&self, session_id: &str) {
        if let Ok(mut fingerprints) = self.session_fingerprints.lock() {
            fingerprints.remove(session_id);
        }
    }

    /// Serialize context pack to YAML
    pub fn to_yaml(pack: &ContextPack) -> Result<String, String> {
        serde_yaml::to_string(pack).map_err(|e| format!("Failed to serialize to YAML: {}", e))
    }
}

// ── Token Estimation ────────────────────────────────────────────────────

/// Estimate token count (rough: 1 token ≈ 4 chars)
fn estimate_tokens(text: &str) -> usize {
    let char_count = text.len();
    let word_count = text.split_whitespace().count();
    let by_chars = char_count / 4;
    let by_words = (word_count as f64 * 1.3) as usize;
    by_chars.max(by_words)
}

/// Truncate text to fit within a token budget
fn truncate_to_tokens(text: &str, budget: usize) -> (String, usize) {
    let char_budget = budget * 4; // Approximate chars per token
    
    if text.len() <= char_budget {
        let tokens = estimate_tokens(text);
        (text.to_string(), tokens)
    } else {
        // Truncate at line boundaries
        let mut result = String::new();
        let mut current_tokens = 0;
        
        for line in text.lines() {
            let line_tokens = estimate_tokens(line);
            if current_tokens + line_tokens > budget {
                result.push_str("... [truncated]\n");
                current_tokens += 1;
                break;
            }
            result.push_str(line);
            result.push('\n');
            current_tokens += line_tokens;
        }
        
        (result, current_tokens)
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        assert!(estimate_tokens("hello world") > 0);
        assert!(estimate_tokens("") == 0);
        assert!(estimate_tokens(&"a".repeat(100)) > 20);
    }

    #[test]
    fn test_truncate_to_tokens_within_budget() {
        let text = "short text";
        let (result, tokens) = truncate_to_tokens(text, 100);
        assert_eq!(result, text);
        assert!(tokens <= 100);
    }

    #[test]
    fn test_truncate_to_tokens_exceeds_budget() {
        // Create text that exceeds a small budget
        let text = (0..100).map(|i| format!("line {} with some content", i)).collect::<Vec<_>>().join("\n");
        let (result, tokens) = truncate_to_tokens(&text, 20);
        // Should be truncated and within reasonable bounds
        assert!(tokens <= 25); // Allow some overhead
        assert!(result.contains("truncated") || result.lines().count() < 100);
    }

    #[test]
    fn test_context_config_default() {
        let config = ContextConfig::default();
        assert_eq!(config.max_tokens, 12000);
        assert_eq!(config.max_files, 20);
        assert_eq!(config.max_lines_per_file, 500);
        assert_eq!(config.cache_order.len(), 3);
    }

    #[test]
    fn test_cache_ordering() {
        let config = ContextConfig::default();
        assert_eq!(config.cache_order[0], "behavioral_skills");
        assert_eq!(config.cache_order[1], "docs_context");
        assert_eq!(config.cache_order[2], "code_context");
    }

    #[test]
    fn test_token_budget_allocation() {
        let max_tokens = 12000;
        let skills_budget = (max_tokens as f64 * 0.20) as usize;
        let docs_budget = (max_tokens as f64 * 0.15) as usize;
        let code_budget = (max_tokens as f64 * 0.55) as usize;
        let metadata_budget = max_tokens - skills_budget - docs_budget - code_budget;

        assert_eq!(skills_budget, 2400);
        assert_eq!(docs_budget, 1800);
        assert_eq!(code_budget, 6600);
        assert_eq!(metadata_budget, 1200);
    }

    #[test]
    fn test_context_pack_yaml_serialization() {
        let pack = ContextPack {
            behavioral_skills: "skills content".to_string(),
            docs_context: "docs content".to_string(),
            code_context: "code content".to_string(),
            token_usage: TokenUsage {
                total_tokens: 100,
                budget_remaining: 50,
                by_source: HashMap::new(),
            },
        };

        let yaml = ContextEngine::to_yaml(&pack).unwrap();
        assert!(yaml.contains("behavioral_skills"));
        assert!(yaml.contains("docs_context"));
        assert!(yaml.contains("code_context"));
    }
}
