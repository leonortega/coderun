use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

// ── Configuration ────────────────────────────────────────────────────────

/// Engram client configuration
#[derive(Debug, Clone)]
pub struct EngramConfig {
    /// Path to engram binary (e.g., "~/.coderun/bin/engram")
    pub binary_path: String,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Maximum number of retries (for CLI calls)
    pub max_retries: u32,
}

impl Default for EngramConfig {
    fn default() -> Self {
        // Try to find engram binary in common locations
        let binary_path = discover_engram_exe();
        Self {
            binary_path,
            timeout_ms: 5000,
            max_retries: 1,
        }
    }
}

/// Discover the engram executable (binary preferred, npx fallback).
fn discover_engram_exe() -> String {
    // 1. ~/.coderun/bin — installed by our installer (preferred)
    #[cfg(target_os = "windows")]
    if let Ok(home) = std::env::var("USERPROFILE") {
        let exe = std::path::PathBuf::from(&home)
            .join(".coderun")
            .join("bin")
            .join("engram.exe");
        if exe.exists() {
            return exe.to_string_lossy().into_owned();
        }
    }
    #[cfg(not(target_os = "windows"))]
    if let Ok(home) = std::env::var("HOME") {
        let exe = std::path::PathBuf::from(&home)
            .join(".coderun")
            .join("bin")
            .join("engram");
        if exe.exists() {
            return exe.to_string_lossy().into_owned();
        }
    }
    // 2. User bin directory
    #[cfg(target_os = "windows")]
    if let Ok(home) = std::env::var("USERPROFILE") {
        let exe = std::path::PathBuf::from(&home)
            .join("bin")
            .join("engram.exe");
        if exe.exists() {
            return exe.to_string_lossy().into_owned();
        }
    }
    #[cfg(not(target_os = "windows"))]
    if let Ok(home) = std::env::var("HOME") {
        let exe = std::path::PathBuf::from(&home)
            .join("bin")
            .join("engram");
        if exe.exists() {
            return exe.to_string_lossy().into_owned();
        }
    }
    // 3. System PATH
    "engram".to_string()
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

/// Engram CLI search result (from JSON output)
#[derive(Debug, Deserialize)]
struct EngramCliSearchResult {
    #[serde(default)]
    memories: Vec<EngramCliMemory>,
    #[serde(default)]
    total: usize,
}

/// Engram CLI memory entry
#[derive(Debug, Deserialize)]
struct EngramCliMemory {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    #[serde(rename = "type")]
    memory_type: String,
    #[serde(default)]
    project: String,
}

// ── Engram Client ────────────────────────────────────────────────────────

/// CLI-based client for engram memory storage
pub struct EngramClient {
    config: EngramConfig,
}

impl EngramClient {
    /// Create a new engram client
    pub fn new(config: EngramConfig) -> Result<Self, String> {
        // Verify binary exists (best-effort)
        let binary = std::path::Path::new(&config.binary_path);
        if !binary.exists() && config.binary_path != "engram" {
            tracing::debug!(binary = %config.binary_path, "engram binary not found, CLI calls may fail");
        }
        Ok(Self { config })
    }

    /// Get the binary path
    pub fn binary_path(&self) -> &str {
        &self.config.binary_path
    }

    /// Check if engram is available
    pub async fn health_check(&self) -> bool {
        let output = std::process::Command::new(&self.config.binary_path)
            .arg("version")
            .output();
        match output {
            Ok(out) => out.status.success(),
            Err(_) => false,
        }
    }

    /// Save a memory entry via CLI
    pub async fn save_memory(&self, entry: &MemoryEntry) -> Result<(), String> {
        let output = std::process::Command::new(&self.config.binary_path)
            .args(["save", &entry.key, &entry.value])
            .args(["--type", "observation"])
            .args(["--project", &entry.namespace])
            .output()
            .map_err(|e| format!("Failed to run engram save: {}", e))?;

        if output.status.success() {
            debug!(
                namespace = %entry.namespace,
                key = %entry.key,
                "Memory saved to engram via CLI"
            );
            Ok(())
        } else {
            let error = String::from_utf8_lossy(&output.stderr).to_string();
            warn!(
                error = %error,
                "Engram CLI save failed"
            );
            Err(format!("Engram CLI save failed: {}", error))
        }
    }

    /// Search memory entries via CLI
    pub async fn search_memory(&self, query: &MemoryQuery) -> Result<MemoryResult, String> {
        let mut args = vec!["search".to_string(), query.query.clone()];
        if let Some(max) = query.max_results {
            args.push("--limit".to_string());
            args.push(max.to_string());
        }
        args.push("--json".to_string());

        let output = std::process::Command::new(&self.config.binary_path)
            .args(&args)
            .output()
            .map_err(|e| format!("Failed to run engram search: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(format!("Engram CLI search failed: {}", error));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let result: EngramCliSearchResult = serde_json::from_str(&stdout)
            .map_err(|e| format!("Failed to parse engram CLI output: {}", e))?;

        let entries = result.memories.into_iter().map(|m| MemoryEntry {
            namespace: m.project,
            key: m.title,
            value: m.content,
            metadata: Some(serde_json::json!({
                "id": m.id,
                "type": m.memory_type,
            })),
        }).collect();

        Ok(MemoryResult {
            entries,
            total_count: result.total,
        })
    }

    /// Get a specific memory entry via CLI
    pub async fn get_memory(&self, namespace: &str, key: &str) -> Result<Option<MemoryEntry>, String> {
        // Use search with specific query to find the entry
        let query = MemoryQuery {
            namespace: namespace.to_string(),
            query: key.to_string(),
            max_results: Some(1),
        };
        let result = self.search_memory(&query).await?;
        Ok(result.entries.into_iter().next())
    }

    /// Delete a memory entry via CLI
    pub async fn delete_memory(&self, namespace: &str, key: &str) -> Result<(), String> {
        // First search to find the ID
        let query = MemoryQuery {
            namespace: namespace.to_string(),
            query: key.to_string(),
            max_results: Some(1),
        };
        let result = self.search_memory(&query).await?;

        if let Some(entry) = result.entries.first() {
            if let Some(id) = entry.metadata.as_ref().and_then(|m| m.get("id")).and_then(|id| id.as_str()) {
                let output = std::process::Command::new(&self.config.binary_path)
                    .args(["delete", id])
                    .output()
                    .map_err(|e| format!("Failed to run engram delete: {}", e))?;

                if output.status.success() {
                    debug!(
                        namespace = namespace,
                        key = key,
                        "Memory deleted from engram via CLI"
                    );
                    Ok(())
                } else {
                    let error = String::from_utf8_lossy(&output.stderr).to_string();
                    Err(format!("Engram CLI delete failed: {}", error))
                }
            } else {
                Err("Memory entry has no ID".to_string())
            }
        } else {
            Err(format!("Memory entry not found: {}", key))
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
        // Binary path should be discovered or default to "engram"
        assert!(!config.binary_path.is_empty());
        assert_eq!(config.timeout_ms, 5000);
        assert_eq!(config.max_retries, 1);
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
