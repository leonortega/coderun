# AI Runtime — Implementation Plan

> Master reference document. Each phase references spec files for implementation details.
> All implementation follows `docs/00-project/PRINCIPLES.md` and `docs/01-architecture/COMPONENTS.md`.

---

## Current Status

**v0.2.0 Released:** August 24, 2026
**Status:** ✅ Complete (6/6 external tool integrations)
**Tests:** 128 passing
**Warnings:** 0

### What's Implemented (v0.2.0)

| Component | Status | Implementation |
|-----------|--------|----------------|
| Config System | ✅ Complete | TOML loading, env overrides |
| Core Types | ✅ Complete | Error enums, IPC types |
| Event Bus | ✅ Complete | broadcast channel |
| Storage | ✅ Complete | SQLite + WAL + tantivy BM25 |
| Repo Intelligence | ✅ Complete | tree-sitter AST + ripgrep search |
| Skill Engine | ✅ Complete | MD/TOML/YAML parsing |
| Knowledge Hub | ✅ Complete | SQLite + engram + FlashRank |
| Model Router | ✅ Complete | Heuristic + LiteLLM routing |
| Optimizer | ✅ Complete | Built-in compressors |
| Context Engine | ✅ Complete | Pipeline assembly |
| Adapter Layer | ✅ Complete | HTTP server (JSON) |
| Daemon Lifecycle | ✅ Complete | Signal handling |
| CLI Commands | ✅ Complete | All subcommands |
| Agent Adapters | ✅ Complete | OpenCode + Claude Code |
| Evaluation | ✅ Complete | Promptfoo framework |

### v0.2.0 External Tool Integration

| Tool | Status | Module |
|------|--------|--------|
| tree-sitter | ✅ Integrated | `coderun-repo-intel/src/parser.rs` |
| ripgrep | ✅ Integrated | `coderun-repo-intel/src/lib.rs` |
| tantivy | ✅ Integrated | `coderun-storage/src/tantivy_index.rs` |
| engram | ✅ Integrated | `coderun-knowledge/src/engram.rs` |
| FlashRank | ✅ Integrated | `coderun-knowledge/src/rerank.rs` |
| LiteLLM | ✅ Integrated | `coderun-router/src/litellm.rs` |
| MkDocs | ⏳ Planned | v0.3.0 |

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

## v0.2.0 Planned Work

See [ROADMAP.md](ROADMAP.md) for detailed plans.

### Priority 1: Core Search & Parsing
- Integrate tree-sitter for AST parsing
- Integrate ripgrep for fast text search
- Integrate tantivy for BM25 indexing

### Priority 2: Knowledge & Memory
- Integrate engram for cross-session memory
- Integrate FlashRank for reranking

### Priority 3: Model Routing
- Integrate LiteLLM for multi-provider routing

### Priority 4: Documentation
- Integrate MkDocs for documentation site
