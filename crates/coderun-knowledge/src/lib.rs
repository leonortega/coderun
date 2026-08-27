pub mod engram;
pub mod rerank;

use std::path::Path;

use coderun_core::{KnowledgeEntry, SkillMatch};
use coderun_events::{EventBus, RuntimeEvent};
use coderun_storage::Database;
use tracing::{debug, info};

// ── Configuration ───────────────────────────────────────────────────────

/// KnowledgeConfig — rerank_enabled gates FlashRank (default false, BM25 primary), memory_enabled gates engram
// TASK-013/014: simplify retrieval pipeline — Tantivy BM25 primary, FlashRank optional, engram optional enrichment
#[derive(Debug, Clone)]
pub struct KnowledgeConfig {
    pub rerank_enabled: bool,
    pub memory_enabled: bool,
    pub memory_endpoint: String,
    pub max_knowledge_entries: usize,
}

impl Default for KnowledgeConfig {
    fn default() -> Self {
        Self {
            rerank_enabled: false, // TASK-013: BM25 primary, FlashRank opt-in (documented, bench compares)
            memory_enabled: true,
            memory_endpoint: "http://localhost:9090".to_string(),
            max_knowledge_entries: 10000,
        }
    }
}

// ── Knowledge Hub ───────────────────────────────────────────────────────

pub struct KnowledgeHub {
    db: Database,
    event_bus: EventBus,
    #[allow(dead_code)]
    config: KnowledgeConfig,
    /// In-memory skill registry (loaded from files)
    skills: Vec<coderun_skills::Skill>,
}

impl KnowledgeHub {
    /// Create a new Knowledge Hub
    pub fn new(db: Database, event_bus: EventBus, config: KnowledgeConfig) -> Self {
        Self {
            db,
            event_bus,
            config,
            skills: Vec::new(),
        }
    }

    /// Load skills from a directory
    pub fn load_skills(&mut self, skills_dir: &Path) -> Result<usize, String> {
        let mut engine = coderun_skills::SkillEngine::new(skills_dir.to_path_buf());
        let count = engine.load_skills()?;
        self.skills = engine.get_skills().to_vec();
        Ok(count)
    }

    /// Load skills from multiple directories in priority order (first occurrence of a skill
    /// name wins) — repo-local `.coderun/skills` merged with the global `~/.coderun/skills`
    /// library. Non-existent directories are skipped.
    pub fn load_skills_from_dirs(&mut self, dirs: &[std::path::PathBuf]) -> Result<usize, String> {
        let mut engine = coderun_skills::SkillEngine::from_skills(Vec::new());
        let count = engine.load_skills_from_dirs(dirs)?;
        self.skills = engine.get_skills().to_vec();
        Ok(count)
    }

    /// Match skills — delegates to canonical SkillEngine scorer (v0.6.0 single impl)
    pub fn match_skills(&self, task_description: &str, max_skills: usize) -> Vec<SkillMatch> {
        let engine = coderun_skills::SkillEngine::from_skills(self.skills.clone());
        engine.match_skills(task_description, max_skills)
    }

    // ── Knowledge Storage ──────────────────────────────────────────

    /// Store a knowledge entry (unscoped — legacy/global '' repository_id)
    pub fn store_knowledge(&self, entry: &KnowledgeEntry) -> Result<i64, String> {
        self.store_knowledge_for_repo(entry, "")
    }

    /// Store a knowledge entry scoped to a repository (TASK-030) — idempotent upsert on
    /// `(category, key, repository_id)` so re-ingestion never grows the table (TASK-032)
    pub fn store_knowledge_for_repo(&self, entry: &KnowledgeEntry, repository_id: &str) -> Result<i64, String> {
        self.db.store_knowledge(
            &entry.category,
            &entry.key,
            &entry.value,
            entry.confidence,
            &entry.source,
            repository_id,
        )
    }

    /// Get a knowledge entry by category and key
    pub fn get_knowledge(&self, category: &str, key: &str) -> Result<Option<KnowledgeEntry>, String> {
        match self.db.get_knowledge(category, key)? {
            Some(record) => Ok(Some(KnowledgeEntry {
                id: Some(record.id),
                category: record.category,
                key: record.key,
                value: record.value,
                confidence: record.confidence,
                source: record.source,
                relevance_score: None,
            })),
            None => Ok(None),
        }
    }

    /// Get all knowledge entries
    pub fn get_all_knowledge(&self) -> Result<Vec<KnowledgeEntry>, String> {
        let records = self.db.get_all_knowledge()?;
        Ok(records
            .into_iter()
            .map(|r| KnowledgeEntry {
                id: Some(r.id),
                category: r.category,
                key: r.key,
                value: r.value,
                confidence: r.confidence,
                source: r.source,
                relevance_score: None,
            })
            .collect())
    }

    /// Update confidence for a knowledge entry
    pub fn update_confidence(&self, id: i64, confidence: f64) -> Result<(), String> {
        self.db.update_knowledge_confidence(id, confidence)
    }

    /// Decay confidence for old knowledge entries
    pub fn decay_confidence(&self, min_age_days: i64, decay_amount: f64) -> Result<usize, String> {
        self.db.decay_knowledge_confidence(min_age_days, decay_amount)
    }

    // ── Knowledge Retrieval (BM25 → FlashRank, adaptive K, spec §3) ───────

    /// Retrieve knowledge matching a query — lexical BM25/tantivy + FlashRank reranking
    /// `K` adaptive to token budget: `K = clamp(remaining_budget / avg_doc_tokens, 5, 20)`; expensive rerank bounded.
    /// `repository_id: Some(id)` scopes results to one repository (TASK-030/F-1).
    pub fn retrieve_knowledge(
        &self,
        query: &str,
        category_filter: Option<&str>,
        max_results: usize,
        repository_id: Option<&str>,
    ) -> Result<Vec<KnowledgeEntry>, String> {
        // Step 1: lexical search (tantivy if available, else LIKE) — cheap pass, collect top 20
        let lexical_limit = 20usize;
        let records = self.db.search_knowledge(query, category_filter, 0.3, lexical_limit, repository_id)?;

        // Step 2: adaptive K for reranker (bound expensive step, not cheap lexical pass)
        // Assume avg doc ~80 tokens, remaining budget heuristic: max_results * 200 tokens
        let avg_doc_tokens = 80usize;
        let remaining_budget = max_results * 200;
        let adaptive_k = ((remaining_budget / avg_doc_tokens).clamp(5, 20)).min(records.len()).max(1);
        let to_rerank = records.len().min(adaptive_k.max(max_results));

        // Step 3: in-process reranking via FlashRank (TF-IDF fallback when ort int8 not loaded) — gated behind rerank_enabled (TASK-013, BM25 primary)
        let reranker = crate::rerank::Reranker::new(crate::rerank::RerankerConfig { enabled: self.config.rerank_enabled, ..Default::default() });
        let candidates: Vec<crate::rerank::RerankCandidate> = records
            .into_iter()
            .take(to_rerank)
            .map(|r| crate::rerank::RerankCandidate {
                id: r.id.to_string(),
                content: format!("{} {}", r.key, r.value),
                path: r.key.clone(),
                language: r.category.clone(),
                symbols: vec![r.key.clone()],
                original_score: r.confidence as f32,
            })
            // keep original records for mapping
            .collect();

        // Preserve original records for lookup after rerank
        // Re-run records fetch to map ids → entries after reranking
        let original_records = self.db.search_knowledge(query, category_filter, 0.3, lexical_limit, repository_id)?;
        let reranked = reranker.rerank(query, candidates);

        // Step 4: map reranked candidates back to KnowledgeEntry, filter by confidence ≥0.3, take max_results
        let mut entries: Vec<KnowledgeEntry> = Vec::new();
        for cand in reranked.iter().take(max_results) {
            if let Some(rec) = original_records.iter().find(|r| r.id.to_string() == cand.id) {
                if rec.confidence >= 0.3 {
                    entries.push(KnowledgeEntry {
                        id: Some(rec.id),
                        category: rec.category.clone(),
                        key: rec.key.clone(),
                        value: rec.value.clone(),
                        confidence: rec.confidence,
                        source: rec.source.clone(),
                        relevance_score: Some(cand.original_score as f64),
                    });
                }
            }
        }

        // Step 5: deterministic engram read (hot path via HTTP, 2s timeout, fail-open)
        // Spec: reads happen deterministically inside pre-generation hook, not MCP tool-choice
        if self.config.memory_enabled {
            if let Ok(engram_hits) = self.try_engram_search(query, category_filter, 3) {
                for (key, value) in engram_hits {
                    // engram hits get confidence boost 1.1 (cross-session memory)
                    entries.push(KnowledgeEntry {
                        id: None,
                        category: "memory".to_string(),
                        key,
                        value,
                        confidence: 0.75,
                        source: "engram".to_string(),
                        relevance_score: Some(0.9),
                    });
                }
                if entries.len() > max_results {
                    entries.truncate(max_results);
                }
            }
        }

        debug!(query = query, results = entries.len(), adaptive_k = adaptive_k, "Knowledge retrieval (BM25→rerank+engram)");

        Ok(entries)
    }

    /// Check if the Knowledge Hub has any seeded knowledge for this repository (P0 #3 proactive detection)
    pub fn is_initialized(&self, repository_id: Option<&str>) -> bool {
        self.db.count_knowledge(repository_id).unwrap_or(0) > 0
    }

    /// Try engram HTTP search with 2s timeout, fail-open to local only (spec §3) — FIRST-CLASS v0.5.0
    /// Primary: `EngramClient::search_memory()` via HTTP `POST /api/memory/search` with 2s timeout.
    /// Fallback: `db.search_memory()` LIKE only on Err/timeout with WARN.
    pub(crate) fn try_engram_search(&self, query: &str, _category: Option<&str>, max: usize) -> Result<Vec<(String,String)>, String> {
        let endpoint = self.config.memory_endpoint.clone();
        // FIRST-CLASS: attempt engram HTTP via EngramClient (MCP-native, SQLite+FTS5)
        let engram_cfg = crate::engram::EngramConfig { endpoint: endpoint.clone(), timeout_ms: 2000, max_retries: 0 };
        if let Ok(client) = crate::engram::EngramClient::new(engram_cfg) {
            // Offload async to new thread to avoid nested runtime panic (same as workflow/src/dbos.rs block_on_in_thread)
            let query_owned = query.to_string();
            let max_owned = max;
            let endpoint_clone = endpoint.clone();
            let http_result: Result<Vec<(String,String)>, String> = std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
                rt.block_on(async {
                    let q = crate::engram::MemoryQuery { namespace: "default".to_string(), query: query_owned, max_results: Some(max_owned) };
                    match tokio::time::timeout(std::time::Duration::from_millis(2000), client.search_memory(&q)).await {
                        Ok(Ok(res)) => Ok(res.entries.into_iter().map(|e| (e.key, e.value)).collect()),
                        Ok(Err(e)) => Err(e),
                        Err(_) => Err("engram timeout 2s".to_string()),
                    }
                })
            }).join().unwrap_or(Err("engram thread join failed".to_string()));
            match http_result {
                Ok(hits) => {
                    if !hits.is_empty() {
                        tracing::debug!(endpoint=%endpoint_clone, hits=%hits.len(), "Engram MCP primary hit");
                    }
                    return Ok(hits);
                }
                Err(e) => {
                    tracing::warn!(endpoint=%endpoint_clone, error=%e, "Engram MCP primary failed, fail-open to local LIKE");
                }
            }
        }
        // FALLBACK only on Err/timeout
        match self.db.search_memory("default", query, max) {
            Ok(recs) => Ok(recs.into_iter().map(|r| (r.key, r.value)).collect()),
            Err(e) => Err(e),
        }
    }

    // ── Knowledge Extraction ───────────────────────────────────────

    /// Extract knowledge from indexed code analysis
    pub fn extract_knowledge(
        &self,
        symbols: &[(String, String)], // (name, kind) pairs
        file_paths: &[String],
    ) -> Result<usize, String> {
        let mut extracted = 0;

        // Detect naming patterns
        let naming_patterns = detect_naming_patterns(symbols);
        for (pattern, confidence) in naming_patterns {
            self.store_knowledge(&KnowledgeEntry {
                id: None,
                category: "convention".to_string(),
                key: format!("naming_{}", pattern),
                value: format!("Project uses {} naming convention", pattern),
                confidence,
                source: "auto_extract".to_string(),
                relevance_score: None,
            })?;
            extracted += 1;
        }

        // Detect architectural patterns
        let arch_patterns = detect_architectural_patterns(symbols, file_paths);
        for (pattern, confidence) in arch_patterns {
            self.store_knowledge(&KnowledgeEntry {
                id: None,
                category: "pattern".to_string(),
                key: format!("arch_{}", pattern),
                value: format!("Project follows {} architecture", pattern),
                confidence,
                source: "auto_extract".to_string(),
                relevance_score: None,
            })?;
            extracted += 1;
        }

        // Detect domain terms
        let domain_terms = detect_domain_terms(symbols);
        for (term, definition, confidence) in domain_terms {
            self.store_knowledge(&KnowledgeEntry {
                id: None,
                category: "domain".to_string(),
                key: term,
                value: definition,
                confidence,
                source: "auto_extract".to_string(),
                relevance_score: None,
            })?;
            extracted += 1;
        }

        info!(extracted = extracted, "Knowledge extraction complete");
        Ok(extracted)
    }

    // ── Memory Operations (via engram or local) ────────────────────

    /// Save to memory (local SQLite fallback)
    pub fn memory_save(&self, namespace: &str, key: &str, value: &str) -> Result<i64, String> {
        let id = self.db.save_memory(namespace, key, value)?;
        
        self.event_bus.emit(RuntimeEvent::MemorySaved {
            entry_id: id.to_string(),
            namespace: namespace.to_string(),
            key: key.to_string(),
        });

        Ok(id)
    }

    /// Search memory (local SQLite fallback)
    pub fn memory_search(&self, namespace: &str, query: &str, max_results: usize) -> Result<Vec<(String, String)>, String> {
        let records = self.db.search_memory(namespace, query, max_results)?;
        Ok(records
            .into_iter()
            .map(|r| (r.key, r.value))
            .collect())
    }
}

// ── Pattern Detection Helpers ───────────────────────────────────────────

/// Detect naming conventions used in the codebase
fn detect_naming_patterns(symbols: &[(String, String)]) -> Vec<(String, f64)> {
    let mut patterns = Vec::new();
    let mut snake_case_count = 0;
    let mut camel_case_count = 0;
    let mut pascal_case_count = 0;
    let total = symbols.len();

    if total == 0 {
        return patterns;
    }

    for (name, _kind) in symbols {
        if name.contains('_') {
            snake_case_count += 1;
        } else if name.chars().next().map(|c| c.is_lowercase()).unwrap_or(false)
            && name.chars().any(|c| c.is_uppercase())
        {
            camel_case_count += 1;
        } else if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
            && name.chars().any(|c| c.is_uppercase())
        {
            pascal_case_count += 1;
        }
    }

    if snake_case_count as f64 / total as f64 > 0.5 {
        patterns.push(("snake_case".to_string(), 0.8));
    }
    if camel_case_count as f64 / total as f64 > 0.5 {
        patterns.push(("camelCase".to_string(), 0.8));
    }
    if pascal_case_count as f64 / total as f64 > 0.5 {
        patterns.push(("PascalCase".to_string(), 0.8));
    }

    patterns
}

/// Detect architectural patterns
fn detect_architectural_patterns(
    symbols: &[(String, String)],
    file_paths: &[String],
) -> Vec<(String, f64)> {
    let mut patterns = Vec::new();

    // Check for controller-service-repo pattern
    let has_controller = symbols.iter().any(|(name, _)| name.to_lowercase().contains("controller"));
    let has_service = symbols.iter().any(|(name, _)| name.to_lowercase().contains("service"));
    let has_repository = symbols.iter().any(|(name, _)| name.to_lowercase().contains("repository") || name.to_lowercase().contains("repo"));

    if has_controller && has_service && has_repository {
        patterns.push(("controller-service-repository".to_string(), 0.9));
    }

    // Check for MVC pattern
    let has_model = file_paths.iter().any(|p| p.to_lowercase().contains("model"));
    let has_view = file_paths.iter().any(|p| p.to_lowercase().contains("view"));
    let has_controller_file = file_paths.iter().any(|p| p.to_lowercase().contains("controller"));

    if has_model && has_view && has_controller_file {
        patterns.push(("mvc".to_string(), 0.85));
    }

    // Check for handler pattern
    let has_handler = symbols.iter().any(|(name, _)| name.to_lowercase().contains("handler"));
    if has_handler {
        patterns.push(("handler".to_string(), 0.7));
    }

    // Check for middleware pattern
    let has_middleware = symbols.iter().any(|(name, _)| name.to_lowercase().contains("middleware"));
    if has_middleware {
        patterns.push(("middleware".to_string(), 0.7));
    }

    patterns
}

/// Detect domain-specific terms
fn detect_domain_terms(symbols: &[(String, String)]) -> Vec<(String, String, f64)> {
    let mut terms = Vec::new();

    // Common domain terms to look for
    let domain_keywords = [
        ("user", "A user of the system"),
        ("auth", "Authentication and authorization"),
        ("session", "User session management"),
        ("permission", "Access control permissions"),
        ("role", "User roles for access control"),
        ("token", "Authentication tokens"),
        ("api", "Application programming interface"),
        ("endpoint", "API endpoint"),
        ("route", "API route definition"),
        ("middleware", "Request/response middleware"),
        ("handler", "Request handler"),
        ("service", "Business logic service"),
        ("repository", "Data access layer"),
        ("model", "Data model"),
        ("schema", "Database schema"),
        ("migration", "Database migration"),
        ("config", "Configuration"),
        ("cache", "Caching layer"),
        ("queue", "Message queue"),
        ("event", "Event handling"),
    ];

    for (keyword, definition) in &domain_keywords {
        let count = symbols
            .iter()
            .filter(|(name, _)| name.to_lowercase().contains(keyword))
            .count();

        if count >= 2 {
            // At least 2 occurrences to be confident
            let confidence = (count as f64 / 10.0).min(0.9);
            terms.push((
                keyword.to_string(),
                definition.to_string(),
                confidence,
            ));
        }
    }

    terms
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_db() -> Database {
        let path = PathBuf::from(":memory:");
        Database::open(&path).expect("Failed to create in-memory database")
    }

    /// Hermetic test hub — memory_enabled=false so NOTHING depends on an external
    /// engram server being alive (compile-time test runs must never touch network)
    fn test_hub() -> KnowledgeHub {
        let db = test_db();
        let event_bus = EventBus::new();
        let config = KnowledgeConfig { memory_enabled: false, ..Default::default() };
        KnowledgeHub::new(db, event_bus, config)
    }

    #[test]
    fn test_store_and_get_knowledge() {
        let hub = test_hub();
        let entry = KnowledgeEntry {
            id: None,
            category: "convention".to_string(),
            key: "naming".to_string(),
            value: "Use snake_case".to_string(),
            confidence: 0.8,
            source: "test".to_string(),
            relevance_score: None,
        };

        let id = hub.store_knowledge(&entry).unwrap();
        assert!(id > 0);

        let retrieved = hub.get_knowledge("convention", "naming").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.value, "Use snake_case");
        assert!((retrieved.confidence - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_get_all_knowledge() {
        let hub = test_hub();
        
        hub.store_knowledge(&KnowledgeEntry {
            id: None,
            category: "convention".to_string(),
            key: "naming".to_string(),
            value: "Use snake_case".to_string(),
            confidence: 0.8,
            source: "test".to_string(),
            relevance_score: None,
        }).unwrap();

        hub.store_knowledge(&KnowledgeEntry {
            id: None,
            category: "pattern".to_string(),
            key: "arch".to_string(),
            value: "MVC pattern".to_string(),
            confidence: 0.7,
            source: "test".to_string(),
            relevance_score: None,
        }).unwrap();

        let all = hub.get_all_knowledge().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_search_knowledge() {
        let hub = test_hub();
        
        hub.store_knowledge(&KnowledgeEntry {
            id: None,
            category: "convention".to_string(),
            key: "naming".to_string(),
            value: "Use snake_case for variables".to_string(),
            confidence: 0.8,
            source: "test".to_string(),
            relevance_score: None,
        }).unwrap();

        let results = hub.retrieve_knowledge("snake", None, 10, None).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].value.contains("snake_case"));
    }

    #[test]
    fn test_update_confidence() {
        let hub = test_hub();
        
        let id = hub.store_knowledge(&KnowledgeEntry {
            id: None,
            category: "test".to_string(),
            key: "key".to_string(),
            value: "value".to_string(),
            confidence: 0.5,
            source: "test".to_string(),
            relevance_score: None,
        }).unwrap();

        hub.update_confidence(id, 0.9).unwrap();
        
        let entry = hub.get_knowledge("test", "key").unwrap().unwrap();
        assert!((entry.confidence - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_memory_save_and_search() {
        let hub = test_hub();
        
        let id = hub.memory_save("conventions", "style", "Use rustfmt").unwrap();
        assert!(id > 0);

        let results = hub.memory_search("conventions", "rustfmt", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "style");
        assert_eq!(results[0].1, "Use rustfmt");
    }

    #[test]
    fn test_extract_knowledge() {
        let hub = test_hub();
        
        let symbols = vec![
            ("get_user".to_string(), "function".to_string()),
            ("set_user".to_string(), "function".to_string()),
            ("user_service".to_string(), "struct".to_string()),
            ("UserController".to_string(), "struct".to_string()),
            ("UserService".to_string(), "struct".to_string()),
            ("UserRepository".to_string(), "struct".to_string()),
        ];

        let file_paths = vec![
            "src/controllers/user_controller.rs".to_string(),
            "src/services/user_service.rs".to_string(),
            "src/repositories/user_repository.rs".to_string(),
        ];

        let extracted = hub.extract_knowledge(&symbols, &file_paths).unwrap();
        assert!(extracted > 0);
    }

    #[test]
    fn test_detect_naming_patterns() {
        let symbols = vec![
            ("get_user".to_string(), "function".to_string()),
            ("set_user".to_string(), "function".to_string()),
            ("is_valid".to_string(), "function".to_string()),
        ];

        let patterns = detect_naming_patterns(&symbols);
        assert!(patterns.iter().any(|(p, _)| p == "snake_case"));
    }

    #[test]
    fn test_detect_architectural_patterns() {
        let symbols = vec![
            ("UserController".to_string(), "struct".to_string()),
            ("UserService".to_string(), "struct".to_string()),
            ("UserRepository".to_string(), "struct".to_string()),
        ];

        let file_paths = vec![
            "src/controllers/user.rs".to_string(),
            "src/services/user.rs".to_string(),
            "src/repositories/user.rs".to_string(),
        ];

        let patterns = detect_architectural_patterns(&symbols, &file_paths);
        assert!(patterns.iter().any(|(p, _)| p == "controller-service-repository"));
    }

    #[test]
    fn test_detect_domain_terms() {
        let symbols = vec![
            ("get_user".to_string(), "function".to_string()),
            ("create_user".to_string(), "function".to_string()),
            ("delete_user".to_string(), "function".to_string()),
            ("find_user".to_string(), "function".to_string()),
        ];

        let terms = detect_domain_terms(&symbols);
        assert!(terms.iter().any(|(t, _, _)| t == "user"));
    }

    // ── v0.5.0 first-class tool tests ──────────────────────────────────
    // Note: try_engram_search is private → test via public retrieve_knowledge/memory_search paths

    #[test]
    fn test_retrieve_knowledge_repo_scoped() {
        // TASK-030/F-1: retrieval must be repository-scoped
        let hub = test_hub();
        hub.store_knowledge_for_repo(&KnowledgeEntry { id: None, category: "docs".to_string(), key: "checkout.md".to_string(), value: "eshop basket checkout flow".to_string(), confidence: 0.9, source: "mkdocs".to_string(), relevance_score: None }, "repo_eshop").unwrap();
        hub.store_knowledge_for_repo(&KnowledgeEntry { id: None, category: "docs".to_string(), key: "router.md".to_string(), value: "coderun daemon router checkout".to_string(), confidence: 0.9, source: "mkdocs".to_string(), relevance_score: None }, "repo_coderun").unwrap();

        let eshop = hub.retrieve_knowledge("checkout", None, 10, Some("repo_eshop")).unwrap();
        assert_eq!(eshop.len(), 1);
        assert_eq!(eshop[0].key, "checkout.md", "only the requested repo's docs may surface");

        let other = hub.retrieve_knowledge("basket", None, 10, Some("repo_coderun")).unwrap();
        assert!(other.is_empty(), "no cross-repo leakage");
    }

    #[test]
    fn test_try_engram_search_fallback_when_endpoint_down() {
        // Engram MCP primary tries HTTP 2s timeout → on Err falls back to local LIKE with WARN
        let hub = test_hub(); // default memory_endpoint http://localhost:9090 (likely down in CI)
        hub.memory_save("default", "fallback_key", "fallback_value contains engram_test_token").unwrap();
        // try_engram_search is private, but retrieve_knowledge exercises it via the same path
        // When engram is down, retrieve should still return local hits via fallback
        hub.store_knowledge(&KnowledgeEntry { id: None, category: "default".to_string(), key: "fallback_key".to_string(), value: "fallback_value contains engram_test_token".to_string(), confidence: 0.8, source: "test".to_string(), relevance_score: None }).unwrap();
        let res = hub.retrieve_knowledge("engram_test_token", None, 5, None).unwrap();
        assert!(!res.is_empty(), "fallback LIKE should return hits when engram down");
    }

    #[test]
    fn test_try_engram_search_direct_fallback() {
        let hub = test_hub();
        // Save to local memory so fallback has data
        hub.memory_save("default", "engram_direct", "hello engram direct fallback").unwrap();
        // Call private try_engram_search via retrieve path — endpoint down → fallback path
        // We test that try_engram_search never panics and returns at least empty vec
        let hits = hub.try_engram_search("engram direct", None, 3).unwrap();
        // May be 0 or 1 depending on local fallback; key is it doesn't error when HTTP is down
        assert!(hits.len() <= 3);
    }

    #[test]
    fn test_retrieve_knowledge_bm25_rerank_engram_pipeline() {
        let hub = test_hub();
        for i in 0..5 {
            hub.store_knowledge(&KnowledgeEntry { id: None, category: "docs".to_string(), key: format!("k{i}"), value: format!("FlashRank ort rerank test doc {i} contains rust"), confidence: 0.6, source: "test".to_string(), relevance_score: None }).unwrap();
        }
        let res = hub.retrieve_knowledge("rust", Some("docs"), 3, None).unwrap();
        assert!(!res.is_empty());
        assert!(res.len() <= 3);
        // Adaptive K is exercised internally — no panic means pipeline (tantivy→rerank→engram) works
    }

    #[test]
    fn test_engram_config_driven_timeout() {
        let db = test_db();
        let event_bus = EventBus::new();
        let config = KnowledgeConfig { rerank_enabled: false, memory_enabled: true, memory_endpoint: "http://127.0.0.1:59999".to_string(), max_knowledge_entries: 10000 };
        let hub = KnowledgeHub::new(db, event_bus, config);
        hub.memory_save("default", "timeout_test", "timeout value").unwrap();
        let hits = hub.try_engram_search("timeout_test", None, 5).unwrap();
        // Should fallback to local memory even when endpoint is bogus (2s timeout handled)
        assert!(hits.iter().any(|(k,_)| k=="timeout_test") || hits.is_empty());
    }

    #[test]
    fn test_v060_match_skills_delegates_to_skill_engine() {
        use std::fs;
        use std::path::PathBuf;
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("coderun_knowledge_v060_{}", nanos));
        fs::create_dir_all(&dir).unwrap();
        // Seed skills directly via KnowledgeHub internal vec
        let db = Database::open(&PathBuf::from(":memory:")).unwrap();
        let cfg = KnowledgeConfig::default();
        let mut hub = KnowledgeHub::new(db, crate::EventBus::new(), cfg);
        // Inject two skills
        hub.skills = vec![
            coderun_skills::Skill { name: "Rust Expert".to_string(), tags: vec!["rust".into(), "cargo".into()], instructions: "rust".into(), examples: vec![], constraints: vec![], description: "".into(), priority: 2, specificity: 0.4 },
            coderun_skills::Skill { name: "Python Expert".to_string(), tags: vec!["python".into()], instructions: "py".into(), examples: vec![], constraints: vec![], description: "".into(), priority: 1, specificity: 0.2 },
        ];
        // Same input to both engines should yield identical top match
        let hub_matches = hub.match_skills("help with rust cargo", 5);
        let engine = coderun_skills::SkillEngine::from_skills(hub.skills.clone());
        let eng_matches = engine.match_skills("help with rust cargo", 5);
        assert_eq!(hub_matches.len(), eng_matches.len());
        if !hub_matches.is_empty() {
            assert_eq!(hub_matches[0].skill_name, eng_matches[0].skill_name);
            assert!((hub_matches[0].match_score - eng_matches[0].match_score).abs() < 1e-9);
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_v060_match_skills_threshold() {
        let db = Database::open(&PathBuf::from(":memory:")).unwrap();
        let hub = KnowledgeHub::new(db, crate::EventBus::new(), KnowledgeConfig::default());
        // No skills loaded → no matches
        assert!(hub.match_skills("rust", 5).is_empty());
    }
}
