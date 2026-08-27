use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

// ── Configuration ────────────────────────────────────────────────────────

/// LiteLLM client configuration
#[derive(Debug, Clone)]
pub struct LiteLLMConfig {
    /// LiteLLM server endpoint (e.g., "http://localhost:4000")
    pub endpoint: String,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Maximum number of retries
    pub max_retries: u32,
    /// API key (optional, for authenticated endpoints)
    pub api_key: Option<String>,
}

impl Default for LiteLLMConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:4000".to_string(),
            timeout_ms: 30000,
            max_retries: 2,
            api_key: None,
        }
    }
}

// ── Data Types ───────────────────────────────────────────────────────────

/// Model request for LiteLLM
#[derive(Debug, Clone, Serialize)]
pub struct ModelRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// Model response from LiteLLM
#[derive(Debug, Clone, Deserialize)]
pub struct ModelResponse {
    pub id: String,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

/// Response choice
#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: Message,
    pub finish_reason: Option<String>,
}

/// Token usage
#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Model info from LiteLLM
#[derive(Debug, Clone, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub max_tokens: u32,
}

// ── LiteLLM Client ───────────────────────────────────────────────────────

/// HTTP client for LiteLLM model gateway
pub struct LiteLLMClient {
    config: LiteLLMConfig,
    http_client: reqwest::Client,
}

impl LiteLLMClient {
    /// Create a new LiteLLM client
    pub fn new(config: LiteLLMConfig) -> Result<Self, String> {
        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(config.timeout_ms));

        // Configure automatic retries on connection errors and 5xx responses
        let endpoint_host = config.endpoint
            .replace("http://", "")
            .replace("https://", "")
            .split('/')
            .next()
            .unwrap_or("localhost")
            .to_string();
        let retry_policy = reqwest::retry::for_host(endpoint_host)
            .max_retries_per_request(config.max_retries)
            .classify_fn(|req_rep| {
                match req_rep.status() {
                    Some(status) if status.is_server_error() => req_rep.retryable(),
                    _ => req_rep.success(),
                }
            });
        builder = builder.retry(retry_policy);

        // Add API key header if provided
        if let Some(ref api_key) = config.api_key {
            let key = api_key.clone();
            builder = builder.default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(
                    "Authorization",
                    reqwest::header::HeaderValue::from_str(&format!("Bearer {}", key))
                        .map_err(|e| format!("Invalid API key: {}", e))?,
                );
                headers
            });
        }

        let http_client = builder
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        Ok(Self {
            config,
            http_client,
        })
    }

    /// Check if LiteLLM is available
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/health", self.config.endpoint);
        match self.http_client.get(&url).send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    /// List available models
    pub async fn list_models(&self) -> Result<Vec<ModelInfo>, String> {
        let url = format!("{}/v1/models", self.config.endpoint);

        match self.http_client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let models: Vec<ModelInfo> = response
                        .json()
                        .await
                        .map_err(|e| format!("Failed to parse response: {}", e))?;
                    Ok(models)
                } else {
                    let error = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    Err(format!("LiteLLM list models failed: {}", error))
                }
            }
            Err(e) => Err(format!("LiteLLM request failed: {}", e)),
        }
    }

    /// Send a completion request — retries are handled automatically by reqwest
    pub async fn complete(&self, request: &ModelRequest) -> Result<ModelResponse, String> {
        let url = format!("{}/v1/chat/completions", self.config.endpoint);

        let response = self.http_client
            .post(&url)
            .json(request)
            .send()
            .await
            .map_err(|e| format!("LiteLLM request failed: {}", e))?;

        if response.status().is_success() {
            let result: ModelResponse = response
                .json()
                .await
                .map_err(|e| format!("Failed to parse response: {}", e))?;

            debug!(
                model = %request.model,
                tokens = result.usage.as_ref().map(|u| u.total_tokens).unwrap_or(0),
                "LiteLLM completion successful"
            );

            Ok(result)
        } else {
            let status = response.status().as_u16();
            let error = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            warn!(
                status = status,
                error = %error,
                "LiteLLM completion failed"
            );
            Err(format!("LiteLLM completion failed: {} {}", status, error))
        }
    }

    /// Select model based on routing decision
    pub fn select_model(&self, tier: &str, available_models: &[String]) -> String {
        // Simple tier-based model selection
        match tier {
            "fast" => {
                // Use the smallest/fastest model
                available_models
                    .iter()
                    .find(|m| m.contains("mini") || m.contains("small") || m.contains("haiku"))
                    .or_else(|| available_models.first())
                    .cloned()
                    .unwrap_or_else(|| "gpt-4o-mini".to_string())
            }
            "balanced" => {
                // Use a balanced model
                available_models
                    .iter()
                    .find(|m| m.contains("4o") && !m.contains("mini"))
                    .or_else(|| available_models.get(1))
                    .cloned()
                    .unwrap_or_else(|| "gpt-4o".to_string())
            }
            "capable" => {
                // Use the most capable model
                available_models
                    .iter()
                    .find(|m| m.contains("o1") || m.contains("claude") || m.contains("opus"))
                    .or_else(|| available_models.last())
                    .cloned()
                    .unwrap_or_else(|| "gpt-4o".to_string())
            }
            _ => {
                // Default to balanced
                available_models
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "gpt-4o".to_string())
            }
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_litellm_config_default() {
        let config = LiteLLMConfig::default();
        assert_eq!(config.endpoint, "http://localhost:4000");
        assert_eq!(config.timeout_ms, 30000);
        assert_eq!(config.max_retries, 2);
        assert!(config.api_key.is_none());
    }

    #[test]
    fn test_model_request_serialization() {
        let request = ModelRequest {
            model: "gpt-4o".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            max_tokens: Some(100),
            temperature: Some(0.7),
            stream: Some(false),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("gpt-4o"));
        assert!(json.contains("Hello"));
        assert!(json.contains("100"));
    }

    #[test]
    fn test_select_model_fast() {
        let config = LiteLLMConfig::default();
        let client = LiteLLMClient::new(config).unwrap();

        let models = vec![
            "gpt-4o".to_string(),
            "gpt-4o-mini".to_string(),
            "claude-3-opus".to_string(),
        ];

        let selected = client.select_model("fast", &models);
        assert!(selected.contains("mini"));
    }

    #[test]
    fn test_select_model_balanced() {
        let config = LiteLLMConfig::default();
        let client = LiteLLMClient::new(config).unwrap();

        let models = vec![
            "gpt-4o".to_string(),
            "gpt-4o-mini".to_string(),
            "claude-3-opus".to_string(),
        ];

        let selected = client.select_model("balanced", &models);
        assert!(selected.contains("4o"));
        assert!(!selected.contains("mini"));
    }

    #[test]
    fn test_select_model_capable() {
        let config = LiteLLMConfig::default();
        let client = LiteLLMClient::new(config).unwrap();

        let models = vec![
            "gpt-4o".to_string(),
            "gpt-4o-mini".to_string(),
            "claude-3-opus".to_string(),
        ];

        let selected = client.select_model("capable", &models);
        assert!(selected.contains("opus"));
    }

    #[test]
    fn test_select_model_default() {
        let config = LiteLLMConfig::default();
        let client = LiteLLMClient::new(config).unwrap();

        let models = vec![
            "gpt-4o".to_string(),
            "gpt-4o-mini".to_string(),
        ];

        let selected = client.select_model("unknown", &models);
        assert_eq!(selected, "gpt-4o");
    }
}
