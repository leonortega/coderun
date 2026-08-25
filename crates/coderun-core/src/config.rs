use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::error::{ConfigError, Result};

// ── Top-level Config ────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub daemon: DaemonConfig,
    pub database: DatabaseConfig,
    pub index: IndexConfig,
    pub knowledge: KnowledgeConfig,
    pub skills: SkillsConfig,
    pub context: ContextConfig,
    pub model: ModelConfig,
    pub routing: RoutingConfig,
    pub litellm: LiteLlmConfig,
    pub rtk: RtkConfig,
    pub workflow: WorkflowConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    pub socket_path: String,
    pub max_concurrent: usize,
    pub request_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub path: String,
    pub max_connections: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IndexConfig {
    pub path: String,
    pub languages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KnowledgeConfig {
    pub memory_enabled: bool,
    pub memory_endpoint: String,
    pub max_knowledge_entries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SkillsConfig {
    pub path: String,
    pub auto_discover: bool,
    pub max_skills_per_request: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ContextConfig {
    pub max_tokens: usize,
    pub max_files: usize,
    pub max_lines_per_file: usize,
    pub cache_order: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelConfig {
    pub default_tier: String,
    pub routing_enabled: bool,
    pub max_tokens_response: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RoutingConfig {
    pub structural_weight: f64,
    pub semantic_weight: f64,
    pub scope_weight: f64,
    pub fast_threshold: f64,
    pub capable_threshold: f64,
    pub fast_model: String,
    pub balanced_model: String,
    pub capable_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LiteLlmConfig {
    pub endpoint: String,
    pub timeout_ms: u64,
    pub max_retries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RtkConfig {
    pub enabled: bool,
    pub max_output_tokens: usize,
    pub compression_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkflowConfig {
    /// Enable DBOS durable workflows
    pub enabled: bool,
    /// Engine: "dbos" | "noop"
    pub engine: String,
    /// DBOS sidecar HTTP endpoint
    pub dbos_endpoint: String,
    /// Shared HMAC secret for DBOS→daemon signing
    pub dbos_shared_secret: Option<String>,
    /// Auto-governance for tier=capable tasks
    pub auto_governance: bool,
    /// Tiers that require approval when governance is on
    pub require_approval_tiers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,
    pub file_path: String,
    pub max_size_mb: usize,
    pub retention_days: usize,
}

// ── Defaults ────────────────────────────────────────────────────────────

// Default is derived since all fields implement Default

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: "/tmp/coderun.sock".to_string(),
            max_concurrent: 10,
            request_timeout_ms: 30000,
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: "~/.coderun/data.db".to_string(),
            max_connections: 5,
        }
    }
}

impl Default for IndexConfig {
    fn default() -> Self {
        // v0.6.0: default = 4 langs; go/java/c/cpp behind --features extended-languages (V0_6_0_PLAN.md:2.2)
        Self {
            path: "~/.coderun/index/".to_string(),
            languages: vec![
                "rust".to_string(),
                "typescript".to_string(),
                "javascript".to_string(),
                "python".to_string(),
            ],
        }
    }
}

impl Default for KnowledgeConfig {
    fn default() -> Self {
        Self {
            memory_enabled: true,
            memory_endpoint: "http://localhost:9090".to_string(),
            max_knowledge_entries: 10000,
        }
    }
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            path: ".coderun/skills/".to_string(),
            auto_discover: true,
            max_skills_per_request: 5,
        }
    }
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

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            default_tier: "balanced".to_string(),
            routing_enabled: true,
            max_tokens_response: 4096,
        }
    }
}

impl Default for RoutingConfig {
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

impl Default for LiteLlmConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:4000".to_string(),
            timeout_ms: 30000,
            max_retries: 3,
        }
    }
}

impl Default for RtkConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_output_tokens: 8000,
            compression_level: "balanced".to_string(),
        }
    }
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        // v0.6.0: DBOS required, enabled true default (SQLite + Litestream)
        Self {
            enabled: true,
            engine: "dbos".to_string(),
            dbos_endpoint: "http://localhost:3001".to_string(),
            dbos_shared_secret: None,
            auto_governance: false,
            require_approval_tiers: vec!["capable".to_string()],
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            file_path: "~/.coderun/logs/coderun.log".to_string(),
            max_size_mb: 100,
            retention_days: 7,
        }
    }
}

// ── Config Loading ──────────────────────────────────────────────────────

impl Config {
    /// Load configuration by merging user, project, and environment configs.
    ///
    /// Priority (highest wins): environment > project > user > defaults
    pub fn load(project_root: &Path) -> Result<Self> {
        let mut config = Self::default();

        // 1. Load user config (~/.config/coderun/config.toml)
        let user_config_path = Self::user_config_path();
        if user_config_path.exists() {
            info!(path = %user_config_path.display(), "Loading user config");
            let user_config = Self::from_file(&user_config_path)?;
            config.merge(user_config);
        }

        // 2. Load project config (.coderun/config.toml)
        let project_config_path = project_root.join(".coderun/config.toml");
        if project_config_path.exists() {
            info!(path = %project_config_path.display(), "Loading project config");
            let project_config = Self::from_file(&project_config_path)?;
            config.merge(project_config);
        }

        // 3. Apply environment variable overrides
        config.apply_env_overrides();

        Ok(config)
    }

    /// Load config from a TOML file
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            ConfigError::FileReadError {
                path: path.display().to_string(),
                source: e,
            }
        })?;
        Self::from_toml(&content)
    }

    /// Load config from a TOML string
    pub fn from_toml(content: &str) -> Result<Self> {
        toml::from_str(content).map_err(|e| ConfigError::ParseError(e.to_string()).into())
    }

    /// Get the user config path (~/.config/coderun/config.toml)
    fn user_config_path() -> PathBuf {
        if let Some(home) = dirs() {
            home.join(".config").join("coderun").join("config.toml")
        } else {
            PathBuf::from("~/.config/coderun/config.toml")
        }
    }

    /// Merge another config into this one (non-default values override)
    pub fn merge(&mut self, other: Config) {
        self.daemon = other.daemon;
        self.database = other.database;
        self.index = other.index;
        self.knowledge = other.knowledge;
        self.skills = other.skills;
        self.context = other.context;
        self.model = other.model;
        self.routing = other.routing;
        self.litellm = other.litellm;
        self.rtk = other.rtk;
        self.workflow = other.workflow;
        self.logging = other.logging;
    }

    /// Apply environment variable overrides (CODERUN_*)
    pub fn apply_env_overrides(&mut self) {
        if let Ok(val) = std::env::var("CODERUN_DAEMON_SOCKET") {
            self.daemon.socket_path = val;
        }
        if let Ok(val) = std::env::var("CODERUN_DATABASE_PATH") {
            self.database.path = val;
        }
        if let Ok(val) = std::env::var("CODERUN_LOG_LEVEL") {
            self.logging.level = val;
        }
        if let Ok(val) = std::env::var("CODERUN_MODEL_DEFAULT") {
            self.model.default_tier = val;
        }
        if let Ok(val) = std::env::var("CODERUN_CONTEXT_MAX_TOKENS") {
            if let Ok(n) = val.parse() {
                self.context.max_tokens = n;
            }
        }
        if let Ok(val) = std::env::var("CODERUN_LITELLM_URL") {
            self.litellm.endpoint = val;
        }
        if let Ok(val) = std::env::var("CODERUN_ENGRAM_ENDPOINT") {
            self.knowledge.memory_endpoint = val;
        }
        if let Ok(val) = std::env::var("CODERUN_DBOS_ENDPOINT") {
            self.workflow.dbos_endpoint = val;
        }
        if let Ok(val) = std::env::var("CODERUN_DBOS_SECRET") {
            self.workflow.dbos_shared_secret = Some(val);
        }
        if let Ok(val) = std::env::var("CODERUN_WORKFLOW_ENABLED") {
            self.workflow.enabled = val == "true" || val == "1";
        }
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        // Daemon
        if self.daemon.max_concurrent == 0 {
            return Err(ConfigError::InvalidValue {
                field: "daemon.max_concurrent".to_string(),
                message: "must be greater than 0".to_string(),
            }
            .into());
        }
        if self.daemon.request_timeout_ms == 0 {
            return Err(ConfigError::InvalidValue {
                field: "daemon.request_timeout_ms".to_string(),
                message: "must be greater than 0".to_string(),
            }
            .into());
        }

        // Database
        if self.database.max_connections == 0 {
            return Err(ConfigError::InvalidValue {
                field: "database.max_connections".to_string(),
                message: "must be greater than 0".to_string(),
            }
            .into());
        }

        // Context
        if self.context.max_tokens == 0 {
            return Err(ConfigError::InvalidValue {
                field: "context.max_tokens".to_string(),
                message: "must be greater than 0".to_string(),
            }
            .into());
        }
        if self.context.max_files == 0 {
            return Err(ConfigError::InvalidValue {
                field: "context.max_files".to_string(),
                message: "must be greater than 0".to_string(),
            }
            .into());
        }
        if self.context.max_lines_per_file == 0 {
            return Err(ConfigError::InvalidValue {
                field: "context.max_lines_per_file".to_string(),
                message: "must be greater than 0".to_string(),
            }
            .into());
        }

        // Model
        let valid_tiers = ["fast", "balanced", "capable"];
        if !valid_tiers.contains(&self.model.default_tier.as_str()) {
            return Err(ConfigError::InvalidValue {
                field: "model.default_tier".to_string(),
                message: format!(
                    "must be one of: {}",
                    valid_tiers.join(", ")
                ),
            }
            .into());
        }

        // Routing weights must sum to ~1.0
        let weight_sum = self.routing.structural_weight
            + self.routing.semantic_weight
            + self.routing.scope_weight;
        if (weight_sum - 1.0).abs() > 0.01 {
            return Err(ConfigError::InvalidValue {
                field: "routing.*_weight".to_string(),
                message: format!(
                    "routing weights must sum to 1.0, got {}",
                    weight_sum
                ),
            }
            .into());
        }

        // Thresholds
        if self.routing.fast_threshold >= self.routing.capable_threshold {
            return Err(ConfigError::InvalidValue {
                field: "routing.*_threshold".to_string(),
                message: "fast_threshold must be less than capable_threshold".to_string(),
            }
            .into());
        }

        // RTK
        let valid_compression = ["light", "balanced", "aggressive"];
        if !valid_compression.contains(&self.rtk.compression_level.as_str()) {
            return Err(ConfigError::InvalidValue {
                field: "rtk.compression_level".to_string(),
                message: format!(
                    "must be one of: {}",
                    valid_compression.join(", ")
                ),
            }
            .into());
        }

        // Logging
        let valid_levels = ["error", "warn", "info", "debug", "trace"];
        if !valid_levels.contains(&self.logging.level.as_str()) {
            return Err(ConfigError::InvalidValue {
                field: "logging.level".to_string(),
                message: format!(
                    "must be one of: {}",
                    valid_levels.join(", ")
                ),
            }
            .into());
        }

        // Workflow — v0.6.0 required
        let valid_engines = ["noop", "dbos"];
        if !valid_engines.contains(&self.workflow.engine.as_str()) {
            return Err(ConfigError::InvalidValue {
                field: "workflow.engine".to_string(),
                message: format!("must be one of: {}", valid_engines.join(", ")),
            }
            .into());
        }
        // Warn if enabled but secret missing (HMAC required) — not fatal for dev; token only needed for cloud
        if self.workflow.enabled && self.workflow.engine == "dbos" && self.workflow.dbos_shared_secret.is_none() {
            tracing::warn!("workflow.enabled=true but dbos_shared_secret missing — local default 'your-secret' is fine; set CODERUN_DBOS_SECRET / DBOS_CONDUCTOR_KEY only when connecting to DBOS Cloud/Conductor");
        }
        // Extended languages warning
        let extended = ["go", "java", "c", "cpp"];
        for lang in &self.index.languages {
            if extended.contains(&lang.as_str()) {
                tracing::warn!(language = %lang, "language requires --features extended-languages; fallback regex will be used");
            }
        }

        Ok(())
    }

    /// Show the effective configuration as TOML string
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| ConfigError::SerializeError(e.to_string()).into())
    }
}

/// Get the user's home directory
fn dirs() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_from_str() {
        let toml = r#"
[daemon]
socket_path = "/custom/sock"
max_concurrent = 5
request_timeout_ms = 10000

[database]
path = "/tmp/test.db"
max_connections = 2

[index]
path = "/tmp/index"
languages = ["rust", "python"]

[knowledge]
memory_enabled = false
memory_endpoint = "http://localhost:8080"
max_knowledge_entries = 5000

[skills]
path = "/skills/"
auto_discover = false
max_skills_per_request = 3

[context]
max_tokens = 8000
max_files = 10
max_lines_per_file = 200
cache_order = ["code_context"]

[model]
default_tier = "fast"
routing_enabled = false
max_tokens_response = 2048

[routing]
structural_weight = 0.5
semantic_weight = 0.3
scope_weight = 0.2
fast_threshold = 0.2
capable_threshold = 0.8
fast_model = "gpt-4o-mini"
balanced_model = "gpt-4o"
capable_model = "o1"

[litellm]
endpoint = "http://custom:4000"
timeout_ms = 10000
max_retries = 1

[rtk]
enabled = false
max_output_tokens = 4000
compression_level = "light"

[logging]
level = "debug"
file_path = "/tmp/test.log"
max_size_mb = 50
retention_days = 3
"#;
        let config = Config::from_toml(toml).unwrap();
        assert_eq!(config.daemon.socket_path, "/custom/sock");
        assert_eq!(config.daemon.max_concurrent, 5);
        assert_eq!(config.database.path, "/tmp/test.db");
        assert_eq!(config.context.max_tokens, 8000);
        assert_eq!(config.model.default_tier, "fast");
        assert!(!config.model.routing_enabled);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_merge() {
        let mut base = Config::default();
        let mut override_config = Config::default();
        override_config.daemon.socket_path = "/new/sock".to_string();
        override_config.context.max_tokens = 5000;

        base.merge(override_config);
        assert_eq!(base.daemon.socket_path, "/new/sock");
        assert_eq!(base.context.max_tokens, 5000);
    }

    #[test]
    fn test_validation_fails_zero_concurrent() {
        let mut config = Config::default();
        config.daemon.max_concurrent = 0;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("daemon.max_concurrent"));
    }

    #[test]
    fn test_validation_fails_invalid_tier() {
        let mut config = Config::default();
        config.model.default_tier = "ultra".to_string();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("model.default_tier"));
    }

    #[test]
    fn test_validation_fails_weight_sum() {
        let mut config = Config::default();
        config.routing.structural_weight = 0.5;
        config.routing.semantic_weight = 0.5;
        config.routing.scope_weight = 0.5; // sum = 1.5
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("routing.*_weight"));
    }

    #[test]
    fn test_validation_fails_threshold_order() {
        let mut config = Config::default();
        config.routing.fast_threshold = 0.8;
        config.routing.capable_threshold = 0.3;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("fast_threshold"));
    }

    #[test]
    fn test_validation_fails_invalid_compression() {
        let mut config = Config::default();
        config.rtk.compression_level = "extreme".to_string();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("rtk.compression_level"));
    }

    #[test]
    fn test_validation_fails_invalid_log_level() {
        let mut config = Config::default();
        config.logging.level = "verbose".to_string();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("logging.level"));
    }

    #[test]
    fn test_config_roundtrip() {
        let config = Config::default();
        let toml = config.to_toml().unwrap();
        let loaded = Config::from_toml(&toml).unwrap();
        assert_eq!(config.daemon.socket_path, loaded.daemon.socket_path);
        assert_eq!(config.context.max_tokens, loaded.context.max_tokens);
        assert_eq!(config.model.default_tier, loaded.model.default_tier);
    }

    #[test]
    fn test_partial_toml_uses_defaults() {
        let toml = r#"
[daemon]
socket_path = "/test/sock"
"#;
        let config = Config::from_toml(toml).unwrap();
        assert_eq!(config.daemon.socket_path, "/test/sock");
        // Other fields use defaults
        assert_eq!(config.daemon.max_concurrent, 10);
        assert_eq!(config.database.path, "~/.coderun/data.db");
        assert_eq!(config.context.max_tokens, 12000);
    }

    #[test]
    fn test_env_overrides() {
        // Save and restore env
        let original = std::env::var("CODERUN_DAEMON_SOCKET").ok();
        let original_log = std::env::var("CODERUN_LOG_LEVEL").ok();

        std::env::set_var("CODERUN_DAEMON_SOCKET", "/env/sock");
        std::env::set_var("CODERUN_LOG_LEVEL", "trace");

        let mut config = Config::default();
        config.apply_env_overrides();

        assert_eq!(config.daemon.socket_path, "/env/sock");
        assert_eq!(config.logging.level, "trace");

        // Restore
        match original {
            Some(v) => std::env::set_var("CODERUN_DAEMON_SOCKET", v),
            None => std::env::remove_var("CODERUN_DAEMON_SOCKET"),
        }
        match original_log {
            Some(v) => std::env::set_var("CODERUN_LOG_LEVEL", v),
            None => std::env::remove_var("CODERUN_LOG_LEVEL"),
        }
    }

    #[test]
    fn test_v060_defaults_workflow_required() {
        let config = Config::default();
        // DBOS required since v0.6.0
        assert!(config.workflow.enabled, "workflow.enabled should be true since v0.6.0");
        assert_eq!(config.workflow.engine, "dbos");
        assert_eq!(config.workflow.dbos_endpoint, "http://localhost:3001");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_v060_defaults_index_four_languages() {
        let config = Config::default();
        assert_eq!(config.index.languages.len(), 4);
        assert_eq!(config.index.languages, vec!["rust", "typescript", "javascript", "python"]);
        // Extended languages trigger warning but not error
        let mut with_extended = Config::default();
        with_extended.index.languages.push("go".to_string());
        with_extended.index.languages.push("java".to_string());
        assert!(with_extended.validate().is_ok(), "extended languages should warn, not fail");
    }

    #[test]
    fn test_v060_toml_roundtrip_workflow() {
        let toml = r#"
[workflow]
enabled = true
engine = "dbos"
dbos_endpoint = "http://localhost:3001"
auto_governance = true
require_approval_tiers = ["capable", "balanced"]

[index]
path = "/tmp/index"
languages = ["rust", "python", "go"]
"#;
        let config = Config::from_toml(toml).unwrap();
        assert!(config.workflow.enabled);
        assert_eq!(config.workflow.engine, "dbos");
        assert!(config.workflow.auto_governance);
        assert_eq!(config.index.languages.len(), 3);
        assert!(config.validate().is_ok());
        // Round-trip preserves
        let serialized = config.to_toml().unwrap();
        let reloaded = Config::from_toml(&serialized).unwrap();
        assert_eq!(reloaded.workflow.engine, "dbos");
        assert_eq!(reloaded.index.languages, config.index.languages);
    }

    #[test]
    fn test_v060_validation_invalid_workflow_engine() {
        let mut config = Config::default();
        config.workflow.engine = "temporal".to_string();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("workflow.engine"));
    }

    #[test]
    fn test_v060_env_overrides_dbos() {
        let orig_endpoint = std::env::var("CODERUN_DBOS_ENDPOINT").ok();
        let orig_secret = std::env::var("CODERUN_DBOS_SECRET").ok();
        let orig_enabled = std::env::var("CODERUN_WORKFLOW_ENABLED").ok();

        std::env::set_var("CODERUN_DBOS_ENDPOINT", "http://example:9999");
        std::env::set_var("CODERUN_DBOS_SECRET", "test-secret-xyz");
        std::env::set_var("CODERUN_WORKFLOW_ENABLED", "false");

        let mut config = Config::default();
        config.apply_env_overrides();
        assert_eq!(config.workflow.dbos_endpoint, "http://example:9999");
        assert_eq!(config.workflow.dbos_shared_secret, Some("test-secret-xyz".to_string()));
        assert!(!config.workflow.enabled);

        // Restore
        match orig_endpoint { Some(v) => std::env::set_var("CODERUN_DBOS_ENDPOINT", v), None => std::env::remove_var("CODERUN_DBOS_ENDPOINT") }
        match orig_secret { Some(v) => std::env::set_var("CODERUN_DBOS_SECRET", v), None => std::env::remove_var("CODERUN_DBOS_SECRET") }
        match orig_enabled { Some(v) => std::env::set_var("CODERUN_WORKFLOW_ENABLED", v), None => std::env::remove_var("CODERUN_WORKFLOW_ENABLED") }
    }

    #[test]
    fn test_v060_languages_default_docs_sync() {
        // Ensures docs still claim 4 default; config must match docs/01-architecture/RUNTIME.md
        let cfg = Config::default();
        assert!(!cfg.index.languages.contains(&"cpp".to_string()));
        assert!(!cfg.index.languages.contains(&"go".to_string()));
    }
}
