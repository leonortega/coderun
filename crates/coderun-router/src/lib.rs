pub mod litellm;

use coderun_core::{RoutingDecision, RoutingScores};
use tracing::{debug, info};

/// Model Router: heuristic complexity scoring and tier-based model selection
pub struct ModelRouter {
    config: RouterConfig,
}

#[derive(Debug, Clone)]
pub struct RouterConfig {
    pub structural_weight: f64,
    pub semantic_weight: f64,
    pub scope_weight: f64,
    pub fast_threshold: f64,
    pub capable_threshold: f64,
    pub fast_model: String,
    pub balanced_model: String,
    pub capable_model: String,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            structural_weight: 0.3,
            semantic_weight: 0.4,
            scope_weight: 0.3,
            fast_threshold: 0.3,
            capable_threshold: 0.7,
            fast_model: "gpt-4o-mini".to_string(),
            balanced_model: "gpt-4o".to_string(),
            capable_model: "o1".to_string(),
        }
    }
}

/// Request context for model routing
pub struct RoutingRequest {
    pub message: String,
    pub file_count: usize,
    pub symbol_count: usize,
    pub knowledge_entries: usize,
    pub skills_matched: usize,
    pub token_count: usize,
    pub model_override: Option<String>,
}

impl ModelRouter {
    pub fn new(config: RouterConfig) -> Self {
        Self { config }
    }

    /// Select the best model for a given request
    pub fn select_model(&self, request: &RoutingRequest) -> RoutingDecision {
        // If model override is specified, use it directly
        if let Some(ref model) = request.model_override {
            return RoutingDecision {
                model: model.clone(),
                tier: "override".to_string(),
                scores: RoutingScores {
                    structural: 0.0,
                    semantic: 0.0,
                    scope: 0.0,
                    final_score: 0.0,
                },
                reasoning: format!("Model override: {}", model),
            };
        }

        // Compute complexity scores
        let structural = self.compute_structural_complexity(
            request.file_count,
            request.symbol_count,
        );
        let semantic = self.compute_semantic_complexity(&request.message);
        let scope = self.compute_scope_complexity(
            request.knowledge_entries,
            request.skills_matched,
            request.token_count,
        );

        // Weighted final score
        let final_score = structural * self.config.structural_weight
            + semantic * self.config.semantic_weight
            + scope * self.config.scope_weight;

        // Map score to tier
        let (tier, model) = if final_score < self.config.fast_threshold {
            ("fast".to_string(), self.config.fast_model.clone())
        } else if final_score > self.config.capable_threshold {
            ("capable".to_string(), self.config.capable_model.clone())
        } else {
            ("balanced".to_string(), self.config.balanced_model.clone())
        };

        let reasoning = format!(
            "Structural: {:.2}, Semantic: {:.2}, Scope: {:.2}, Final: {:.2} → {}",
            structural, semantic, scope, final_score, tier
        );

        debug!(
            structural = structural,
            semantic = semantic,
            scope = scope,
            final_score = final_score,
            tier = %tier,
            model = %model,
            "Model routing decision"
        );

        info!(model = %model, tier = %tier, score = final_score, "Selected model");

        RoutingDecision {
            model,
            tier,
            scores: RoutingScores {
                structural,
                semantic,
                scope,
                final_score,
            },
            reasoning,
        }
    }

    /// Compute structural complexity (0.0-1.0) based on code structure
    fn compute_structural_complexity(&self, file_count: usize, symbol_count: usize) -> f64 {
        let file_score = (file_count as f64 / 20.0).min(1.0);
        let symbol_score = (symbol_count as f64 / 100.0).min(1.0);
        (file_score + symbol_score) / 2.0
    }

    /// Compute semantic complexity (0.0-1.0) based on message content
    fn compute_semantic_complexity(&self, message: &str) -> f64 {
        let lower = message.to_lowercase();
        let word_count = message.split_whitespace().count();

        // Technical terms increase complexity
        let technical_terms = [
            "refactor", "migrate", "database", "schema", "api",
            "middleware", "authentication", "authorization", "concurrency",
            "parallel", "async", "distributed", "microservice", "architecture",
            "deployment", "infrastructure", "docker", "kubernetes", "terraform",
            "algorithm", "optimization", "performance", "security", "encryption",
            "protocol", "serialization", "deserialization", "cache", "index",
        ];

        let tech_count = technical_terms
            .iter()
            .filter(|term| lower.contains(*term))
            .count();

        // Action verbs increase complexity
        let action_verbs = [
            "implement", "fix", "add", "remove", "refactor", "migrate",
            "optimize", "debug", "test", "deploy", "configure", "integrate",
            "redesign", "rewrite", "scale", "secure", "audit", "benchmark",
        ];

        let action_count = action_verbs
            .iter()
            .filter(|verb| lower.contains(*verb))
            .count();

        // Combine factors
        let length_score = (word_count as f64 / 50.0).min(1.0);
        let tech_score = (tech_count as f64 / 5.0).min(1.0);
        let action_score = (action_count as f64 / 3.0).min(1.0);

        (length_score + tech_score + action_score) / 3.0
    }

    /// Compute scope complexity (0.0-1.0) based on context size
    fn compute_scope_complexity(
        &self,
        knowledge_entries: usize,
        skills_matched: usize,
        token_count: usize,
    ) -> f64 {
        let knowledge_score = (knowledge_entries as f64 / 10.0).min(1.0);
        let skill_score = (skills_matched as f64 / 3.0).min(1.0);
        let token_score = (token_count as f64 / 8000.0).min(1.0);

        (knowledge_score + skill_score + token_score) / 3.0
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_router() -> ModelRouter {
        ModelRouter::new(RouterConfig::default())
    }

    #[test]
    fn test_simple_message_selects_fast() {
        let router = default_router();
        let request = RoutingRequest {
            message: "fix typo".to_string(),
            file_count: 1,
            symbol_count: 2,
            knowledge_entries: 0,
            skills_matched: 0,
            token_count: 100,
            model_override: None,
        };

        let decision = router.select_model(&request);
        assert_eq!(decision.tier, "fast");
        assert_eq!(decision.model, "gpt-4o-mini");
    }

    #[test]
    fn test_complex_message_selects_capable() {
        let router = default_router();
        let request = RoutingRequest {
            message: "refactor the database schema migration to implement distributed microservice architecture with authentication and concurrency handling".to_string(),
            file_count: 15,
            symbol_count: 80,
            knowledge_entries: 8,
            skills_matched: 3,
            token_count: 10000,
            model_override: None,
        };

        let decision = router.select_model(&request);
        assert_eq!(decision.tier, "capable");
        assert_eq!(decision.model, "o1");
    }

    #[test]
    fn test_moderate_message_selects_balanced() {
        let router = default_router();
        let request = RoutingRequest {
            message: "implement a new API endpoint for user management with authentication and database schema migration".to_string(),
            file_count: 10,
            symbol_count: 40,
            knowledge_entries: 5,
            skills_matched: 2,
            token_count: 5000,
            model_override: None,
        };

        let decision = router.select_model(&request);
        assert_eq!(decision.tier, "balanced");
        assert_eq!(decision.model, "gpt-4o");
    }

    #[test]
    fn test_model_override() {
        let router = default_router();
        let request = RoutingRequest {
            message: "fix typo".to_string(),
            file_count: 1,
            symbol_count: 1,
            knowledge_entries: 0,
            skills_matched: 0,
            token_count: 50,
            model_override: Some("custom-model".to_string()),
        };

        let decision = router.select_model(&request);
        assert_eq!(decision.model, "custom-model");
        assert_eq!(decision.tier, "override");
    }

    #[test]
    fn test_structural_complexity() {
        let router = default_router();

        // Low complexity
        let score = router.compute_structural_complexity(1, 5);
        assert!(score < 0.3);

        // High complexity
        let score = router.compute_structural_complexity(20, 100);
        assert!(score > 0.7);
    }

    #[test]
    fn test_semantic_complexity() {
        let router = default_router();

        // Simple message
        let score = router.compute_semantic_complexity("fix typo");
        assert!(score < 0.3);

        // Complex message
        let score = router.compute_semantic_complexity(
            "refactor the database schema migration to implement distributed microservice architecture"
        );
        assert!(score > 0.5);
    }

    #[test]
    fn test_scope_complexity() {
        let router = default_router();

        let score = router.compute_scope_complexity(0, 0, 100);
        assert!(score < 0.2);

        let score = router.compute_scope_complexity(10, 5, 10000);
        assert!(score > 0.5);
    }

    #[test]
    fn test_scores_are_in_range() {
        let router = default_router();
        let request = RoutingRequest {
            message: "implement a new feature with database migration".to_string(),
            file_count: 10,
            symbol_count: 50,
            knowledge_entries: 5,
            skills_matched: 2,
            token_count: 5000,
            model_override: None,
        };

        let decision = router.select_model(&request);
        assert!(decision.scores.structural >= 0.0 && decision.scores.structural <= 1.0);
        assert!(decision.scores.semantic >= 0.0 && decision.scores.semantic <= 1.0);
        assert!(decision.scores.scope >= 0.0 && decision.scores.scope <= 1.0);
        assert!(decision.scores.final_score >= 0.0 && decision.scores.final_score <= 1.0);
    }
}
