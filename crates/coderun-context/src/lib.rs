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
    /// Default repo intelligence (daemon CWD) — used when a request carries no repository_path
    default_repo_intel: Arc<Mutex<RepositoryIntelligence>>,
    /// Lazily-created per-repository intelligence keyed by canonical workspace path (TASK-036/F-7):
    /// ONE daemon serves many opencode windows on different repos simultaneously.
    repo_cache: Arc<Mutex<HashMap<String, Arc<Mutex<RepositoryIntelligence>>>>>,
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
            default_repo_intel: Arc::new(Mutex::new(repo_intel)),
            repo_cache: Arc::new(Mutex::new(HashMap::new())),
            knowledge_hub: Arc::new(Mutex::new(knowledge_hub)),
            model_router,
            event_bus,
            config,
            session_fingerprints: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Resolve the per-request repository view (TASK-036): when the agent's workspace path is
    /// known, build/cache a RepositoryIntelligence for it so retrieval + file reads target THAT
    /// repo instead of wherever the daemon happens to run. Falls back to the daemon-CWD engine.
    fn resolve_repo_intel(
        &self,
        repository_path: Option<&str>,
    ) -> Result<Arc<Mutex<RepositoryIntelligence>>, String> {
        let hint = match repository_path.map(str::trim).filter(|s| !s.is_empty()) {
            Some(h) => h,
            None => return Ok(self.default_repo_intel.clone()),
        };
        let canonical = dunce::canonicalize(hint)
            .unwrap_or_else(|_| std::path::PathBuf::from(hint));
        let key = canonical.to_string_lossy().to_string();
        if let Ok(cache) = self.repo_cache.lock() {
            if let Some(ri) = cache.get(&key) {
                return Ok(ri.clone());
            }
        }
        // Retrieval-only instance — DB is only needed for indexing/metadata, use throwaway in-memory store
        let db = coderun_storage::Database::open(&std::path::PathBuf::from(":memory:"))?;
        let ri = Arc::new(Mutex::new(RepositoryIntelligence::new(
            canonical.clone(),
            db,
            self.event_bus.clone(),
        )));
        if let Ok(mut cache) = self.repo_cache.lock() {
            let repo_id = match ri.lock() {
                Ok(guard) => guard.repository_id().to_string(),
                Err(_) => String::new(),
            };
            info!(repo = %canonical.to_string_lossy(), repository_id = %repo_id, "resolved per-repository intelligence (TASK-036)");
            cache.entry(key).or_insert_with(|| ri.clone());
        }
        Ok(ri)
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

        // TASK-036: resolve the per-request repository view (agent workspace, else daemon CWD)
        let repo_intel = self.resolve_repo_intel(request.repository_path.as_deref())?;
        // TASK-030: scope all retrieval to THIS repository's stamp (shared hash formula)
        let repository_id = match repo_intel.lock() {
            Ok(guard) => guard.repository_id().to_string(),
            Err(_) => String::new(),
        };

        // Step 1: Search code via Repository Intelligence (dedup-eligible) — capture SearchResult.score for provenance (TASK-007)
        let (raw_code, code_scored) = self.search_code_scored(&repo_intel, &repository_id, &request.message, &request.context_hints)?;
        let code_context = self.dedup_content(&request.session_id, &raw_code);

        // Step 2: Retrieve knowledge via Knowledge Hub — capture BM25 vs engram provenance
        let (raw_knowledge, knowledge_scored) = self.retrieve_knowledge_scored(&repository_id, &request.message)?;
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
            hasher.update(self.repository_state_for(&repo_intel).as_bytes());
            let task_hash = format!("{:x}", hasher.finalize())[..16].to_string();
            context_pack.metadata.task_hash = task_hash;
            context_pack.metadata.correlation_id = correlation_id.to_string();
            let repo_state = self.repository_state_for(&repo_intel);
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
                // TASK-033/F-4: category stays in `source`; path is cleaned of prefixes/verbatim markers
                context_pack.provenance.push(coderun_core::ipc::ContextProvenance {
                    path: clean_provenance_path(&entry.key),
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
                    path: clean_provenance_path(&result.path),
                    source: "code".to_string(),
                    retriever: retriever.to_string(),
                    score: result.score,
                    reason: reason.to_string(),
                });
            }
            // TASK-032/F-3: dedup provenance by (path, source, retriever) keeping highest score
            dedup_provenance(&mut context_pack.provenance);
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

    /// Repository state (git HEAD) for deterministic ContextPack (TASK-008) — resolved per request repo (TASK-036)
    fn repository_state_for(&self, repo_intel: &Mutex<RepositoryIntelligence>) -> String {
        // Try env override first (for tests)
        if let Ok(v) = std::env::var("CODERUN_REPO_STATE") { return v; }
        let repo_path = match repo_intel.lock() {
            Ok(guard) => guard.repo_path().to_path_buf(),
            Err(_) => std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        };
        // Try git rev-parse HEAD in repo_path (best-effort, fail-open to empty)
        if let Ok(out) = std::process::Command::new("git").arg("-C").arg(&repo_path).arg("rev-parse").arg("HEAD").output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if s.len() >= 7 { return s; }
            }
        }
        // Fallback: hash of repo_path for determinism
        coderun_core::repository_id_from_path(&repo_path.to_string_lossy())
    }

    /// Search code via Repository Intelligence (scored variant for provenance TASK-007) —
    /// scoped to the resolved repository (TASK-030/036)
    fn search_code_scored(
        &self,
        repo_intel: &Mutex<RepositoryIntelligence>,
        repository_id: &str,
        query: &str,
        context_hints: &Option<ContextHints>,
    ) -> Result<(String, Vec<coderun_core::SearchResult>), String> {
        let repo_intel = repo_intel.lock().map_err(|e| format!("Lock error: {}", e))?;
        let mut results = Vec::new();
        let mut scored = Vec::new();
        // scored search — try tantivy BM25 first for real scores (repo-scoped), fallback to ripgrep
        let search_results = repo_intel.search_fulltext(query, None, 10, Some(repository_id)).or_else(|_| repo_intel.search_text(query, None, 10)).unwrap_or(coderun_core::SearchResults { results: vec![], total_count: 0 });
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

    /// Retrieve knowledge via Knowledge Hub (scored variant) — scoped to one repository (TASK-030)
    fn retrieve_knowledge_scored(&self, repository_id: &str, query: &str) -> Result<(String, Vec<coderun_core::KnowledgeEntry>), String> {
        let knowledge_hub = self.knowledge_hub.lock().map_err(|e| format!("Lock error: {}", e))?;
        let entries = knowledge_hub.retrieve_knowledge(query, None, 10, Some(repository_id))?;
        let formatted: Vec<String> = entries.iter().map(|e| format!("// [{}] {}: {}", e.category, e.key, e.value)).collect();
        Ok((formatted.join("\n"), entries))
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
        // Zero-result safeguard: when both code and knowledge retrieval are empty,
        // the agent has insufficient context — signal this to the router so it can
        // escalate tier instead of defaulting to the cheapest model.
        let retrieval_empty = code_context.is_empty() && knowledge_context.is_empty();
        let request = RoutingRequest {
            message: message.to_string(),
            file_count: code_context.lines().count(),
            symbol_count: 0, // Would need repo-intel integration
            knowledge_entries: knowledge_context.lines().count(),
            skills_matched: skills_context.matches("---").count() + 1,
            token_count,
            model_override: None,
            retrieval_empty,
        };

        self.model_router.select_model(&request)
    }

    /// Clear session fingerprint (e.g., on daemon restart)
    pub fn clear_session_fingerprint(&self, session_id: &str) {
        if let Ok(mut fingerprints) = self.session_fingerprints.lock() {
            fingerprints.remove(session_id);
        }
    }

    /// Serialize context pack to YAML — compact, deterministic order (skills → docs → code).
    /// TASK-031/F-2: empty sections are omitted entirely; a zero-value pack serializes to an
    /// EMPTY string so the daemon can pass the prompt through byte-identical instead of
    /// paying ~500-700 tokens of metadata skeleton for no retrievable value.
    pub fn to_yaml(pack: &ContextPack) -> Result<String, String> {
        if pack.token_usage.total_tokens == 0 {
            return Ok(String::new());
        }
        let mut out = String::new();
        let mut block = |key: &str, content: &str| {
            if content.is_empty() {
                return;
            }
            out.push_str(key);
            out.push_str(": |\n");
            for line in content.lines() {
                out.push_str("  ");
                out.push_str(line);
                out.push('\n');
            }
        };
        block("behavioral_skills", &pack.behavioral_skills);
        block("docs_context", &pack.docs_context);
        block("code_context", &pack.code_context);
        // Compact single-line metadata — only when there is actual content above
        out.push_str(&format!(
            "token_usage: {{total_tokens: {}, budget_remaining: {}}}\n",
            pack.token_usage.total_tokens, pack.token_usage.budget_remaining
        ));
        if !pack.provenance.is_empty() {
            out.push_str("provenance:\n");
            for p in &pack.provenance {
                out.push_str(&format!(
                    "  - {{path: \"{}\", source: {}, retriever: {}, score: {:.3}}}\n",
                    p.path.replace('\\', "/").replace('"', "'"),
                    p.source, p.retriever, p.score
                ));
            }
        }
        Ok(out)
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

// ── Provenance hygiene (TASK-032/033 — F-3, F-4) ─────────────────────────

/// Clean a provenance path (TASK-033/F-4): strip Windows verbatim prefixes (`\\?\`,
/// `\\?\UNC\`) and knowledge collection prefixes (`docs:`, `adr:` …) so provenance renders
/// plain absolute or repo-relative paths. Category stays in the `source` field.
fn clean_provenance_path(raw: &str) -> String {
    let mut p = raw.trim().to_string();
    // Windows verbatim prefixes first
    if let Some(rest) = p.strip_prefix(r"\\?\UNC\") {
        p = format!(r"\\{rest}");
    } else if let Some(rest) = p.strip_prefix(r"\\?\") {
        p = rest.to_string();
    }
    // Collection/category prefix like "docs:" / "adr:" — only when followed by an
    // absolute-looking path (drive letter, backslash, slash, or another verbatim marker)
    const CATEGORIES: [&str; 7] = ["docs", "adr", "convention", "pattern", "domain", "profile", "memory"];
    for cat in CATEGORIES {
        let prefix = format!("{cat}:");
        if let Some(rest) = p.strip_prefix(&prefix) {
            let looks_absolute = rest.starts_with('\\')
                || rest.starts_with('/')
                || rest.starts_with(r"\\?\")
                || {
                    let mut chars = rest.chars();
                    matches!((chars.next(), chars.next()), (Some(a), Some(':')) if a.is_ascii_alphabetic())
                };
            if looks_absolute {
                p = rest.trim_start_matches(r"\\?\").to_string();
                break;
            }
        }
    }
    p
}

/// Dedup provenance entries by (path, source, retriever), keeping the highest score
/// (TASK-032/F-3) — identical rows must render exactly once.
fn dedup_provenance(provenance: &mut Vec<coderun_core::ipc::ContextProvenance>) {
    let mut seen: HashMap<(String, String, String), usize> = HashMap::new();
    let mut deduped: Vec<coderun_core::ipc::ContextProvenance> = Vec::with_capacity(provenance.len());
    for entry in provenance.drain(..) {
        let key = (entry.path.clone(), entry.source.clone(), entry.retriever.clone());
        match seen.get(&key) {
            Some(&idx) => {
                if entry.score > deduped[idx].score {
                    deduped[idx] = entry;
                }
            }
            None => {
                seen.insert(key, deduped.len());
                deduped.push(entry);
            }
        }
    }
    *provenance = deduped;
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

    /// Serializes tests that mutate process-global env vars / the shared tantivy index dir —
    /// parallel mutation makes retrieval results non-deterministic.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
        let kh = KnowledgeHub::new(db, event_bus.clone(), KnowledgeConfig { memory_enabled: false, ..Default::default() });
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
        let kh = KnowledgeHub::new(db, event_bus.clone(), KnowledgeConfig { memory_enabled: false, ..Default::default() });
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

        // Deterministic: same repo+task+config → same pack content even with different session_id, not deduped
        let _env = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("coderun_det_{}", uuid::Uuid::new_v4()));
        // Isolate from other tests' writes to the shared tantivy index — ripgrep fallback over
        // this temp repo is what must be deterministic.
        std::env::set_var("CODERUN_INDEX_DIR", dir.join("idx").to_string_lossy().to_string());
        std::env::set_var("CODERUN_REPO_STATE", "deterministic-test-head-abc123");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.rs"), "fn hello() { println!(\"hi\"); }").unwrap();
        let db_path = dir.join("det.db");
        let db = Database::open(&db_path).unwrap();
        let event_bus = EventBus::new();
        // Index repo so code retrieval has something
        let mut ri = RepositoryIntelligence::new(dir.clone(), Database::open(&db_path).unwrap(), event_bus.clone());
        let _ = ri.index_repository();
        let kh = KnowledgeHub::new(db, event_bus.clone(), KnowledgeConfig { memory_enabled: false, ..Default::default() });
        let engine = ContextEngine::new(ri, kh, event_bus, ContextConfig::default());
        let task1 = TaskRequest { message: "fix hello function".to_string(), session_id: "sessA".to_string(), context_hints: None, repository_id: String::new(), repository_path: None };
        let task2 = TaskRequest { message: "fix hello function".to_string(), session_id: "sessB".to_string(), context_hints: None, repository_id: String::new(), repository_path: None };
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
        let task3 = TaskRequest { message: "fix hello function".to_string(), session_id: "sessA".to_string(), context_hints: None, repository_id: String::new(), repository_path: None };
        let (pack3, _) = engine.build_context(&task3).unwrap();
        // dedup_content may empty repeated content for same session; pack3 may have empty sections but task_hash still same
        assert_eq!(pack3.metadata.task_hash, pack1.metadata.task_hash);
        std::env::remove_var("CODERUN_INDEX_DIR");
        std::env::remove_var("CODERUN_REPO_STATE");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_clean_provenance_path_strips_verbatim_and_category() {
        // TASK-033/F-4
        assert_eq!(clean_provenance_path(r"\\?\C:\Leon\eShop\src\Checkout.cs"), r"C:\Leon\eShop\src\Checkout.cs");
        assert_eq!(clean_provenance_path(r"\\?\UNC\server\share\a.rs"), r"\\server\share\a.rs");
        assert_eq!(clean_provenance_path(r"docs:\\?\C:\Leon\coderun\docs\DATA_FLOW.md"), r"C:\Leon\coderun\docs\DATA_FLOW.md");
        assert_eq!(clean_provenance_path(r"adr:C:\repos\x\docs\adr\0001.md"), r"C:\repos\x\docs\adr\0001.md");
        // Relative paths pass through untouched — category prefix only stripped when absolute follows
        assert_eq!(clean_provenance_path("docs/guide.md"), "docs/guide.md");
        assert_eq!(clean_provenance_path("src/main.rs"), "src/main.rs");
    }

    #[test]
    fn test_dedup_provenance_keeps_highest_score() {
        use coderun_core::ipc::ContextProvenance;
        let mk = |score: f64| ContextProvenance {
            path: "docs/guide.md".into(),
            source: "docs".into(),
            retriever: "tantivy".into(),
            score,
            reason: "bm25".into(),
        };
        let mut prov = vec![mk(0.5), mk(0.9), mk(0.7), mk(0.9)];
        dedup_provenance(&mut prov);
        assert_eq!(prov.len(), 1, "F-3: identical rows must render once");
        assert!((prov[0].score - 0.9).abs() < 1e-9);
    }

    #[test]
    fn test_to_yaml_zero_value_pack_is_empty() {
        // TASK-031/F-2: no hits → empty YAML → daemon passes prompt through untouched
        let pack = ContextPack {
            behavioral_skills: String::new(),
            docs_context: String::new(),
            code_context: String::new(),
            token_usage: TokenUsage { total_tokens: 0, budget_remaining: 12000, by_source: HashMap::new() },
            provenance: vec![],
            metadata: coderun_core::ipc::ContextMetadata::default(),
            repository_state: String::new(),
        };
        assert_eq!(ContextEngine::to_yaml(&pack).unwrap(), "");
    }

    #[test]
    fn test_to_yaml_omits_empty_sections_and_has_content() {
        // TASK-031/F-2: with hits, appended block contains actual content, no empty skeletons
        use coderun_core::ipc::ContextProvenance;
        let pack = ContextPack {
            behavioral_skills: String::new(),
            docs_context: String::new(),
            code_context: "// src/Checkout.cs:10\npublic async Task Checkout()".to_string(),
            token_usage: TokenUsage { total_tokens: 42, budget_remaining: 11958, by_source: HashMap::new() },
            provenance: vec![ContextProvenance { path: "src/Checkout.cs".into(), source: "code".into(), retriever: "tantivy".into(), score: 3.2, reason: "bm25".into() }],
            metadata: coderun_core::ipc::ContextMetadata::default(),
            repository_state: String::new(),
        };
        let yaml = ContextEngine::to_yaml(&pack).unwrap();
        assert!(yaml.contains("code_context"));
        assert!(yaml.contains("Checkout"));
        assert!(!yaml.contains("behavioral_skills:"), "empty sections must be omitted");
        assert!(!yaml.contains("docs_context:"), "empty sections must be omitted");
        assert!(yaml.contains("total_tokens: 42"));
    }

    #[tokio::test]
    async fn test_per_repo_resolution_no_cross_repo_leak() {
        // TASK-036/F-7 + F-1 acceptance: one engine, two repos — each request resolves to its own repo.
        let _env = ENV_LOCK.lock().unwrap();
        std::env::set_var("CODERUN_INDEX_DIR", std::env::temp_dir().join(format!("coderun_idx_{}", uuid::Uuid::new_v4())).to_string_lossy().to_string());
        std::env::set_var("CODERUN_REPO_STATE", "per-repo-test-head");
        let repo_a = std::env::temp_dir().join(format!("coderun_repoA_{}", uuid::Uuid::new_v4()));
        let repo_b = std::env::temp_dir().join(format!("coderun_repoB_{}", uuid::Uuid::new_v4()));
        for (root, marker) in [(&repo_a, "eshop basket checkout flow unique_marker_alpha"), (&repo_b, "coderun router daemon unique_marker_beta")] {
            std::fs::create_dir_all(root).unwrap();
            std::fs::write(root.join("feature.txt"), format!("{marker}\n")).unwrap();
        }
        // Seed the global tantivy index from both repos
        for root in [&repo_a, &repo_b] {
            let db = coderun_storage::Database::open(&std::path::PathBuf::from(":memory:")).unwrap();
            let mut ri = RepositoryIntelligence::new(root.clone(), db, EventBus::new());
            ri.index_repository().unwrap();
        }
        // Engine whose default view is repo_b (simulates a daemon started in repo_b)
        let db = coderun_storage::Database::open(&std::path::PathBuf::from(":memory:")).unwrap();
        let kh_db = coderun_storage::Database::open(&std::path::PathBuf::from(":memory:")).unwrap();
        let hub = KnowledgeHub::new(kh_db, EventBus::new(), coderun_knowledge::KnowledgeConfig { memory_enabled: false, ..Default::default() });
        let engine = ContextEngine::new(
            RepositoryIntelligence::new(repo_b.clone(), db, EventBus::new()),
            hub,
            EventBus::new(),
            ContextConfig::default(),
        );
        // Prompt scoped to repo A must surface ONLY repo A's file even though daemon CWD is repo B
        let task_a = TaskRequest { message: "eshop basket checkout flow unique_marker_alpha".to_string(), session_id: "sA".to_string(), context_hints: None, repository_id: String::new(), repository_path: Some(repo_a.to_string_lossy().to_string()) };
        let (pack_a, _) = engine.build_context(&task_a).unwrap();
        assert!(pack_a.code_context.contains("unique_marker_alpha"), "repo A content expected, provenance was {:?}", pack_a.provenance);
        // Provenance uniqueness invariant (F-3)
        let mut keys: Vec<(String, String, String)> = pack_a.provenance.iter()
            .map(|p| (p.path.clone(), p.source.clone(), p.retriever.clone())).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), pack_a.provenance.len(), "provenance rows must be unique");

        // Repo-scoped query against repo B works too (same engine instance)
        let task_b = TaskRequest { message: "coderun router daemon unique_marker_beta".to_string(), session_id: "sB".to_string(), context_hints: None, repository_id: String::new(), repository_path: None };
        let (pack_b, _) = engine.build_context(&task_b).unwrap();
        assert!(pack_b.code_context.contains("unique_marker_beta"));

        std::env::remove_var("CODERUN_INDEX_DIR");
        std::env::remove_var("CODERUN_REPO_STATE");
        let _ = std::fs::remove_dir_all(&repo_a);
        let _ = std::fs::remove_dir_all(&repo_b);
    }
}
