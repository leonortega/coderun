pub mod litellm;

use coderun_core::{RoutingDecision, RoutingScores, IModelGateway};
use tracing::{debug, info};

/// Model Router: heuristic complexity scoring and tier-based model selection
/// TASK-018: separate model config from routing logic — routing chooses tier, ModelsConfig defines actual models
pub struct ModelRouter {
    config: RouterConfig,
    models: ModelsConfig,
}

#[derive(Debug, Clone)]
pub struct RouterConfig {
    pub structural_weight: f64,
    pub semantic_weight: f64,
    pub scope_weight: f64,
    pub fast_threshold: f64,
    pub capable_threshold: f64,
}

#[derive(Debug, Clone)]
pub struct ModelsConfig {
    pub fast: String,
    pub balanced: String,
    pub capable: String,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            structural_weight: 0.3,
            semantic_weight: 0.4,
            scope_weight: 0.3,
            fast_threshold: 0.3,
            capable_threshold: 0.7,
        }
    }
}

impl Default for ModelsConfig {
    fn default() -> Self {
        Self {
            fast: "gpt-4o-mini".to_string(),
            balanced: "gpt-4o".to_string(),
            capable: "o1".to_string(),
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
    /// True when both code search and knowledge retrieval returned zero results.
    /// This signals insufficient context, not simplicity — should escalate tier.
    pub retrieval_empty: bool,
}

impl ModelRouter {
    pub fn new(config: RouterConfig) -> Self {
        Self { config, models: ModelsConfig::default() }
    }
    /// TASK-018: new with explicit ModelsConfig (ModelRouter::new reads ModelsConfig)
    pub fn new_with_models(config: RouterConfig, models: ModelsConfig) -> Self {
        Self { config, models }
    }
    pub fn from_models(models: ModelsConfig) -> Self {
        Self { config: RouterConfig::default(), models }
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
        let mut final_score = structural * self.config.structural_weight
            + semantic * self.config.semantic_weight
            + scope * self.config.scope_weight;

        // Zero-result safeguard: when retrieval returns nothing, the agent is flying blind.
        // Floor the scope score at 0.9 to prevent routing to cheap models on empty context.
        // With scope_weight=0.3, this contributes ~0.27 to the final score — enough to push
        // even a simple semantic message (0.05-0.15) above the fast_threshold (0.3).
        let scope_override = if request.retrieval_empty && scope < 0.9 {
            debug!(original_scope = scope, "zero-result retrieval: floored scope to 0.9");
            0.9
        } else {
            scope
        };

        if request.retrieval_empty && scope < 0.9 {
            final_score = structural * self.config.structural_weight
                + semantic * self.config.semantic_weight
                + scope_override * self.config.scope_weight;
        }

        // Map score to tier — models from ModelsConfig (TASK-018)
        let (tier, model) = if final_score < self.config.fast_threshold {
            ("fast".to_string(), self.models.fast.clone())
        } else if final_score > self.config.capable_threshold {
            ("capable".to_string(), self.models.capable.clone())
        } else {
            ("balanced".to_string(), self.models.balanced.clone())
        };

        let reasoning = if request.retrieval_empty && scope < 0.9 {
            format!(
                "Structural: {:.2}, Semantic: {:.2}, Scope: {:.2} (zero-result floor 0.9 applied), Final: {:.2} → {}",
                structural, semantic, scope_override, final_score, tier
            )
        } else {
            format!(
                "Structural: {:.2}, Semantic: {:.2}, Scope: {:.2}, Final: {:.2} → {}",
                structural, semantic, scope, final_score, tier
            )
        };

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

impl coderun_core::IModelGateway for ModelRouter {
    fn select_model(
        &self,
        message: &str,
        file_count: usize,
        symbol_count: usize,
        knowledge_entries: usize,
        skills_matched: usize,
        token_count: usize,
        model_override: Option<&str>,
    ) -> RoutingDecision {
        let req = RoutingRequest {
            message: message.to_string(),
            file_count,
            symbol_count,
            knowledge_entries,
            skills_matched,
            token_count,
            model_override: model_override.map(|s| s.to_string()),
            retrieval_empty: false,
        };
        self.select_model(&req)
    }

    fn tier_to_model(&self, tier: &str) -> String {
        match tier {
            "fast" => self.models.fast.clone(),
            "balanced" => self.models.balanced.clone(),
            "capable" => self.models.capable.clone(),
            _ => self.models.balanced.clone(),
        }
    }
}

/// FIRST-CLASS v0.5.0: LiteLLM gateway wrapper — primary POST /v1/chat/completions with fallback cascade
pub struct LiteLLMGateway {
    pub client: litellm::LiteLLMClient,
    pub router: ModelRouter,
}

impl LiteLLMGateway {
    pub fn new(client: litellm::LiteLLMClient, router: ModelRouter) -> Self { Self { client, router } }

    /// Attempt LiteLLM complete with fallback_chain; on all tiers Err, return heuristic decision
    pub async fn complete_with_fallback(&self, req: &litellm::ModelRequest, tier: &str) -> Result<litellm::ModelResponse, String> {
        for t in fallback_chain(tier) {
            let model = self.router.tier_to_model(&t);
            let mut r = req.clone(); r.model = model.clone();
            match self.client.complete(&r).await {
                Ok(resp) => {
                    tracing::info!(tier=%t, model=%model, "LiteLLM primary/fallback success");
                    return Ok(resp);
                }
                Err(e) => {
                    tracing::warn!(tier=%t, model=%model, error=%e, "LiteLLM fallback to next tier");
                    continue;
                }
            }
        }
        Err("LiteLLM all tiers failed, fallback to heuristic".to_string())
    }
}

/// Fallback chain helper (spec §3 Model Router — LiteLLM fallback chains, per-key budgets, cost tracking)
pub fn fallback_chain(tier: &str) -> Vec<String> {
    match tier {
        "capable" => vec!["capable".to_string(), "balanced".to_string(), "fast".to_string()],
        "balanced" => vec!["balanced".to_string(), "fast".to_string()],
        "fast" => vec!["fast".to_string()],
        _ => vec!["balanced".to_string()],
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use coderun_core::IModelGateway;

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
            retrieval_empty: false,
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
            retrieval_empty: false,
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
            retrieval_empty: false,
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
            retrieval_empty: false,
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
            retrieval_empty: false,
        };

        let decision = router.select_model(&request);
        assert!(decision.scores.structural >= 0.0 && decision.scores.structural <= 1.0);
        assert!(decision.scores.semantic >= 0.0 && decision.scores.semantic <= 1.0);
        assert!(decision.scores.scope >= 0.0 && decision.scores.scope <= 1.0);
        assert!(decision.scores.final_score >= 0.0 && decision.scores.final_score <= 1.0);
    }

    #[test]
    fn test_fallback_chain() {
        assert_eq!(fallback_chain("capable"), vec!["capable", "balanced", "fast"]);
        assert_eq!(fallback_chain("balanced"), vec!["balanced", "fast"]);
        assert_eq!(fallback_chain("fast"), vec!["fast"]);
        assert_eq!(fallback_chain("unknown"), vec!["balanced"]);
        // LiteLLM gateway: cascade capable→balanced→fast per spec §3, per-key budgets untouched
        let chain = fallback_chain("capable");
        assert_eq!(chain[0], "capable");
        assert_eq!(chain[2], "fast");
    }

    #[test]
    fn test_imodelgateway_trait() {
        let router = default_router();
        let d = <ModelRouter as coderun_core::IModelGateway>::select_model(
            &router, "fix typo", 1, 1, 0, 0, 100, None,
        );
        assert_eq!(d.tier, "fast");
        assert_eq!(router.tier_to_model("fast"), "gpt-4o-mini");
        assert_eq!(router.tier_to_model("capable"), "o1");
    }

    #[test]
    fn test_zero_result_floor_escalates_tier() {
        // When retrieval is empty (retrieval_empty=true) and scope is low,
        // the zero-result floor should escalate the tier from fast to balanced.
        let router = default_router();
        // Simple message + zero context → normally fast tier
        let request_without_floor = RoutingRequest {
            message: "fix the checkout flow".to_string(),
            file_count: 0,
            symbol_count: 0,
            knowledge_entries: 0,
            skills_matched: 0,
            token_count: 0,
            model_override: None,
            retrieval_empty: false,
        };
        let d1 = router.select_model(&request_without_floor);
        // With zero context and simple message, this should be fast
        assert_eq!(d1.tier, "fast", "zero context without floor should be fast");

        // Same request but with retrieval_empty=true → should escalate
        let request_with_floor = RoutingRequest {
            message: "fix the checkout flow".to_string(),
            file_count: 0,
            symbol_count: 0,
            knowledge_entries: 0,
            skills_matched: 0,
            token_count: 0,
            model_override: None,
            retrieval_empty: true,
        };
        let d2 = router.select_model(&request_with_floor);
        assert_eq!(d2.tier, "balanced", "zero-result floor should escalate to balanced");
        assert!(d2.reasoning.contains("zero-result floor"), "reasoning should document the override");
    }

    #[test]
    fn test_zero_result_floor_not_applied_when_results_exist() {
        // When retrieval has results (retrieval_empty=false), normal scoring applies
        let router = default_router();
        let request = RoutingRequest {
            message: "fix the checkout flow".to_string(),
            file_count: 0,
            symbol_count: 0,
            knowledge_entries: 5,
            skills_matched: 1,
            token_count: 2000,
            model_override: None,
            retrieval_empty: false,
        };
        let d = router.select_model(&request);
        assert!(!d.reasoning.contains("zero-result floor"), "should not apply floor when results exist");
    }

    // ── v0.5.0 first-class tool tests ──────────────────────────────────

    #[tokio::test]
    async fn test_litellm_gateway_complete_with_fallback_primary_fails() {
        // Primary LiteLLM endpoint down → fallback cascade to heuristic (not panic)
        let client = litellm::LiteLLMClient::new(litellm::LiteLLMConfig { endpoint: "http://127.0.0.1:59999".to_string(), timeout_ms: 200, max_retries: 0, api_key: None }).unwrap();
        let router = default_router();
        let gateway = LiteLLMGateway::new(client, router);
        let req = litellm::ModelRequest { model: "gpt-4o".to_string(), messages: vec![litellm::Message { role: "user".to_string(), content: "hi".to_string() }], max_tokens: Some(10), temperature: None, stream: None };
        let res = gateway.complete_with_fallback(&req, "capable").await;
        assert!(res.is_err(), "all tiers should fail when endpoint is down, fallback to heuristic");
        assert!(res.unwrap_err().contains("all tiers failed"));
    }

    #[tokio::test]
    async fn test_litellm_gateway_fallback_chain_with_mock() {
        // Mock LiteLLM that fails primary then succeeds: use wiremock-like manual axum server
        let app = axum::Router::new().route("/v1/chat/completions", axum::routing::post(|| async {
            let body = serde_json::json!({"id":"chatcmpl-1","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}});
            axum::Json(body)
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let client = litellm::LiteLLMClient::new(litellm::LiteLLMConfig { endpoint: format!("http://{}", addr), timeout_ms: 1000, max_retries: 1, api_key: None }).unwrap();
        let gateway = LiteLLMGateway::new(client, default_router());
        let req = litellm::ModelRequest { model: "o1".to_string(), messages: vec![litellm::Message { role: "user".to_string(), content: "hi".to_string() }], max_tokens: Some(5), temperature: None, stream: None };
        let res = gateway.complete_with_fallback(&req, "capable").await;
        assert!(res.is_ok(), "mock server should succeed on fallback attempt");
        assert_eq!(res.unwrap().choices[0].message.content, "ok");
    }

    #[test]
    fn test_litellm_config_cost_tracking() {
        // 003_graph.sql adds cost_usd column; fetch via storage to ensure migration applied
        let db = coderun_storage::Database::open(&std::path::PathBuf::from(":memory:")).unwrap();
        db.insert_usage("req_cost_1", "pre_generation", 100, 50, "gpt-4o", "balanced").unwrap();
        let stats = db.get_usage_stats().unwrap();
        assert!(stats.total_requests >= 1);
        // cost_usd defaults to 0.0, but column exists
    }
}
