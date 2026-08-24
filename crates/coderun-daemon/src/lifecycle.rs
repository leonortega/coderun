use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tracing::{error, info, warn};

use crate::adapter::{AdapterConfig, AdapterLayer};
use coderun_core::Config;
use coderun_events::EventBus;
use coderun_knowledge::{KnowledgeConfig, KnowledgeHub};
use coderun_repo_intel::RepositoryIntelligence;
use coderun_storage::Database;

// ── Daemon State ────────────────────────────────────────────────────────

#[allow(clippy::arc_with_non_send_sync)]
pub struct DaemonState {
    pub config: Config,
    #[allow(dead_code)]
    pub db: Arc<Database>,
    pub event_bus: EventBus,
    pub shutdown_flag: Arc<AtomicBool>,
    pub force_shutdown_flag: Arc<AtomicBool>,
}

impl DaemonState {
    /// Initialize daemon state from config
    pub fn initialize(config: Config) -> Result<Self, String> {
        // Initialize logging
        initialize_logging(&config.logging.level);

        info!("Initializing daemon...");

        // Open database
        let db_path = expand_path(&config.database.path);
        let db_dir = db_path.parent().unwrap_or(&db_path);
        std::fs::create_dir_all(db_dir)
            .map_err(|e| format!("Failed to create database directory: {}", e))?;

        let db = Database::open(&db_path)
            .map_err(|e| format!("Failed to open database: {}", e))?;

        info!(path = %db_path.display(), "Database opened");

        // Initialize event bus
        let event_bus = EventBus::new();

        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let force_shutdown_flag = Arc::new(AtomicBool::new(false));

        Ok(Self {
            config,
            #[allow(clippy::arc_with_non_send_sync)]
            db: Arc::new(db),
            event_bus,
            shutdown_flag,
            force_shutdown_flag,
        })
    }

    /// Start the daemon
    pub async fn serve(&self) -> Result<(), String> {
        info!("Starting coderun daemon...");

        // Print startup banner
        print_banner(&self.config);

        // Initialize repository intelligence
        let repo_path = std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?;

        // Create a second Database connection for repo_intel (SQLite allows multiple readers)
        let repo_db_path = expand_path(&self.config.database.path);
        let repo_db = Database::open(&repo_db_path)
            .map_err(|e| format!("Failed to open database for repo-intel: {}", e))?;

        let repo_intel = RepositoryIntelligence::new(
            repo_path.clone(),
            repo_db,
            self.event_bus.clone(),
        );

        // Initialize knowledge hub
        let knowledge_db_path = expand_path(&self.config.database.path);
        let knowledge_db = Database::open(&knowledge_db_path)
            .map_err(|e| format!("Failed to open database for knowledge: {}", e))?;

        let knowledge_config = KnowledgeConfig {
            memory_enabled: self.config.knowledge.memory_enabled,
            memory_endpoint: self.config.knowledge.memory_endpoint.clone(),
            max_knowledge_entries: self.config.knowledge.max_knowledge_entries,
        };
        let knowledge_hub = KnowledgeHub::new(
            knowledge_db,
            self.event_bus.clone(),
            knowledge_config,
        );

        // Initialize context engine
        let context_config = coderun_context::ContextConfig {
            max_tokens: self.config.context.max_tokens,
            max_files: self.config.context.max_files,
            max_lines_per_file: self.config.context.max_lines_per_file,
            cache_order: self.config.context.cache_order.clone(),
        };
        let context_engine = coderun_context::ContextEngine::new(
            repo_intel,
            knowledge_hub,
            self.event_bus.clone(),
            context_config,
        );

        // Start background indexing
        let indexing_db_path = expand_path(&self.config.database.path);
        let event_bus_clone = self.event_bus.clone();
        let repo_path_clone = repo_path.clone();
        let indexing_handle = tokio::spawn(async move {
            info!("Starting background indexing...");
            let indexing_db = match Database::open(&indexing_db_path) {
                Ok(db) => db,
                Err(e) => {
                    error!(error = %e, "Failed to open database for indexing");
                    return;
                }
            };
            let mut repo_intel = RepositoryIntelligence::new(
                repo_path_clone,
                indexing_db,
                event_bus_clone,
            );
            match repo_intel.index_repository() {
                Ok(stats) => {
                    info!(
                        files = stats.files_indexed,
                        symbols = stats.symbols_extracted,
                        duration_ms = stats.duration_ms,
                        "Background indexing complete"
                    );
                }
                Err(e) => {
                    error!(error = %e, "Background indexing failed");
                }
            }
        });

        // Configure adapter
        let adapter_config = AdapterConfig {
            socket_path: PathBuf::from(&self.config.daemon.socket_path),
            request_timeout_ms: self.config.daemon.request_timeout_ms,
            max_concurrent: self.config.daemon.max_concurrent,
            tcp_port: 9527,
        };

        // Create adapter layer
        let adapter = AdapterLayer::new(
            adapter_config,
            context_engine,
            self.event_bus.clone(),
        );

        // Start adapter server
        let adapter_handle = tokio::spawn(async move {
            adapter.serve().await
        });

        // Wait for shutdown signal
        info!("Daemon ready. Press Ctrl+C to shutdown.");
        wait_for_shutdown(self.shutdown_flag.clone(), self.force_shutdown_flag.clone()).await;

        // Graceful shutdown
        info!("Shutting down gracefully...");

        // Wait for adapter to finish (max 30s)
        match tokio::time::timeout(Duration::from_secs(30), adapter_handle).await {
            Ok(Ok(Ok(()))) => info!("Adapter stopped"),
            Ok(Ok(Err(e))) => warn!(error = %e, "Adapter stopped with error"),
            Ok(Err(e)) => warn!(error = %e, "Adapter task failed"),
            Err(_) => warn!("Adapter shutdown timed out"),
        }

        // Wait for indexing to finish
        let _ = tokio::time::timeout(Duration::from_secs(10), indexing_handle).await;

        // Cleanup
        cleanup(&self.config.daemon.socket_path);

        info!("Daemon shutdown complete");
        Ok(())
    }
}

// ── Signal Handling ─────────────────────────────────────────────────────

async fn wait_for_shutdown(shutdown_flag: Arc<AtomicBool>, force_flag: Arc<AtomicBool>) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigint = signal(SignalKind::interrupt()).expect("Failed to register SIGINT handler");
        let mut sigterm = signal(SignalKind::terminate()).expect("Failed to register SIGTERM handler");
        let mut sighup = signal(SignalKind::hangup()).expect("Failed to register SIGHUP handler");

        loop {
            tokio::select! {
                _ = sigint.recv() => {
                    if shutdown_flag.load(Ordering::Relaxed) {
                        warn!("Second signal received, forcing shutdown");
                        force_flag.store(true, Ordering::Relaxed);
                        break;
                    }
                    info!("SIGINT received, initiating graceful shutdown");
                    shutdown_flag.store(true, Ordering::Relaxed);
                }
                _ = sigterm.recv() => {
                    if shutdown_flag.load(Ordering::Relaxed) {
                        warn!("Second signal received, forcing shutdown");
                        force_flag.store(true, Ordering::Relaxed);
                        break;
                    }
                    info!("SIGTERM received, initiating graceful shutdown");
                    shutdown_flag.store(true, Ordering::Relaxed);
                }
                _ = sighup.recv() => {
                    info!("SIGHUP received, reloading configuration");
                    // TODO: Implement config reload
                }
            }
        }
    }

    #[cfg(not(unix))]
    {
        // Windows: use tokio's ctrl_c
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                if shutdown_flag.load(Ordering::Relaxed) {
                    warn!("Second signal received, forcing shutdown");
                    force_flag.store(true, Ordering::Relaxed);
                } else {
                    info!("Ctrl+C received, initiating graceful shutdown");
                    shutdown_flag.store(true, Ordering::Relaxed);
                }
            }
            Err(e) => error!(error = %e, "Failed to listen for shutdown signal"),
        }
    }
}

// ── Helper Functions ────────────────────────────────────────────────────

fn initialize_logging(level: &str) {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(true)
        .init();
}

fn expand_path(path: &str) -> PathBuf {
    if path.starts_with("~/") || path.starts_with("~\\") {
        if let Some(home) = dirs() {
            return home.join(&path[2..]);
        }
    }
    PathBuf::from(path)
}

fn dirs() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .ok()
            .map(PathBuf::from)
    }
}

fn print_banner(config: &Config) {
    println!("╔══════════════════════════════════════════╗");
    println!("║         Coderun AI Runtime v0.1.0        ║");
    println!("╚══════════════════════════════════════════╝");
    println!();
    println!("  Socket:  {}", config.daemon.socket_path);
    println!("  Database: {}", config.database.path);
    println!("  Log level: {}", config.logging.level);
    println!("  Timeout:  {}ms", config.daemon.request_timeout_ms);
    println!();
}

fn cleanup(socket_path: &str) {
    let path = PathBuf::from(socket_path);
    if path.exists() {
        if let Err(e) = std::fs::remove_file(&path) {
            warn!(error = %e, "Failed to remove socket file");
        } else {
            info!(path = socket_path, "Socket file removed");
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_path_home() {
        let path = expand_path("~/test/path");
        assert!(path.to_string_lossy().contains("test/path"));
        assert!(!path.to_string_lossy().starts_with("~/"));
    }

    #[test]
    fn test_expand_path_absolute() {
        let path = expand_path("/absolute/path");
        assert_eq!(path, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_expand_path_relative() {
        let path = expand_path("relative/path");
        assert_eq!(path, PathBuf::from("relative/path"));
    }

    #[test]
    fn test_cleanup_nonexistent() {
        // Should not panic
        cleanup("/tmp/nonexistent_socket_12345.sock");
    }
}
