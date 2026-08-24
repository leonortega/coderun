use tracing::debug;

// ── Reranking Configuration ──────────────────────────────────────────────

/// Reranker configuration
#[derive(Debug, Clone)]
pub struct RerankerConfig {
    /// Whether to use FlashRank for reranking
    pub enabled: bool,
    /// FlashRank endpoint (if using HTTP API)
    pub endpoint: Option<String>,
    /// Maximum number of candidates to rerank
    pub max_candidates: usize,
    /// Timeout in milliseconds
    pub timeout_ms: u64,
}

impl Default for RerankerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: None,
            max_candidates: 100,
            timeout_ms: 5000,
        }
    }
}

// ── Reranker ─────────────────────────────────────────────────────────────

/// Reranker for improving search result quality
pub struct Reranker {
    config: RerankerConfig,
}

impl Reranker {
    /// Create a new reranker
    pub fn new(config: RerankerConfig) -> Self {
        Self { config }
    }

    /// Rerank search results based on query relevance
    pub fn rerank(
        &self,
        query: &str,
        candidates: Vec<RerankCandidate>,
    ) -> Vec<RerankCandidate> {
        if !self.config.enabled || candidates.is_empty() {
            return candidates;
        }

        // Simple TF-IDF-like scoring for now
        // In production, this would call FlashRank or use a trained model
        let query_terms: Vec<String> = query
            .to_lowercase()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        let mut scored: Vec<(f64, RerankCandidate)> = candidates
            .into_iter()
            .map(|candidate| {
                let score = self.compute_relevance_score(&query_terms, &candidate);
                (score, candidate)
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let original_count = scored.len();

        // Take top N
        let results: Vec<RerankCandidate> = scored
            .into_iter()
            .take(self.config.max_candidates)
            .map(|(_, candidate)| candidate)
            .collect();

        debug!(
            query = query,
            original_count = original_count,
            reranked_count = results.len(),
            "Reranking complete"
        );

        results
    }

    /// Compute relevance score between query and candidate
    fn compute_relevance_score(&self, query_terms: &[String], candidate: &RerankCandidate) -> f64 {
        let content_lower = candidate.content.to_lowercase();
        let mut score = 0.0;

        // Term frequency scoring
        for term in query_terms {
            if content_lower.contains(term.as_str()) {
                score += 1.0;
            }
        }

        // Normalize by query length
        if !query_terms.is_empty() {
            score /= query_terms.len() as f64;
        }

        // Boost score if content is shorter (more focused)
        let length_penalty = 1.0 / (1.0 + candidate.content.len() as f64 / 1000.0);
        score *= length_penalty;

        // Boost score if it has symbols
        if !candidate.symbols.is_empty() {
            score *= 1.2;
        }

        score
    }
}

// ── Data Types ───────────────────────────────────────────────────────────

/// Candidate for reranking
#[derive(Debug, Clone)]
pub struct RerankCandidate {
    pub id: String,
    pub content: String,
    pub path: String,
    pub language: String,
    pub symbols: Vec<String>,
    pub original_score: f32,
}

/// Reranked result
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
    fn test_rerank_disabled() {
        let config = RerankerConfig {
            enabled: false,
            ..Default::default()
        };
        let reranker = Reranker::new(config);

        let candidates = vec![
            RerankCandidate {
                id: "1".to_string(),
                content: "test content".to_string(),
                path: "test.rs".to_string(),
                language: "rust".to_string(),
                symbols: vec![],
                original_score: 0.5,
            },
        ];

        let result = reranker.rerank("test", candidates);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_rerank_enabled() {
        let config = RerankerConfig {
            enabled: true,
            max_candidates: 10,
            ..Default::default()
        };
        let reranker = Reranker::new(config);

        let candidates = vec![
            RerankCandidate {
                id: "1".to_string(),
                content: "unrelated content".to_string(),
                path: "a.rs".to_string(),
                language: "rust".to_string(),
                symbols: vec![],
                original_score: 0.5,
            },
            RerankCandidate {
                id: "2".to_string(),
                content: "test function implementation".to_string(),
                path: "b.rs".to_string(),
                language: "rust".to_string(),
                symbols: vec!["test".to_string()],
                original_score: 0.3,
            },
        ];

        let result = reranker.rerank("test", candidates);
        assert_eq!(result.len(), 2);
        // The second candidate should be ranked higher due to matching terms
        assert_eq!(result[0].id, "2");
    }

    #[test]
    fn test_rerank_max_candidates() {
        let config = RerankerConfig {
            enabled: true,
            max_candidates: 2,
            ..Default::default()
        };
        let reranker = Reranker::new(config);

        let candidates: Vec<RerankCandidate> = (0..10)
            .map(|i| RerankCandidate {
                id: i.to_string(),
                content: format!("content {}", i),
                path: format!("{}.rs", i),
                language: "rust".to_string(),
                symbols: vec![],
                original_score: 0.5,
            })
            .collect();

        let result = reranker.rerank("test", candidates);
        assert_eq!(result.len(), 2);
    }
}
