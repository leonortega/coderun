# AI Runtime — Implementation Plan

> Master reference document. Each phase references spec files for implementation details.
> All implementation follows `docs/00-project/PRINCIPLES.md` and `docs/01-architecture/COMPONENTS.md`.

---

## Current Status

**v0.4.0 Released:** August 24, 2026
**Status:** ✅ Complete (12 crates, Prometheus + DBOS + RwLock concurrency)
**Tests:** 165 passing (6 new in `coderun-workflow`, 2 in `metrics`/`ratelimit`)
**Warnings:** 0 (clippy clean)

### What's Implemented (v0.4.0)

| Component | Status | Implementation |
|-----------|--------|----------------|
| Config System | ✅ Complete | TOML + env (`CODERUN_DBOS_*`, `CODERUN_WORKFLOW_ENABLED`), `WorkflowConfig`, validation |
| Core Types | ✅ Complete | Error enums, IPC (MessagePack+rmp-serde), `IWorkflowEngine` (`Noop`+`DBOS`), HMAC |
| Event Bus | ✅ Complete | broadcast 1000-ring → SQLite `004_events.sql` |
| Storage | ✅ Complete | SQLite WAL + tantivy BM25 + `005_audits.sql` (`audits`+`workflows`) + `cost_usd` |
| Repo Intelligence | ✅ Complete | tree-sitter (4 langs) + ripgrep + tantivy full-text (`search_fulltext`) + `search_structural` heuristic + `graph.rs` + `watcher.rs` + stub `lsp.rs` |
| Skill Engine | ✅ Complete | MD/TOML/YAML parsing, tag matching, conflict/priority |
| Knowledge Hub | ✅ Complete | BM25 top20→rerank adaptive K + engram deterministic 2s fail-open `try_engram_search()` |
| Model Router | ✅ Complete | Heuristic + LiteLLM `fallback_chain()` + `IModelGateway` |
| Optimizer | ✅ Complete | `RtkAdapter` (binary `which rtk`) + built-ins + tee `~/.coderun/logs/tool-failures/` + `tiktoken-rs` |
| Context Engine | ✅ Complete | `BuildContext` `RwLock`, cache-order `skills→docs→code` + `FROZEN PREFIX END` + dedup + reversible `get_original()` |
| Adapter Layer | ✅ Complete | UDS/MessagePack `adapter.rs` (RwLock, rate-limit) + HTTP fallback `http_server.rs` (`/hook`+`/metrics`+`/workflow/*`) + 30s fail-open |
| Daemon Lifecycle | ✅ Complete | Signal handling + metrics + DBOS sidecar spawn when `workflow.enabled` |
| CLI Commands | ✅ Complete | `init --wizard`, `index --watch`, `preview`/`replay`, `workflow start/status/approve/list`, `doctor` 8 probes |
| Agent Adapters | ✅ Complete | OpenCode, Claude Code, **Cursor (v0.4.0)**, **Gemini CLI (v0.4.0)** Tier 1; Tier 2 best-effort |
| DBOS Workflows | ✅ Complete | `coderun-workflow` crate + Node sidecar `workflow/dbos/` (SQLite WAL+Litestream) |
| Metrics | ✅ Complete | `daemon/src/metrics.rs` histogram p95 + `ratelimit.rs` per-session bucket |
| Evaluation | ✅ Complete | Promptfoo + UDS custom provider |
| Distribution | ✅ Complete | `Dockerfile` (distroless), `Formula/coderun.rb` (brew+launchd), `cargo-wix` MSI scaffold |

### v0.4.0 External Tool Integration

| Tool | Status | Module |
|------|--------|--------|
| tree-sitter | ✅ Integrated (incremental `old_tree`) | `coderun-repo-intel/src/parser.rs` |
| ripgrep | ✅ Integrated | `coderun-repo-intel/src/lib.rs:338` |
| tantivy | ✅ Integrated (MmapDirectory) | `coderun-storage/src/tantivy_index.rs` + `search_fulltext()` |
| ast-grep | ✅ Heuristic (tree-sitter+regex, `sg-core` deferred) | `repo-intel/src/lib.rs:352` |
| engram | ✅ Integrated (2s fail-open) | `coderun-knowledge/src/engram.rs` |
| FlashRank (`ort`) | ✅ Integrated (TF-IDF fallback, int8 deferred) | `coderun-knowledge/src/rerank.rs` |
| LiteLLM | ✅ Integrated (fallback chain) | `coderun-router/src/litellm.rs` |
| RTK | ✅ Integrated (binary optional) | `coderun-optimizer/src/rtk.rs` |
| DBOS Transact | ✅ Integrated (sidecar) | `crates/coderun-workflow` + `workflow/dbos/` |
| MkDocs | ✅ Integrated | `mkdocs.yml` + `docs/dashboards` |
| Prometheus | ✅ Integrated | `daemon/src/metrics.rs` + `GET /metrics` |

---

## Phase 0: Project Scaffolding ✅

**Goal:** Create the Rust workspace, directory structure, and configuration foundation.

- [x] `0.1` Initialize Rust workspace with `cargo init --name coderun`
- [x] `0.2` Create workspace `Cargo.toml` with shared dependencies
- [x] `0.3` Create crate structure (11 crates)
- [x] `0.4` Add shared dependencies to workspace
- [x] `0.5` Create directory structure per spec
- [x] `0.6` Create `README.md` with build instructions
- [x] `0.7` Verify `cargo build` compiles cleanly
- [x] `0.8` Verify `cargo test` passes

---

## Phase 1: Configuration System ✅

**Goal:** Load, validate, and merge configuration from TOML files and environment variables.

- [x] `1.1` Define `Config` struct in `crates/coderun-core/src/config.rs`
- [x] `1.2` Implement config loading: user → project → environment merge order
- [x] `1.3` Implement environment variable overrides (`CODERUN_*`)
- [x] `1.4` Implement config validation with descriptive error messages
- [x] `1.5` Implement `config show` and `config validate` CLI commands
- [x] `1.6` Write unit tests for config loading and merging
- [x] `1.7` Write unit tests for config validation

---

## Phase 2: Error Types and Core Types ✅

**Goal:** Define all shared types, error enums, and data structures used across modules.

- [x] `2.1` Define `RuntimeError` enum
- [x] `2.2` Define `CorrelationId` newtype wrapper
- [x] `2.3` Define IPC message types
- [x] `2.4` Define `ContextHints` struct
- [x] `2.5` Define `TaskRequest` struct
- [x] `2.6` Define `SearchResult`, `SearchResults` structs
- [x] `2.7` Define `KnowledgeEntry` struct
- [x] `2.8` Define `SkillMatch` struct
- [x] `2.9` Define `RoutingDecision` struct
- [x] `2.10` Define `ContextPack` struct
- [x] `2.11` Define `TokenUsage` struct
- [x] `2.12` Define `CodeFile` struct
- [x] `2.13` Define `OutputType` enum
- [x] `2.14` Implement `serde::Serialize` / `serde::Deserialize` on all types
- [x] `2.15` Write unit tests for type serialization/deserialization

---

## Phase 3: Event Bus ✅

**Goal:** Implement the async-only observability event system.

- [x] `3.1` Define `RuntimeEvent` enum
- [x] `3.2` Implement `EventBus` struct using `tokio::sync::broadcast`
- [x] `3.3` Implement `emit(event)` method
- [x] `3.4` Implement `subscribe()` method
- [x] `3.5` Implement in-memory event buffer
- [x] `3.6` Implement `get_recent_events(n)`
- [x] `3.7` Implement `get_events_by_correlation(id)`
- [x] `3.8` Write unit tests

---

## Phase 4: Local Storage (SQLite) ✅

**Goal:** Implement SQLite database with schema, migrations, and connection pooling.

- [x] `4.1` Implement `Database` struct
- [x] `4.2` Implement database initialization
- [x] `4.3` Implement migration 001
- [x] `4.4` Implement migration runner
- [x] `4.5` Implement `files` table operations
- [x] `4.6` Implement `symbols` table operations
- [x] `4.7` Implement `token_usage` table operations
- [x] `4.8` Implement slow query logging
- [x] `4.9` Write unit tests
- [x] `4.10` Write integration tests

---

## Phase 5: Repository Intelligence ✅

**Goal:** Implement incremental indexing, search, and metadata storage.

- [x] `5.1` Implement `RepositoryIntelligence` struct
- [x] `5.2` Implement directory walker with ignore patterns
- [x] `5.3` Implement language detection from file extensions
- [ ] `5.4` Integrate tree-sitter for AST parsing *(planned for v0.2.0)*
- [x] `5.5` Implement incremental indexing (content hash)
- [x] `5.6` Implement `search_text(query)` with regex
- [ ] `5.7` Implement `search_structural(pattern)` with ast-grep *(planned for v0.2.0)*
- [ ] `5.8` Implement `search_fulltext(query)` with BM25/tantivy *(planned for v0.2.0)*
- [x] `5.9` Implement `get_file_content(path, line_range)`
- [x] `5.10` Implement `get_file_info(path)`
- [x] `5.11` Implement `get_symbol_info(query)`
- [x] `5.12` Emit `RepositoryUpdated` event
- [x] `5.13` Log indexing progress
- [x] `5.14` Handle binary files gracefully
- [x] `5.15` Write unit tests
- [x] `5.16` Write unit tests for incremental indexing
- [x] `5.17` Write integration tests
- [x] `5.18` Write tests for text search

---

## Phase 6: Skill Engine ✅

**Goal:** Implement deterministic tag-based skill matching from community-format files.

- [x] `6.1` Implement `SkillEngine` struct
- [x] `6.2` Define `Skill` struct
- [x] `6.3` Implement Markdown skill parser
- [x] `6.4` Implement TOML skill parser
- [x] `6.5` Implement YAML skill parser
- [x] `6.6` Implement skill validation
- [x] `6.7` Implement `load_skills(directory)`
- [x] `6.8` Implement `match_skills(task_description, max_skills)`
- [x] `6.9` Implement conflict detection
- [x] `6.10` Implement priority resolution
- [x] `6.11` Implement `reload_skills()`
- [x] `6.12` Implement `list_skills()`
- [x] `6.13` Write unit tests
- [x] `6.14` Write unit tests for skill matching
- [x] `6.15` Write unit tests for conflict detection
- [x] `6.16` Create sample skill files

---

## Phase 7: Knowledge Hub ✅

**Goal:** Implement unified knowledge storage and retrieval.

- [x] `7.1` Implement `KnowledgeHub` struct
- [x] `7.2` Implement SQLite knowledge table operations
- [ ] `7.3` Implement BM25/tantivy knowledge index *(planned for v0.2.0)*
- [x] `7.4` Implement `retrieve_knowledge()` with LIKE queries
- [ ] `7.5` Integrate FlashRank via `ort` *(planned for v0.2.0)*
- [ ] `7.6` Integrate engram for memory *(planned for v0.2.0)*
- [x] `7.7` Implement `extract_knowledge()`
- [x] `7.8` Implement confidence decay
- [x] `7.9` Write unit tests
- [x] `7.10` Write unit tests for skill matching
- [ ] `7.11` Write integration tests for BM25 + FlashRank *(planned for v0.2.0)*
- [ ] `7.12` Write tests for engram integration *(planned for v0.2.0)*

---

## Phase 8: Model Router ✅

**Goal:** Implement heuristic complexity scoring and tier-based model selection.

- [x] `8.1` Implement `ModelRouter` struct
- [x] `8.2` Implement `select_model(request)`
- [x] `8.3` Implement technical term detection
- [x] `8.4` Implement action verb detection
- [x] `8.5` Implement tier-to-model mapping
- [x] `8.6` Implement model override support
- [ ] `8.7` Implement fallback chain via LiteLLM *(planned for v0.2.0)*
- [x] `8.8` Emit `ModelSelected` event
- [x] `8.9` Log scoring breakdown
- [x] `8.10` Log routing decision
- [x] `8.11` Write unit tests
- [x] `8.12` Write unit tests for tier mapping
- [x] `8.13` Write unit tests for fallback logic

---

## Phase 9: Execution Optimizer ✅

**Goal:** Implement tool-output compression.

- [x] `9.1` Implement `ExecutionOptimizer` struct
- [x] `9.2` Implement `compress_output(tool_output)`
- [x] `9.3` Implement file read compression
- [x] `9.4` Implement search result compression
- [x] `9.5` Implement shell output compression
- [ ] `9.6` Integrate RTK *(planned for v0.2.0)*
- [x] `9.7` Implement tee-on-failure (fail-open)
- [x] `9.8` Implement token counting
- [x] `9.9` Emit `ToolExecuted` event
- [x] `9.10` Log compression ratio
- [x] `9.11` Write unit tests
- [x] `9.12` Write unit tests for fail-open
- [ ] `9.13` Write integration tests for RTK *(planned for v0.2.0)*

---

## Phase 10: Context Engine ✅

**Goal:** Implement `BuildContext(task)` — the central pipeline.

- [x] `10.1` Implement `ContextEngine` struct
- [x] `10.2` Implement `build_context(task)`
- [x] `10.3` Implement cache-aware ordering
- [x] `10.4` Implement deduplication
- [x] `10.5` Implement token budget enforcement
- [x] `10.6` Implement YAML Context Pack serialization
- [x] `10.7` Implement session fingerprint management
- [x] `10.8` Implement token counting
- [x] `10.9` Emit `ContextBuilt` event
- [x] `10.10` Log token usage
- [x] `10.11` Write unit tests
- [x] `10.12` Write unit tests for deduplication
- [x] `10.13` Write unit tests for YAML serialization
- [x] `10.14` Write integration tests
- [x] `10.15` Write integration tests for fail-open

---

## Phase 11: Adapter Layer ✅

**Goal:** Implement HTTP server with JSON IPC and fail-open behavior.

- [x] `11.1` Implement `AdapterLayer` struct
- [x] `11.2` Implement HTTP server with axum
- [x] `11.3` Implement JSON decoding
- [x] `11.4` Implement request validation
- [x] `11.5` Implement correlation ID generation
- [x] `11.6` Implement PreGeneration handler
- [x] `11.7` Implement PreToolCall handler
- [x] `11.8` Implement JSON encoding
- [x] `11.9` Implement fail-open behavior
- [x] `11.10` Implement 30s timeout
- [x] `11.11` Emit `ResponseGenerated` event
- [x] `11.12` Log requests and responses
- [x] `11.13` Write unit tests
- [x] `11.14` Write unit tests for fail-open
- [x] `11.15` Write integration tests

---

## Phase 12: Daemon Lifecycle ✅

**Goal:** Implement daemon startup, shutdown, signal handling.

- [x] `12.1` Implement `serve` command
- [x] `12.2` Implement signal handling
- [x] `12.3` Implement graceful shutdown
- [x] `12.4` Implement force shutdown
- [x] `12.5` Print startup banner
- [x] `12.6` Write integration tests

---

## Phase 13: CLI Commands ✅

**Goal:** Implement all CLI commands.

- [x] `13.1` Implement `coderun serve`
- [x] `13.2` Implement `coderun init`
- [x] `13.3` Implement `coderun index`
- [x] `13.4` Implement `coderun preview <prompt>`
- [ ] `13.5` Implement `coderun replay <correlation_id>` *(planned for v0.2.0)*
- [x] `13.6` Implement `coderun status`
- [x] `13.7` Implement `coderun skills list`
- [x] `13.8` Implement `coderun skills validate`
- [x] `13.9` Implement `coderun config show`
- [x] `13.10` Implement `coderun config validate`
- [x] `13.11` Implement `coderun doctor`
- [x] `13.12` Write unit tests
- [x] `13.13` Write integration tests

---

## Phase 14: Agent Adapters ✅

**Goal:** Implement agent-specific adapter configurations.

- [x] `14.1` Research opencode hook API
- [x] `14.2` Research Claude Code hook API
- [x] `14.3` Create OpenCode plugin (TypeScript)
- [x] `14.4` Create Claude Code hooks (shell scripts)
- [x] `14.5` Write documentation

---

## Phase 15: Evaluation Framework ✅

**Goal:** Set up Promptfoo evaluation.

- [x] `15.1` Create `eval/` directory structure
- [x] `15.2` Create Promptfoo configuration
- [x] `15.3` Create evaluation dataset
- [x] `15.4` Implement evaluation runner script
- [x] `15.5` Run baseline evaluation
- [x] `15.6` Document evaluation results

---

## Phase 16: Hardening and Documentation ✅

**Goal:** Final hardening and documentation.

- [x] `16.1` Add comprehensive error messages
- [x] `16.2` Add structured logging
- [x] `16.3` Add correlation ID propagation
- [ ] `16.4` Add performance benchmarks *(planned for v0.2.0)*
- [ ] `16.5` Add memory usage benchmarks *(planned for v0.2.0)*
- [x] `16.6` Write README
- [x] `16.7` Write CONTRIBUTING.md
- [x] `16.8` Create release configuration
- [x] `16.9` Run full test suite (108 tests passing)
- [x] `16.10` Run clippy (zero warnings)
- [x] `16.11` Run `cargo audit` (zero vulnerabilities)

---

## v0.4.0 What's New (production hardening + DBOS)

See [ROADMAP.md](ROADMAP.md) + `docs/V0_4_0_PLAN.md` (5-week plan, P0-P3).

### Priority 0 — DBOS (chosen over Temporal, V0_4_0_PLAN.md:1.1)
- `WorkflowConfig` + `005_audits.sql` (`audits`+`workflows`) + `crate coderun-workflow` `DBOSWorkflowEngine` HTTP bridge to `workflow/dbos/` Node sidecar (SQLite WAL+Litestream, `governed.ts` `DBOS.workflow`+`transaction`+`waitForSignal`, approval gate, audit, fail-open `wf_*`, HMAC `X-Coderun-Signature`)
- `GET /metrics` + `GET /health` + `POST /workflow/*` wired in `daemon/src/http_server.rs:93`

### Priority 1 — Observability + Security + Concurrency
- `daemon/src/metrics.rs` Prometheus histogram `coderun_build_context_duration_seconds` (Timer RAII), `global()` singleton, Grafana `docs/dashboards/coderun.json`, `deploy/prometheus/alerts.yml` (p95>50ms, fail-open>5%)
- `daemon/src/ratelimit.rs` token-bucket 10/s burst 20 per `session_id` at `AdapterLayer`, `verify_hmac()` before DBOS→daemon
- `AdapterLayer` `Mutex→RwLock` `adapter.rs:44`, `RateLimiter` + audit spill off hot path, soak 20×100

### Priority 2 — Distribution + Multi-agent
- Cursor/Gemini CLI Tier 1 `adapters/cursor/extension.ts` + `adapters/gemini/hooks.sh` (UDS/MessagePack primary, HTTP fallback, 30s fail-open `V0_4_0_PLAN.md:4`), Continue promoted, `ADAPTERS.md:10` updated
- `Dockerfile` multi-stage `rust:1.75-slim → distroless`, `Formula/coderun.rb` (brew tap+launchd service), `cargo-wix` MSI scaffold, `benches/context_bench.rs` criterion

### Historical

See `CHANGELOG.md:0.4.0` for full delta from v0.3.0. v0.3.0 closed spec 58%→90%+ (UDS, tiktoken, cache-aware pack, repo-intel completion); v0.4.0 closes 90%→99%+ production SLOs.
