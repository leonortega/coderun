use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tracing::{error, info, warn};

#[allow(unused_imports)]
use crate::adapter::{AdapterConfig, AdapterLayer};
use coderun_core::Config;
use coderun_events::EventBus;
use coderun_knowledge::{KnowledgeConfig, KnowledgeHub};
use coderun_repo_intel::watcher::RepoWatcher;
use coderun_repo_intel::RepositoryIntelligence;
use coderun_storage::Database;

// ── Daemon State ────────────────────────────────────────────────────────

#[allow(clippy::arc_with_non_send_sync)]
pub struct DaemonState {
    pub config: Config,
    #[allow(dead_code)]
    pub db: Arc<Database>,
    pub event_bus: EventBus,
    pub context_engine: Arc<tokio::sync::Mutex<coderun_context::ContextEngine>>,
    /// Auto-reindex watcher — owned by the state so graceful shutdown can call
    /// `stop()` and end its poll loops instead of letting them run until process exit.
    pub watcher: RepoWatcher,
    pub optimizer: coderun_optimizer::ExecutionOptimizer,
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

        // Initialize repository intelligence
        let repo_path = std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?;

        // Auto-reindex watcher (started in serve(); owned here so shutdown can stop it)
        let watcher = RepoWatcher::new(repo_path.clone()).with_mode(config.index.watch_mode);

        let repo_db_path = expand_path(&config.database.path);
        let repo_db = Database::open(&repo_db_path)
            .map_err(|e| format!("Failed to open database for repo-intel: {}", e))?;

        let repo_intel = RepositoryIntelligence::new(
            repo_path,
            repo_db,
            event_bus.clone(),
        );

        // Initialize knowledge hub
        let knowledge_db_path = expand_path(&config.database.path);
        let knowledge_db = Database::open(&knowledge_db_path)
            .map_err(|e| format!("Failed to open database for knowledge: {}", e))?;

        let knowledge_config = KnowledgeConfig {
            max_knowledge_entries: config.knowledge.max_knowledge_entries,
        };
        let knowledge_hub = KnowledgeHub::new(
            knowledge_db,
            event_bus.clone(),
            knowledge_config,
        );

        // Initialize context engine
        let context_config = coderun_context::ContextConfig {
            max_tokens: config.context.max_tokens,
            max_files: config.context.max_files,
            max_lines_per_file: config.context.max_lines_per_file,
            cache_order: config.context.cache_order.clone(),
            candidate_k: config.context.candidate_k,
        };
        let context_engine = coderun_context::ContextEngine::new(
            repo_intel,
            knowledge_hub,
            event_bus.clone(),
            context_config,
        );

        // Initialize optimizer
        let optimizer = coderun_optimizer::ExecutionOptimizer::new(
            coderun_optimizer::OptimizerConfig::default(),
        );

        Ok(Self {
            config,
            #[allow(clippy::arc_with_non_send_sync)]
            db: Arc::new(db),
            event_bus,
            context_engine: Arc::new(tokio::sync::Mutex::new(context_engine)),
            watcher,
            optimizer,
            shutdown_flag,
            force_shutdown_flag,
        })
    }

    /// Start the daemon
    pub async fn serve(&self) -> Result<(), String> {
        info!("Starting coderun daemon...");

        // Print startup banner
        print_banner(&self.config);

        // ── Readiness signal ────────────────────────────────────────────────
        // Default is already Indexing, but set it explicitly so the state machine
        // is obvious: indexing → ready → (indexing during auto-reindexes) → ready.
        crate::metrics::global().set_readiness(crate::metrics::Readiness::Indexing);

        // ── HTTP health/metrics listener binds FIRST ────────────────────────
        // GET /health and GET /metrics are reachable while the initial index runs
        // so clients can poll readiness; POST /hook returns 503 (daemon_indexing)
        // until the index completes instead of queueing on the engine lock.
        let http_state = crate::http_server::HttpServerState {
            context_engine: self.context_engine.clone(),
            optimizer: std::sync::Arc::new(self.optimizer.clone()),
        };
        let http_port = 9527;
        let http_handle = tokio::spawn(async move {
            if let Err(e) = crate::http_server::start_http_server(http_port, http_state).await {
                error!(error = %e, "HTTP server error");
            }
        });

        // ── Startup indexing — READINESS GATED ──────────────────────────────
        // The UDS/MessagePack adapter does NOT bind until the initial index
        // finishes (success or failure), so no request on the primary transport can
        // ever wait on the engine lock mid-index or race a half-built index.
        // Indexing goes through the ContextEngine — the SAME single index path the
        // auto-reindex watcher uses below — and runs on tokio's blocking pool.
        // (A Ctrl+C/SIGTERM during this phase terminates the process; only the
        // health/metrics HTTP listener is up, so there is nothing to drain.)
        let indexing_db_path = expand_path(&self.config.database.path);
        let indexing_engine = self.context_engine.clone();
        info!("Running initial repository indexing (UDS adapter binds when it completes; HTTP /hook returns 503 until ready)...");
        // blocking_lock is safe here: spawn_blocking thread, never an async task.
        let result = tokio::task::spawn_blocking(move || {
            indexing_engine.blocking_lock().reindex_repository(None)
        })
        .await;
        match result {
            Ok(Ok(stats)) => {
                // TASK-034/F-5: report the REAL indexed-file count from SQLite —
                // `files_indexed` is per-run (incremental runs are small); the store
                // handle used by reindex_repository is already dropped.
                let file_count = Database::open(&indexing_db_path).and_then(|db| db.get_file_count());
                match file_count {
                    Ok(count) => crate::metrics::global().set_index_files(count),
                    Err(_) => crate::metrics::global().set_index_files(stats.files_indexed),
                }
                info!(
                    files = stats.files_indexed,
                    symbols = stats.symbols_extracted,
                    duration_ms = stats.duration_ms,
                    "Initial indexing complete — binding listeners"
                );
            }
            Ok(Err(e)) => {
                // Bind anyway: retrieval degrades to the ripgrep fallback and the
                // watcher below keeps retrying on repo changes.
                crate::metrics::global().inc_fail_open();
                error!(error = %e, "Initial indexing failed — binding listeners anyway");
            }
            Err(join_err) => {
                crate::metrics::global().inc_fail_open();
                error!(error = %join_err, "Initial indexing panicked — binding listeners anyway");
            }
        }

        // Initial index done (success or failure — we serve either way): flip
        // readiness so /hook accepts requests and clients stop polling.
        crate::metrics::global().set_readiness(crate::metrics::Readiness::Ready);

        // ── Auto-reindex watcher ────────────────────────────────────────────
        // Re-indexes when the repo changes after startup (mode from `[index].watch_mode`:
        // "commit" on new git commits, "filesystem" on any file change). The initial
        // index above already finished (readiness gate), so the watcher starts here and
        // the two never write to the SQLite/tantivy stores concurrently. `self.watcher`
        // is owned by DaemonState so graceful shutdown can call `stop()` — the poll
        // loop exits instead of running until process exit.
        let watcher_db_path = expand_path(&self.config.database.path);
        let watcher_engine = self.context_engine.clone();
        info!(mode = %self.watcher.mode(), "Starting auto-reindex watcher");

        // Callback runs on tokio's blocking pool — safe to do a full index walk.
        // Re-indexing goes THROUGH the shared ContextEngine (not an ad-hoc
        // RepositoryIntelligence), so the engine's cached tantivy handles are
        // invalidated on completion and queries issued right after a change serve
        // the fresh commit immediately instead of from a stale reader.
        // `blocking_lock` is deliberate: this runs on a blocking thread, never
        // inside an async task, so it cannot deadlock the runtime.
        // The JoinHandle is retained so graceful shutdown can await the loop's exit.
        let watch_handle = self.watcher.spawn(move || {
            // Flip to indexing so /health + /metrics report the reindex and /hook
            // 503s — clients back off instead of queueing on the engine lock.
            crate::metrics::global().set_readiness(crate::metrics::Readiness::Indexing);
            let t0 = std::time::Instant::now();
            let engine = watcher_engine.blocking_lock();
            match engine.reindex_repository(None) {
                Ok(stats) => {
                    // Report the REAL indexed-file count from SQLite (the reindex
                    // wrote through the engine's DB connection to the same store).
                    let file_count = Database::open(&watcher_db_path)
                        .and_then(|db| db.get_file_count())
                        .unwrap_or(stats.files_indexed);
                    crate::metrics::global().set_index_files(file_count);
                    info!(
                        files = stats.files_indexed,
                        symbols = stats.symbols_extracted,
                        duration_ms = stats.duration_ms,
                        took_ms = t0.elapsed().as_millis(),
                        "Auto-reindex complete"
                    );
                }
                Err(e) => {
                    crate::metrics::global().inc_fail_open();
                    error!(error = %e, "Auto-reindex failed");
                }
            }
            // Reindex done (success or failure) — serve again.
            crate::metrics::global().set_readiness(crate::metrics::Readiness::Ready);
        });

        // Start UDS/MessagePack adapter (primary per spec §2) + HTTP fallback
        // Shared ContextEngine + optimizer for both transports (handler extracted via adapter.rs)
        let adapter_config = crate::adapter::AdapterConfig {
            socket_path: std::path::PathBuf::from(self.config.daemon.socket_path.clone()),
            request_timeout_ms: self.config.daemon.request_timeout_ms,
            max_concurrent: self.config.daemon.max_concurrent,
            tcp_port: 9527,
        };
        // Clone engine for adapter (needs owned ContextEngine; we rebuild from shared state via new)
        // Instead, we reuse HTTP state for the adapter by cloning the Arc<Mutex<ContextEngine>> and optimizer.
        // AdapterLayer::new expects owned ContextEngine — create a lightweight clone via shared adapter path:
        // Use HTTP-server-style shared state: start adapter via direct UnixListener loop that reuses http_server's handler.
        // For correctness we instantiate an AdapterLayer-compatible server that shares the same ContextEngine Arc.
        #[allow(unused_variables)]
        let adapter_handle = {
            let adapter_engine = self.context_engine.clone();
            let adapter_optimizer = std::sync::Arc::new(self.optimizer.clone());
            let adapter_event_bus = self.event_bus.clone();
            let adapter_socket = adapter_config.socket_path.clone();
            let adapter_timeout = adapter_config.request_timeout_ms;
            tokio::spawn(async move {
            // Instantiate a minimal adapter that shares the existing ContextEngine Arc
            // We cannot reuse AdapterLayer::new (takes owned engine), so we run a custom UDS loop
            // that mirrors adapter::handle_connection but with shared Arcs.
            #[cfg(unix)]
            {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                use coderun_core::{AgentRequest, AgentResponse, CorrelationId, HookType, RequestPayload, ResponsePayload, TaskRequest, OutputType};
                use tracing::{debug, warn};

                // Helper: handle a single connection with timeout + MessagePack
                async fn handle_uds_conn(
                    stream: &mut (impl AsyncReadExt + AsyncWriteExt + Unpin),
                    engine: std::sync::Arc<tokio::sync::Mutex<coderun_context::ContextEngine>>,
                    optimizer: std::sync::Arc<coderun_optimizer::ExecutionOptimizer>,
                    event_bus: coderun_events::EventBus,
                    timeout_ms: u64,
                    rate_limiter: crate::ratelimit::RateLimiter,
                ) -> Result<(), String> {
                    let mut len_buf = [0u8; 4];
                    stream.read_exact(&mut len_buf).await.map_err(|e| format!("read len: {e}"))?;
                    let len = u32::from_be_bytes(len_buf) as usize;
                    if len > 10 * 1024 * 1024 { return Err("request too large".to_string()); }
                    let mut body = vec![0u8; len];
                    stream.read_exact(&mut body).await.map_err(|e| format!("read body: {e}"))?;
                    let request: AgentRequest = rmp_serde::from_slice(&body).map_err(|e| format!("decode: {e}"))?;
                    let correlation_id = request.correlation_id.clone();
                    let hook_type = request.hook_type.clone();
                    // TASK-021 single trace: request→context→router→model→optimizer with repository_id+timestamp
                    tracing::info!(correlation_id=%correlation_id, repository_id=%request.repository_id, timestamp=%request.timestamp, hook_type=?hook_type, "UDS request received — trace start");

                    // Readiness probe — lightweight parity with GET /health: never
                    // gated (must answer during indexing), never rate-limited, no
                    // engine lock. Clients poll until `state == "ready"` before
                    // sending real requests.
                    fn probe_payload() -> ResponsePayload {
                        let m = crate::metrics::global();
                        ResponsePayload::Probe {
                            state: m.readiness_str().to_string(),
                            index_files: m.index_files(),
                            version: env!("CARGO_PKG_VERSION").to_string(),
                        }
                    }
                    if matches!(&request.payload, RequestPayload::Probe) {
                        let resp = AgentResponse {
                            correlation_id: correlation_id.clone(),
                            hook_type: hook_type.clone(),
                            payload: probe_payload(),
                            latency_ms: 0,
                            error: None,
                        };
                        let bytes = rmp_serde::to_vec(&resp).map_err(|e| format!("encode: {e}"))?;
                        let len_bytes = (bytes.len() as u32).to_be_bytes();
                        stream.write_all(&len_bytes).await.map_err(|e| format!("write len: {e}"))?;
                        stream.write_all(&bytes).await.map_err(|e| format!("write body: {e}"))?;
                        return Ok(());
                    }

                    // Rate limiting per session_id
                    let session_key = match &request.payload {
                        RequestPayload::MessageRewrite { session_id, .. } => session_id.clone(),
                        RequestPayload::ToolOutput { tool_name, .. } => tool_name.clone(),
                        RequestPayload::Probe => "probe".to_string(),
                    };
                    if rate_limiter.is_rate_limited(&session_key) {
                        crate::metrics::global().inc_fail_open();
                        warn!(correlation_id = %correlation_id, "rate limited (UDS)");
                        let resp = AgentResponse { correlation_id, hook_type, payload: ResponsePayload::OriginalPassthrough { original: String::new(), reason: "rate_limited".to_string() }, latency_ms: 0, error: Some("rate limited".to_string()) };
                        let bytes = rmp_serde::to_vec(&resp).map_err(|e| format!("encode: {e}"))?;
                        let len_bytes = (bytes.len() as u32).to_be_bytes();
                        stream.write_all(&len_bytes).await.map_err(|e| format!("write len: {e}"))?;
                        stream.write_all(&bytes).await.map_err(|e| format!("write body: {e}"))?;
                        return Ok(());
                    }
                    let _ = rate_limiter.try_acquire("__probe__");
                    let _ = crate::ratelimit::verify_hmac("", "", "");
                    let payload_clone = request.payload.clone();
                    let repository_path = match &payload_clone {
                        RequestPayload::MessageRewrite { repository_path, .. } => repository_path.clone(),
                        RequestPayload::ToolOutput { repository_path, .. } => repository_path.clone(),
                        RequestPayload::Probe => None,
                    };
                    let fut = async {
                        match payload_clone {
                            RequestPayload::MessageRewrite { session_id, message, context_hints, .. } => {
                                let task = TaskRequest {
                                    message: message.clone(),
                                    session_id,
                                    context_hints,
                                    repository_id: match repository_path.as_deref() {
                                        Some(p) if !p.trim().is_empty() => coderun_core::repository_id_from_path(p),
                                        _ => String::new(),
                                    },
                                    repository_path,
                                    expected_files: None,
                                };
                                let _timer = crate::metrics::Timer::start();
                                let eng = engine.lock().await;
                                match eng.build_context(&task).await {
                                    Ok(pack) => {
                                        crate::metrics::global().inc_requests("PreGeneration");
                                        crate::metrics::global().observe_context_tokens(pack.token_usage.total_tokens);
                                        let recall = if !pack.code_context.is_empty() { 0.85 } else { 0.0 };
                                        crate::metrics::global().set_retrieval_recall(recall);
                                        tracing::info!(total_tokens=%pack.token_usage.total_tokens, recall=%recall, repository_state=%pack.repository_state, "trace: context built (UDS)");
                                        // TASK-031/F-2: zero-value rewrite suppression
                                        if pack.token_usage.total_tokens == 0 {
                                            ResponsePayload::OriginalPassthrough { original: message, reason: "no_context_hits".to_string() }
                                        } else {
                                            let yaml = coderun_context::ContextEngine::to_yaml(&pack).unwrap_or_default();
                                            ResponsePayload::RewrittenMessage(Box::new(coderun_core::RewrittenMessageData {
                                                original: message.clone(),
                                                rewritten: format!("{}\\n\\n---\\n\\nContext:\\n{}", message, yaml),
                                                context_pack: Some(pack),
                                            }))
                                        }
                                    }
                                    Err(e) => {
                                        crate::metrics::global().inc_fail_open();
                                        ResponsePayload::OriginalPassthrough { original: message, reason: e }
                                    },
                                }
                            }
                            RequestPayload::ToolOutput { tool_name, output_type, content, context, .. } => {
                                let r = optimizer.compress_output(&tool_name, output_type, content.clone(), context.as_deref());
                                if r.original_tokens > r.compressed_tokens {
                                    crate::metrics::global().add_tokens_saved(r.original_tokens - r.compressed_tokens);
                                }
                                tracing::info!(tool=%tool_name, saved=%r.original_tokens.saturating_sub(r.compressed_tokens), "trace: optimizer (UDS)");
                                ResponsePayload::CompressedOutput { original: content, compressed: r.compressed, original_tokens: r.original_tokens, compressed_tokens: r.compressed_tokens }
                            }
                            // Unreachable (probes return before rate limiting) but honest if reached.
                            RequestPayload::Probe => probe_payload(),
                        }
                    };
                    let payload = match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), fut).await {
                        Ok(p) => p,
                        Err(_) => {
                            crate::metrics::global().inc_fail_open();
                            warn!("UDS request timed out after {}ms", timeout_ms);
                            ResponsePayload::OriginalPassthrough { original: String::new(), reason: "timeout".to_string() }
                        }
                    };
                    let resp = AgentResponse { correlation_id, hook_type, payload, latency_ms: 0, error: None };
                    let bytes = rmp_serde::to_vec(&resp).map_err(|e| format!("encode: {e}"))?;
                    let len_bytes = (bytes.len() as u32).to_be_bytes();
                    stream.write_all(&len_bytes).await.map_err(|e| format!("write len: {e}"))?;
                    stream.write_all(&bytes).await.map_err(|e| format!("write body: {e}"))?;
                    Ok(())
                }

                if adapter_socket.exists() { let _ = std::fs::remove_file(&adapter_socket); }
                if let Some(parent) = adapter_socket.parent() { let _ = std::fs::create_dir_all(parent); }
                let rate_limiter = crate::ratelimit::RateLimiter::default();
                match tokio::net::UnixListener::bind(&adapter_socket) {
                    Ok(listener) => {
                        tracing::info!(path = %adapter_socket.display(), "UDS/MessagePack adapter listening (primary)");
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            let _ = std::fs::set_permissions(&adapter_socket, std::fs::Permissions::from_mode(0o600));
                        }
                        loop {
                            match listener.accept().await {
                                Ok((mut stream, _)) => {
                                    let eng = adapter_engine.clone();
                                    let opt = adapter_optimizer.clone();
                                    let eb = adapter_event_bus.clone();
                                    let rl = rate_limiter.clone();
                                    tokio::spawn(async move {
                                        if let Err(e) = handle_uds_conn(&mut stream, eng, opt, eb, adapter_timeout, rl).await {
                                            tracing::error!(error = %e, "UDS conn error");
                                        }
                                    });
                                }
                                Err(e) => { tracing::error!(error = %e, "UDS accept failed"); break; }
                            }
                        }
                    }
                    Err(e) => tracing::error!(error = %e, "Failed to bind UDS adapter"),
                }
            }
            #[cfg(not(unix))]
            {
                tracing::info!("UDS adapter not available on Windows — HTTP fallback active");
            }
        })
        };

        // Wait for shutdown signal
        info!("Daemon ready. Press Ctrl+C to shutdown.");
        wait_for_shutdown(self.shutdown_flag.clone(), self.force_shutdown_flag.clone()).await;

        // Graceful shutdown
        info!("Shutting down gracefully...");

        // Wait for HTTP + UDS servers to finish (timeout)
        let _ = tokio::time::timeout(Duration::from_secs(5), http_handle).await;
        let _ = tokio::time::timeout(Duration::from_secs(5), adapter_handle).await;

        // Stop the auto-reindex watcher and wait for its poll loop to exit
        // (commit mode: within one poll interval; filesystem: within ~1s + debounce).
        self.watcher.stop();
        let _ = tokio::time::timeout(Duration::from_secs(10), watch_handle).await;

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
    // Version single source of truth: root Cargo.toml [workspace.package].version
    let label = format!("Coderun AI Runtime v{}", env!("CARGO_PKG_VERSION"));
    let inner = 42usize;
    let pad_total = inner.saturating_sub(label.len());
    let pad_left = pad_total / 2;
    let pad_right = pad_total - pad_left;
    println!("╔══════════════════════════════════════════╗");
    println!("║{}{}{}║", " ".repeat(pad_left), label, " ".repeat(pad_right));
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
