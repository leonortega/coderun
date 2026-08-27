//! Reranker module — simplified passthrough for v1.
//!
//! ## Why FlashRank was removed
//!
//! Per the 48-task eShopOnWeb benchmark evaluation (August 2026):
//!
//! ```text
//! Baseline BM25:   Recall@5=16.97%  MRR=0.5003  Latency=507ms
//! + FlashRank:     Recall@5=18.94%  MRR=0.4325  Latency=8532ms
//! ```
//!
//! FlashRank provided +1.97pp Recall@5 but degraded MRR by -6.78pp and was
//! 17x slower. The cost-benefit ratio was unacceptable for a real-time path.
//!
//! ### Decision
//!
//! - FlashRank is **offline evaluation only** — not part of the v1 runtime.
//! - The reranker struct is preserved as a **passthrough** for API compatibility.
//! - The context engine's FlashRank reranking section is commented out.
//! - The `ort` feature flag is removed from `coderun-knowledge/Cargo.toml`.
//!
//! ### What actually improves retrieval
//!
//! The same benchmark showed that **index-time representation** improvements
//! beat any post-processing reranker:
//!
//! ```text
//! + PascalCase splitting:    Recall@5=22.19%  (+5.22pp, zero cost)
//! + Symbol name field:       Recall@5=22.62%  (+0.43pp)
//! + Path tokenization:       Recall@5=24.08%  (+1.46pp)
//! ```
//!
//! These are deterministic, fast, and explainable — no neural reranker needed.



// ── Reranking Configuration ──────────────────────────────────────────────

/// Reranker configuration — preserved for API compatibility.
/// In v1, the reranker is a no-op passthrough.
#[derive(Debug, Clone)]
pub struct RerankerConfig {
    /// Whether to use reranking (kept for config compat; always passthrough in v1)
    pub enabled: bool,
    /// Endpoint (unused, kept for config compat)
    pub endpoint: Option<String>,
    /// Model path (unused, kept for config compat)
    pub model_path: Option<String>,
    /// Maximum number of candidates to return
    pub max_candidates: usize,
    /// Timeout in milliseconds (unused in passthrough)
    pub timeout_ms: u64,
}

impl Default for RerankerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: None,
            model_path: None,
            max_candidates: 100,
            timeout_ms: 5000,
        }
    }
}

// ── Reranker ─────────────────────────────────────────────────────────────

/// Reranker — v1 passthrough. Returns candidates in original order.
///
/// FlashRank was removed from the v1 runtime path per benchmark evaluation.
/// See module-level documentation for rationale and numbers.
#[derive(Debug, Clone)]
pub struct Reranker {
    pub config: RerankerConfig,
}

impl Reranker {
    /// Create a new reranker (passthrough)
    pub fn new(config: RerankerConfig) -> Self {
        Self { config }
    }

    /// Rerank search results — v1 passthrough, returns candidates unchanged.
    pub fn rerank(
        &self,
        _query: &str,
        candidates: Vec<RerankCandidate>,
    ) -> Vec<RerankCandidate> {
        candidates
            .into_iter()
            .take(self.config.max_candidates)
            .collect()
    }
}

// ── Data Types ───────────────────────────────────────────────────────────

/// Candidate for reranking (kept for API compatibility)
#[derive(Debug, Clone)]
pub struct RerankCandidate {
    pub id: String,
    pub content: String,
    pub path: String,
    pub language: String,
    pub symbols: Vec<String>,
    pub original_score: f32,
}

/// Reranked result (kept for API compatibility)
#[derive(Debug, Clone)]
pub struct RerankResult {
    pub candidates: Vec<RerankCandidate>,
    pub query: String,
    pub method: String,
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reranker_config_default() {
        let config = RerankerConfig::default();
        assert!(!config.enabled);
        assert!(config.endpoint.is_none());
        assert_eq!(config.max_candidates, 100);
    }

    #[test]
    fn test_rerank_passthrough() {
        let reranker = Reranker::new(RerankerConfig::default());
        let candidates = vec![
            RerankCandidate {
                id: "1".into(),
                content: "test".into(),
                path: "test.rs".into(),
                language: "rust".into(),
                symbols: vec![],
                original_score: 0.5,
            },
            RerankCandidate {
                id: "2".into(),
                content: "test2".into(),
                path: "test2.rs".into(),
                language: "rust".into(),
                symbols: vec![],
                original_score: 0.3,
            },
        ];

        let result = reranker.rerank("test", candidates);
        assert_eq!(result.len(), 2);
        // Passthrough preserves original order
        assert_eq!(result[0].id, "1");
        assert_eq!(result[1].id, "2");
    }

    #[test]
    fn test_rerank_empty_returns_empty() {
        let reranker = Reranker::new(RerankerConfig::default());
        let res = reranker.rerank("query", vec![]);
        assert!(res.is_empty());
    }

    #[test]
    fn test_rerank_max_candidates() {
        let config = RerankerConfig {
            max_candidates: 2,
            ..Default::default()
        };
        let reranker = Reranker::new(config);

        let candidates: Vec<RerankCandidate> = (0..10)
            .map(|i| RerankCandidate {
                id: i.to_string(),
                content: format!("content {}", i),
                path: format!("{}.rs", i),
                language: "rust".into(),
                symbols: vec![],
                original_score: 0.5,
            })
            .collect();

        let result = reranker.rerank("test", candidates);
        assert_eq!(result.len(), 2);
    }
}
