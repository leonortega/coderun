use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

// ── Configuration ────────────────────────────────────────────────────────

/// Engram client configuration
#[derive(Debug, Clone)]
pub struct EngramConfig {
    /// Engram server endpoint (e.g., "http://localhost:9090")
    pub endpoint: String,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Maximum number of retries
    pub max_retries: u32,
}

impl Default for EngramConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:9090".to_string(),
            timeout_ms: 5000,
            max_retries: 2,
        }
    }
}

// ── Data Types ───────────────────────────────────────────────────────────

/// Memory entry for engram
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub namespace: String,
    pub key: String,
    pub value: String,
    pub metadata: Option<serde_json::Value>,
}

/// Memory search query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    pub namespace: String,
    pub query: String,
    pub max_results: Option<usize>,
}

/// Memory search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryResult {
    pub entries: Vec<MemoryEntry>,
    pub total_count: usize,
}

/// Engram API response
#[derive(Debug, Deserialize)]
struct EngramResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

// ── Engram Client ────────────────────────────────────────────────────────

/// HTTP client for engram memory storage
pub struct EngramClient {
    config: EngramConfig,
    http_client: reqwest::Client,
}

impl EngramClient {
    /// Create a new engram client
    pub fn new(config: EngramConfig) -> Result<Self, String> {
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
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .retry(retry_policy)
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        Ok(Self {
            config,
            http_client,
        })
    }

    /// Check if engram is available
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/health", self.config.endpoint);
        match self.http_client.get(&url).send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    /// Save a memory entry — retries are handled automatically by reqwest
    pub async fn save_memory(&self, entry: &MemoryEntry) -> Result<(), String> {
        let url = format!("{}/api/memory/save", self.config.endpoint);

        let response = self.http_client
            .post(&url)
            .json(entry)
            .send()
            .await
            .map_err(|e| format!("Engram save request failed: {}", e))?;

        if response.status().is_success() {
            debug!(
                namespace = %entry.namespace,
                key = %entry.key,
                "Memory saved to engram"
            );
            Ok(())
        } else {
            let status = response.status().as_u16();
            let error = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            warn!(
                status = status,
                error = %error,
                "Engram save failed"
            );
            Err(format!("Engram save failed: {} {}", status, error))
        }
    }

    /// Search memory entries
    pub async fn search_memory(&self, query: &MemoryQuery) -> Result<MemoryResult, String> {
        let url = format!("{}/api/memory/search", self.config.endpoint);

        match self.http_client.post(&url).json(query).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let result: EngramResponse<MemoryResult> = response
                        .json()
                        .await
                        .map_err(|e| format!("Failed to parse response: {}", e))?;

                    if result.success {
                        Ok(result.data.unwrap_or(MemoryResult {
                            entries: Vec::new(),
                            total_count: 0,
                        }))
                    } else {
                        Err(result.error.unwrap_or_else(|| "Unknown error".to_string()))
                    }
                } else {
                    let error = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    Err(format!("Engram search failed: {}", error))
                }
            }
            Err(e) => Err(format!("Engram search request failed: {}", e)),
        }
    }

    /// Get a specific memory entry
    pub async fn get_memory(&self, namespace: &str, key: &str) -> Result<Option<MemoryEntry>, String> {
        let url = format!("{}/api/memory/get", self.config.endpoint);
        let query = serde_json::json!({
            "namespace": namespace,
            "key": key,
        });

        match self.http_client.post(&url).json(&query).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let result: EngramResponse<MemoryEntry> = response
                        .json()
                        .await
                        .map_err(|e| format!("Failed to parse response: {}", e))?;

                    if result.success {
                        Ok(result.data)
                    } else {
                        Err(result.error.unwrap_or_else(|| "Unknown error".to_string()))
                    }
                } else {
                    let error = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    Err(format!("Engram get failed: {}", error))
                }
            }
            Err(e) => Err(format!("Engram get request failed: {}", e)),
        }
    }

    /// Delete a memory entry
    pub async fn delete_memory(&self, namespace: &str, key: &str) -> Result<(), String> {
        let url = format!("{}/api/memory/delete", self.config.endpoint);
        let query = serde_json::json!({
            "namespace": namespace,
            "key": key,
        });

        match self.http_client.post(&url).json(&query).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    debug!(
                        namespace = namespace,
                        key = key,
                        "Memory deleted from engram"
                    );
                    Ok(())
                } else {
                    let error = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    Err(format!("Engram delete failed: {}", error))
                }
            }
            Err(e) => Err(format!("Engram delete request failed: {}", e)),
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engram_config_default() {
        let config = EngramConfig::default();
        assert_eq!(config.endpoint, "http://localhost:9090");
        assert_eq!(config.timeout_ms, 5000);
        assert_eq!(config.max_retries, 2);
    }

    #[test]
    fn test_memory_entry_serialization() {
        let entry = MemoryEntry {
            namespace: "test".to_string(),
            key: "foo".to_string(),
            value: "bar".to_string(),
            metadata: None,
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("test"));
        assert!(json.contains("foo"));
        assert!(json.contains("bar"));
    }

    #[test]
    fn test_memory_query_serialization() {
        let query = MemoryQuery {
            namespace: "test".to_string(),
            query: "foo".to_string(),
            max_results: Some(10),
        };

        let json = serde_json::to_string(&query).unwrap();
        assert!(json.contains("test"));
        assert!(json.contains("foo"));
        assert!(json.contains("10"));
    }
}
