use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use coderun_core::{ContextHints, ContextPack, RoutingDecision, TaskRequest, TokenUsage};
use coderun_events::{EventBus, RuntimeEvent, TokenCounts};
use coderun_knowledge::KnowledgeHub;
use coderun_repo_intel::RepositoryIntelligence;
use coderun_router::{ModelRouter, RouterConfig, RoutingRequest};
use tracing::{debug, info, warn};

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

    /// Build context for a task — the main entry point (sync; never via EventBus per spec §2)
    pub fn build_context(
        &self,
        request: &TaskRequest,
    ) -> Result<(ContextPack, RoutingDecision), String> {
        let start = Instant::now();
        let correlation_id = coderun_core::CorrelationId::new();

        debug!(
            correlation_id = %correlation_id,
            session_id = %request.session_id,
            message = %request.message,
            "Building context"
        );

        // Initialize token budget
        let mut token_budget = self.config.max_tokens;
        let mut token_usage_by_source: HashMap<String, usize> = HashMap::new();

        // Step 1: Search code via Repository Intelligence (dedup-eligible) — capture SearchResult.score for provenance (TASK-007)
        let (raw_code, code_scored) = self.search_code_scored(&request.message, &request.context_hints)?;
        let code_context = self.dedup_content(&request.session_id, &raw_code);

        // Step 2: Retrieve knowledge via Knowledge Hub — capture BM25 vs engram provenance
        let (raw_knowledge, knowledge_scored) = self.retrieve_knowledge_scored(&request.message)?;
        let knowledge_context = self.dedup_content(&request.session_id, &raw_knowledge);

        // Step 3: Match skills via Knowledge Hub → Skill Engine — capture tag overlap score
        let (raw_skills, skills_scored) = self.match_skills_scored(&request.message)?;
        let skills_context = self.dedup_content(&request.session_id, &raw_skills);

        // Step 4: Assemble context pack with cache-aware ordering + frozen-prefix + reversible compression
        let (mut context_pack, total_tokens) = self.assemble_context_pack(
            &skills_context,
            &knowledge_context,
            &code_context,
            &mut token_budget,
            &mut token_usage_by_source,
        );
        // TASK-007/008/009: stable artifact + provenance (deterministic) — real scores
        {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(request.message.as_bytes());
            hasher.update(self.config.max_tokens.to_be_bytes());
            hasher.update(self.config.cache_order.join(",").as_bytes());
            hasher.update(self.repository_state().as_bytes());
            let task_hash = format!("{:x}", hasher.finalize())[..16].to_string();
            context_pack.metadata.task_hash = task_hash;
            context_pack.metadata.correlation_id = correlation_id.to_string();
            let repo_state = self.repository_state();
            context_pack.metadata.repository_state = repo_state.clone();
            context_pack.repository_state = repo_state.clone();
            // Provenance: real scores (TASK-007) — BM25 vs symbol match vs skill_engine:tag overlap
            for (skill_name, score, specificity) in &skills_scored {
                // Only include skills that actually contributed to context (dedup may have emptied but scored still valid)
                if !skills_context.is_empty() || !skill_name.is_empty() {
                    context_pack.provenance.push(coderun_core::ipc::ContextProvenance {
                        path: skill_name.clone(),
                        source: "skills".to_string(),
                        retriever: "skill_engine".to_string(),
                        score: *score,
                        reason: format!("tag overlap specificity={:.2}", specificity),
                    });
                }
            }
            for entry in &knowledge_scored {
                let retriever = if entry.source == "engram" { "engram" } else { "tantivy" };
                let reason = if entry.source == "engram" { "memory".to_string() } else { "bm25".to_string() };
                context_pack.provenance.push(coderun_core::ipc::ContextProvenance {
                    path: entry.key.clone(),
                    source: "docs".to_string(),
                    retriever: retriever.to_string(),
                    score: entry.relevance_score.unwrap_or(entry.confidence),
                    reason,
                });
            }
            for result in &code_scored {
                // Distinguish BM25 (tantivy score > 1.0) vs symbol/ripgrep (1.0) vs structural (0.9)
                let (retriever, reason) = if result.score > 2.0 {
                    ("tantivy", "bm25")
                } else if (result.score - 0.9).abs() < 0.01 {
                    ("ast-grep", "symbol match")
                } else {
                    ("tantivy", "symbol match")
                };
                context_pack.provenance.push(coderun_core::ipc::ContextProvenance {
                    path: result.path.clone(),
                    source: "code".to_string(),
                    retriever: retriever.to_string(),
                    score: result.score,
                    reason: reason.to_string(),
                });
            }
            // Keep deduplicated behavior: if dedup emptied context, provenance reflects original scoring but content empty; for honest determinism, provenance always matches scored retrieval
        }

        // Step 5: Select model via Model Router (heuristic, no LLM call)
        let routing_decision = self.select_model(
            &request.message,
            &code_context,
            &knowledge_context,
            &skills_context,
            total_tokens,
        );

        // Step 6: Emit ContextBuilt event (async-only, never blocks hot path)
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

    /// Deduplicate content against session fingerprint (spec §3 deduplication + PRINCIPLES.md:10)
    fn dedup_content(&self, session_id: &str, content: &str) -> String {
        if content.is_empty() || session_id.is_empty() {
            return content.to_string();
        }
        let hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(content.as_bytes());
            format!("{:x}", hasher.finalize())
        };
        if let Ok(mut fps) = self.session_fingerprints.lock() {
            let entry = fps.entry(session_id.to_string()).or_default();
            if entry.contains(&hash) {
                debug!(session_id = %session_id, hash = %hash, "Dedup: skipping duplicate content block");
                return String::new();
            }
            entry.insert(hash);
        }
        content.to_string()
    }

    /// Repository state (git HEAD) for deterministic ContextPack (TASK-008)
    fn repository_state(&self) -> String {
        // Try env override first (for tests)
        if let Ok(v) = std::env::var("CODERUN_REPO_STATE") { return v; }
        // Try git rev-parse HEAD in repo_path (best-effort, fail-open to empty)
        let repo_path = self.repo_intel.lock().ok().map(|ri| ri.repo_path().to_path_buf()).unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")));
        if let Ok(out) = std::process::Command::new("git").arg("-C").arg(&repo_path).arg("rev-parse").arg("HEAD").output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if s.len() >= 7 { return s; }
            }
        }
        // Fallback: hash of repo_path for determinism
        {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(repo_path.to_string_lossy().as_bytes());
            format!("{:x}", h.finalize())[..12].to_string()
        }
    }

    /// Search code via Repository Intelligence (scored variant for provenance TASK-007)
    fn search_code_scored(
        &self,
        query: &str,
        context_hints: &Option<ContextHints>,
    ) -> Result<(String, Vec<coderun_core::SearchResult>), String> {
        let repo_intel = self.repo_intel.lock().map_err(|e| format!("Lock error: {}", e))?;
        let mut results = Vec::new();
        let mut scored = Vec::new();
        // scored search — try tantivy BM25 first for real scores, fallback to ripgrep
        let search_results = repo_intel.search_fulltext(query, None, 10).or_else(|_| repo_intel.search_text(query, None, 10)).unwrap_or(coderun_core::SearchResults { results: vec![], total_count: 0 });
        for result in &search_results.results {
            scored.push(result.clone());
            if let Ok(content) = repo_intel.get_file_content(
                &result.path,
                Some((result.line.saturating_sub(5), result.line + 10)),
            ) {
                results.push(format!("// {}:{}\n{}", result.path, result.line, content));
            }
        }
        // Also search for files mentioned in context hints (score 1.0, symbol match)
        if let Some(hints) = context_hints {
            if let Some(files) = &hints.files_mentioned {
                for file in files {
                    if let Ok(content) = repo_intel.get_file_content(file, Some((1, self.config.max_lines_per_file))) {
                        results.push(format!("// {}\n{}", file, content));
                        scored.push(coderun_core::SearchResult { path: file.clone(), line: 1, content: file.clone(), score: 1.0 });
                    }
                }
            }
        }
        Ok((results.join("\n\n"), scored))
    }

    /// Search code via Repository Intelligence (legacy wrapper)
    fn search_code(
        &self,
        query: &str,
        context_hints: &Option<ContextHints>,
    ) -> Result<String, String> {
        Ok(self.search_code_scored(query, context_hints)?.0)
    }

    /// Retrieve knowledge via Knowledge Hub (scored variant)
    fn retrieve_knowledge_scored(&self, query: &str) -> Result<(String, Vec<coderun_core::KnowledgeEntry>), String> {
        let knowledge_hub = self.knowledge_hub.lock().map_err(|e| format!("Lock error: {}", e))?;
        let entries = knowledge_hub.retrieve_knowledge(query, None, 10)?;
        let formatted: Vec<String> = entries.iter().map(|e| format!("// [{}] {}: {}", e.category, e.key, e.value)).collect();
        Ok((formatted.join("\n"), entries))
    }

    /// Retrieve knowledge via Knowledge Hub (legacy)
    fn retrieve_knowledge(&self, query: &str) -> Result<String, String> {
        Ok(self.retrieve_knowledge_scored(query)?.0)
    }

    /// Match skills via Knowledge Hub (scored variant — tag overlap)
    fn match_skills_scored(&self, query: &str) -> Result<(String, Vec<(String, f64, f64)>), String> {
        let knowledge_hub = self.knowledge_hub.lock().map_err(|e| format!("Lock error: {}", e))?;
        let matches = knowledge_hub.match_skills(query, 5);
        let formatted: Vec<String> = matches.iter().map(|m| {
            format!("# {}\n{}\n\nExamples:\n{}\n\nConstraints:\n{}", m.skill_name, m.instructions, m.examples.join("\n"), m.constraints.join("\n"))
        }).collect();
        // Capture (skill_name, score, specificity proxy = score)
        let scored = matches.into_iter().map(|m| {
            let specificity = m.match_score;
            (m.skill_name, m.match_score, specificity)
        }).collect();
        Ok((formatted.join("\n\n---\n\n"), scored))
    }

    /// Match skills via Knowledge Hub (legacy)
    fn match_skills(&self, query: &str) -> Result<String, String> {
        Ok(self.match_skills_scored(query)?.0)
    }

    /// Assemble the context pack with cache-aware ordering + frozen-prefix + reversible compression
    /// Order: skills (most stable) → docs → code (least stable) per PRINCIPLES.md:42-54
    /// Frozen-prefix boundary: byte-identical skills block is cache-stable; only content after boundary changes.
    fn assemble_context_pack(
        &self,
        skills_context: &str,
        knowledge_context: &str,
        code_context: &str,
        token_budget: &mut usize,
        token_usage_by_source: &mut HashMap<String, usize>,
    ) -> (ContextPack, usize) {
        let mut total_tokens = 0;

        // Section 1: behavioral_skills (20% budget) — most cache-stable
        let skills_budget = (*token_budget as f64 * 0.20) as usize;
        let skills_tokens = count_tokens(skills_context);
        let (mut skills_content, skills_used) = if skills_tokens <= skills_budget {
            (skills_context.to_string(), skills_tokens)
        } else {
            truncate_to_tokens(skills_context, skills_budget)
        };
        // Frozen-prefix boundary marker — only content after this line changes between calls
        const FROZEN_BOUNDARY: &str = "\n# --- FROZEN PREFIX END (cache-stable above, variable below) ---\n";
        if !skills_content.is_empty() && !skills_content.contains("FROZEN PREFIX END") {
            skills_content.push_str(FROZEN_BOUNDARY);
        }
        token_usage_by_source.insert("behavioral_skills".to_string(), skills_used);
        total_tokens += skills_used;

        // Section 2: docs_context (15% budget)
        let docs_budget = (*token_budget as f64 * 0.15) as usize;
        let docs_tokens = count_tokens(knowledge_context);
        let (docs_content, docs_used) = if docs_tokens <= docs_budget {
            (knowledge_context.to_string(), docs_tokens)
        } else {
            truncate_to_tokens(knowledge_context, docs_budget)
        };
        token_usage_by_source.insert("docs_context".to_string(), docs_used);
        total_tokens += docs_used;

        // Section 3: code_context (55% budget)
        let code_budget = (*token_budget as f64 * 0.55) as usize;
        let code_tokens = count_tokens(code_context);
        let (code_content, code_used) = if code_tokens <= code_budget {
            (code_context.to_string(), code_tokens)
        } else {
            truncate_to_tokens(code_context, code_budget)
        };
        token_usage_by_source.insert("code_context".to_string(), code_used);
        total_tokens += code_used;

        // Remaining budget for metadata
        let remaining = token_budget.saturating_sub(total_tokens);
        token_usage_by_source.insert("metadata".to_string(), remaining);

        let context_pack = ContextPack {
            behavioral_skills: skills_content,
            docs_context: docs_content,
            code_context: code_content,
            token_usage: TokenUsage {
                total_tokens,
                budget_remaining: remaining,
                by_source: token_usage_by_source.clone(),
            },
            provenance: vec![],
            metadata: coderun_core::ipc::ContextMetadata {
                task_hash: String::new(),
                correlation_id: String::new(),
                cache_order: self.config.cache_order.clone(),
                repository_state: String::new(),
            },
            repository_state: String::new(),
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

    /// Serialize context pack to YAML (fixed order: skills → docs → code for cache stability)
    pub fn to_yaml(pack: &ContextPack) -> Result<String, String> {
        serde_yaml::to_string(pack).map_err(|e| format!("Failed to serialize to YAML: {}", e))
    }

    /// Retrieve original full content saved by reversible compression (spec §2)
    pub fn get_original(hash: &str) -> Result<String, String> {
        let path = reversible_cache_path(hash);
        std::fs::read_to_string(&path).map_err(|e| format!("Original not found for {hash}: {e}"))
    }
}

impl coderun_core::IContextBuilder for ContextEngine {
    fn build_context(
        &self,
        task: &TaskRequest,
    ) -> std::result::Result<(ContextPack, RoutingDecision), coderun_core::CoderunError> {
        ContextEngine::build_context(self, task)
            .map_err(|e| coderun_core::CoderunError::ContextBuildFailed(e))
    }

    fn to_yaml(pack: &ContextPack) -> std::result::Result<String, coderun_core::CoderunError>
    where
        Self: Sized,
    {
        ContextEngine::to_yaml(pack)
            .map_err(|e| coderun_core::CoderunError::Serialization(e))
    }
}

// ── Helpers for reversible compression ───────────────────────────────────

fn reversible_cache_dir() -> std::path::PathBuf {
    if let Some(home) = dirs_home() {
        home.join(".coderun").join("cache").join("originals")
    } else {
        std::path::PathBuf::from(".coderun/cache/originals")
    }
}

fn reversible_cache_path(hash: &str) -> std::path::PathBuf {
    reversible_cache_dir().join(format!("{}.txt", hash))
}

fn dirs_home() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    { std::env::var("USERPROFILE").ok().map(std::path::PathBuf::from) }
    #[cfg(not(target_os = "windows"))]
    { std::env::var("HOME").ok().map(std::path::PathBuf::from) }
}

// ── Token Counting (tiktoken-rs, never via model API) ────────────────────

/// Count tokens locally with tiktoken-rs `cl100k_base` (spec §3, §4)
/// Fallback to char/4 heuristic only if tokenizer fails — logs WARN.
pub fn count_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    match tiktoken_rs::cl100k_base() {
        Ok(bpe) => bpe.encode_ordinary(text).len(),
        Err(e) => {
            warn!(error = %e, "tiktoken load failed, falling back to heuristic");
            estimate_tokens_heuristic(text)
        }
    }
}

fn estimate_tokens_heuristic(text: &str) -> usize {
    let char_count = text.len();
    let word_count = text.split_whitespace().count();
    let by_chars = char_count / 4;
    let by_words = (word_count as f64 * 1.3) as usize;
    by_chars.max(by_words)
}

/// Legacy alias — keep for external callers that matched heuristic name
pub fn estimate_tokens(text: &str) -> usize {
    count_tokens(text)
}

/// Truncate text to fit within a token budget, reversible by default.
/// Saves full content to `~/.coderun/cache/originals/{hash}.txt` and appends pointer.
fn truncate_to_tokens(text: &str, budget: usize) -> (String, usize) {
    let tokens = count_tokens(text);
    if tokens <= budget {
        return (text.to_string(), tokens);
    }
    // Reversible: save original
    let hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        format!("{:x}", hasher.finalize())[..16].to_string()
    };
    let cache_path = reversible_cache_path(&hash);
    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&cache_path, text);

    // Truncate at line boundaries by token count
    let mut result = String::new();
    let mut current_tokens = 0;
    for line in text.lines() {
        let line_tokens = count_tokens(line);
        // +1 for newline
        if current_tokens + line_tokens + 1 > budget.saturating_sub(5) {
            result.push_str(&format!(
                "... [truncated — full at {} | retrieve via ContextEngine::get_original(\"{}\")]\n",
                cache_path.display(),
                hash
            ));
            current_tokens += 5;
            break;
        }
        result.push_str(line);
        result.push('\n');
        current_tokens += line_tokens + 1;
    }
    if result.is_empty() {
        result = format!(
            "... [truncated — full at {} | hash {}]\n",
            cache_path.display(),
            hash
        );
        current_tokens = 5;
    }
    (result, current_tokens)
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        assert!(estimate_tokens("hello world") > 0);
        assert!(estimate_tokens("") == 0);
        // tiktoken packs repeated "a" efficiently, so threshold lower than old heuristic
        assert!(estimate_tokens(&"a".repeat(100)) > 5);
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
            provenance: vec![],
            metadata: coderun_core::ipc::ContextMetadata::default(),
            repository_state: String::new(),
        };

        let yaml = ContextEngine::to_yaml(&pack).unwrap();
        assert!(yaml.contains("behavioral_skills"));
        assert!(yaml.contains("docs_context"));
        assert!(yaml.contains("code_context"));
    }

    #[test]
    fn test_count_tokens_tiktoken_vs_heuristic() {
        // tiktoken count should be non-zero and within 5x heuristic for English
        let text = "hello world, this is a test of token counting";
        let t = count_tokens(text);
        let h = estimate_tokens_heuristic(text);
        assert!(t > 0);
        assert!(t <= h * 5);
        assert!(count_tokens("") == 0);
    }

    #[test]
    fn test_frozen_prefix_boundary() {
        // Directly test assemble_context_pack boundary
        use coderun_events::EventBus;
        use coderun_knowledge::KnowledgeHub;
        use coderun_knowledge::KnowledgeConfig;
        use coderun_repo_intel::RepositoryIntelligence;
        use coderun_storage::Database;
        use std::path::PathBuf;

        let db = Database::open(&PathBuf::from(":memory:")).unwrap();
        let event_bus = EventBus::new();
        let repo_intel = RepositoryIntelligence::new(PathBuf::from("."), Database::open(&PathBuf::from(":memory:")).unwrap(), event_bus.clone());
        let kh = KnowledgeHub::new(db, event_bus.clone(), KnowledgeConfig::default());
        let engine = ContextEngine::new(repo_intel, kh, event_bus, ContextConfig::default());
        let mut budget = 12000;
        let mut usage = HashMap::new();
        let (pack, _) = engine.assemble_context_pack("skill content line", "doc line", "code line", &mut budget, &mut usage);
        assert!(pack.behavioral_skills.contains("FROZEN PREFIX END"));
        assert_eq!(engine.config.cache_order[0], "behavioral_skills");
    }

    #[test]
    fn test_dedup_skips_duplicate() {
        use coderun_events::EventBus;
        use coderun_knowledge::{KnowledgeConfig, KnowledgeHub};
        use coderun_repo_intel::RepositoryIntelligence;
        use coderun_storage::Database;
        use std::path::PathBuf;

        let db = Database::open(&PathBuf::from(":memory:")).unwrap();
        let event_bus = EventBus::new();
        let repo_intel = RepositoryIntelligence::new(PathBuf::from("."), Database::open(&PathBuf::from(":memory:")).unwrap(), event_bus.clone());
        let kh = KnowledgeHub::new(db, event_bus.clone(), KnowledgeConfig::default());
        let engine = ContextEngine::new(repo_intel, kh, event_bus, ContextConfig::default());
        let a = engine.dedup_content("sess1", "hello world");
        let b = engine.dedup_content("sess1", "hello world");
        let c = engine.dedup_content("sess2", "hello world");
        assert_eq!(a, "hello world");
        assert_eq!(b, ""); // deduped
        assert_eq!(c, "hello world"); // different session
    }

    #[test]
    fn test_reversible_truncation_pointer() {
        let long = (0..500).map(|i| format!("line {} with some content to exceed budget and trigger truncation", i)).collect::<Vec<_>>().join("\n");
        let (truncated, tokens) = truncate_to_tokens(&long, 20);
        assert!(truncated.contains("truncated"));
        assert!(truncated.contains("retrieve via ContextEngine::get_original"));
        assert!(tokens <= 30);
        // Verify file was written and can be retrieved
        let hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(long.as_bytes());
            format!("{:x}", hasher.finalize())[..16].to_string()
        };
        let original = ContextEngine::get_original(&hash).unwrap();
        assert_eq!(original, long);
    }

    #[test]
    fn test_build_context_deterministic() {
        use coderun_core::TaskRequest;
        use coderun_events::EventBus;
        use coderun_knowledge::{KnowledgeConfig, KnowledgeHub};
        use coderun_repo_intel::RepositoryIntelligence;
        use coderun_storage::Database;
        use std::path::PathBuf;

        // Deterministic: same repo+task+config → same pack content even with different session_id, not deduped
        std::env::set_var("CODERUN_REPO_STATE", "deterministic-test-head-abc123");
        let dir = std::env::temp_dir().join(format!("coderun_det_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.rs"), "fn hello() { println!(\"hi\"); }").unwrap();
        let db_path = dir.join("det.db");
        let db = Database::open(&db_path).unwrap();
        let event_bus = EventBus::new();
        // Index repo so code retrieval has something
        let mut ri = RepositoryIntelligence::new(dir.clone(), Database::open(&db_path).unwrap(), event_bus.clone());
        let _ = ri.index_repository();
        let kh = KnowledgeHub::new(db, event_bus.clone(), KnowledgeConfig::default());
        let engine = ContextEngine::new(ri, kh, event_bus, ContextConfig::default());
        let task1 = TaskRequest { message: "fix hello function".to_string(), session_id: "sessA".to_string(), context_hints: None };
        let task2 = TaskRequest { message: "fix hello function".to_string(), session_id: "sessB".to_string(), context_hints: None };
        let (pack1, routing1) = engine.build_context(&task1).unwrap();
        let (pack2, routing2) = engine.build_context(&task2).unwrap();
        // Deterministic: same repo state hash, same task hash, same content (correlation_id intentionally differs, not compared)
        assert_eq!(pack1.repository_state, pack2.repository_state, "repo_state must be deterministic");
        assert_eq!(pack1.metadata.repository_state, pack2.metadata.repository_state);
        assert_eq!(pack1.metadata.task_hash, pack2.metadata.task_hash, "task_hash deterministic");
        assert_eq!(pack1.metadata.cache_order, pack2.metadata.cache_order);
        // Code/docs/skills content should be equal (different session => not deduped)
        assert_eq!(pack1.code_context, pack2.code_context);
        assert_eq!(pack1.docs_context, pack2.docs_context);
        assert_eq!(pack1.behavioral_skills, pack2.behavioral_skills);
        assert_eq!(pack1.token_usage.total_tokens, pack2.token_usage.total_tokens);
        assert_eq!(routing1.tier, routing2.tier);
        // Same session should dedup second call (different from first, empty on second)
        let task3 = TaskRequest { message: "fix hello function".to_string(), session_id: "sessA".to_string(), context_hints: None };
        let (pack3, _) = engine.build_context(&task3).unwrap();
        // dedup_content may empty repeated content for same session; pack3 may have empty sections but task_hash still same
        assert_eq!(pack3.metadata.task_hash, pack1.metadata.task_hash);
        std::env::remove_var("CODERUN_REPO_STATE");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
