# AI Runtime — Implementation Plan

> Master reference document. Each phase references spec files for implementation details.
> All implementation follows `docs/00-project/PRINCIPLES.md` and `docs/01-architecture/COMPONENTS.md`.

---

## Phase 0: Project Scaffolding

**Goal:** Create the Rust workspace, directory structure, and configuration foundation.

- [ ] `0.1` Initialize Rust workspace with `cargo init --name coderun`
- [ ] `0.2` Create workspace `Cargo.toml` with shared dependencies
- [ ] `0.3` Create crate structure:
  - `crates/coderun-core/` — shared types, errors, config
  - `crates/coderun-daemon/` — daemon binary (UDS server, signal handling)
  - `crates/coderun-cli/` — CLI binary (clap commands)
  - `crates/coderun-repo-intel/` — Repository Intelligence module
  - `crates/coderun-knowledge/` — Knowledge Hub module
  - `crates/coderun-skills/` — Skill Engine module
  - `crates/coderun-context/` — Context Engine module
  - `crates/coderun-router/` — Model Router module
  - `crates/coderun-optimizer/` — Execution Optimizer module
  - `crates/coderun-events/` — Event Bus module
  - `crates/coderun-storage/` — Local Storage (SQLite) module
- [ ] `0.4` Add shared dependencies to workspace:
  - `tokio` (async runtime)
  - `serde` + `serde_json` + `serde_yaml` (serialization)
  - `toml` (config)
  - `tracing` + `tracing-subscriber` (logging)
  - `anyhow` + `thiserror` (errors)
  - `uuid` (correlation IDs)
  - `sha2` (content hashing)
- [ ] `0.5` Create directory structure per spec:
  - `docs/` (already exists)
  - `.coderun/skills/` (skill definitions)
  - `.coderun/config.toml` (default config)
- [ ] `0.6` Create `README.md` with build instructions
- [ ] `0.7` Verify `cargo build` compiles cleanly
- [ ] `0.8` Verify `cargo test` passes with empty test suite

**References:**
- Spec: `docs/01-architecture/ARCHITECTURE.md` (Technology Stack section)
- Spec: `docs/01-architecture/RUNTIME.md` (Configuration schema)

---

## Phase 1: Configuration System

**Goal:** Load, validate, and merge configuration from TOML files and environment variables.

- [ ] `1.1` Define `Config` struct in `crates/coderun-core/src/config.rs` matching spec schema:
  - `daemon` section (socket_path, max_concurrent, request_timeout_ms)
  - `database` section (path, max_connections)
  - `index` section (path, languages)
  - `knowledge` section (memory_enabled, memory_endpoint, max_knowledge_entries)
  - `skills` section (path, auto_discover, max_skills_per_request)
  - `context` section (max_tokens, max_files, max_lines_per_file, cache_order)
  - `model` section (default_tier, routing_enabled, max_tokens_response)
  - `routing` section (weights, thresholds, tier mappings)
  - `litellm` section (endpoint, timeout_ms, max_retries)
  - `rtk` section (enabled, max_output_tokens, compression_level)
  - `logging` section (level, file_path, max_size_mb, retention_days)
- [ ] `1.2` Implement config loading: user → project → environment merge order
- [ ] `1.3` Implement environment variable overrides (`CODERUN_*`)
- [ ] `1.4` Implement config validation with descriptive error messages
- [ ] `1.5` Implement `config show` and `config validate` CLI commands
- [ ] `1.6` Write unit tests for config loading and merging
- [ ] `1.7` Write unit tests for config validation

**References:**
- Spec: `docs/01-architecture/RUNTIME.md` (Configuration section, full schema)
- Spec: `docs/01-architecture/RUNTIME.md` (Environment Variables table)

---

## Phase 2: Error Types and Core Types

**Goal:** Define all shared types, error enums, and data structures used across modules.

- [ ] `2.1` Define `RuntimeError` enum in `crates/coderun-core/src/error.rs`:
  - `Timeout`, `InvalidRequest`, `IndexNotReady`, `ContextBuildFailed`
  - `ModelRoutingFailed`, `LlmUnavailable`, `RtkCompressionFailed`
  - `KnowledgeRetrievalFailed`, `SkillMatchFailed`, `DatabaseError`
  - `IndexError`, `EngramUnavailable`, `ConfigurationError`
- [ ] `2.2` Define `CorrelationId` newtype wrapper
- [ ] `2.3` Define IPC message types in `crates/coderun-core/src/ipc.rs`:
  - `AgentRequest` (correlation_id, hook_type, payload)
  - `AgentResponse` (correlation_id, hook_type, payload, latency_ms, error)
  - `HookType` enum (PreGeneration, PreToolCall)
  - `RequestPayload` enum (MessageRewrite, ToolOutput)
  - `ResponsePayload` enum (RewrittenMessage, CompressedOutput, OriginalPassthrough)
- [ ] `2.4` Define `ContextHints` struct (files_mentioned, language)
- [ ] `2.5` Define `TaskRequest` struct (message, session_id, context_hints)
- [ ] `2.6` Define `SearchResult`, `SearchResults` structs
- [ ] `2.7` Define `KnowledgeEntry` struct (id, category, key, value, confidence, source, relevance_score)
- [ ] `2.8` Define `SkillMatch` struct (skill_name, match_score, instructions, examples, constraints)
- [ ] `2.9` Define `RoutingDecision` struct (model, tier, scores, reasoning)
- [ ] `2.10` Define `ContextPack` struct (behavioral_skills, docs_context, code_context, token_usage)
- [ ] `2.11` Define `TokenUsage` struct (total_tokens, budget_remaining, by_source)
- [ ] `2.12` Define `CodeFile` struct (path, content, language, line_range, token_count)
- [ ] `2.13` Define `OutputType` enum (FileRead, SearchResult, ShellOutput, Other)
- [ ] `2.14` Implement `serde::Serialize` / `serde::Deserialize` on all types
- [ ] `2.15` Write unit tests for type serialization/deserialization

**References:**
- Spec: `docs/01-architecture/RUNTIME.md` (IPC Protocol, Message Format section)
- Spec: `docs/01-architecture/COMPONENTS.md` (all component output types)
- Spec: `docs/00-project/GLOSSARY.md` (term definitions)

---

## Phase 3: Event Bus

**Goal:** Implement the async-only observability event system.

- [ ] `3.1` Define `RuntimeEvent` enum in `crates/coderun-events/src/lib.rs`:
  - `ContextBuilt` { correlation_id, token_counts, file_count, latency_ms }
  - `SkillActivated` { correlation_id, skill_name, match_score }
  - `RepositoryUpdated` { files_indexed, symbols_extracted, duration_ms }
  - `ToolExecuted` { tool_name, original_tokens, compressed_tokens, ratio }
  - `ModelSelected` { correlation_id, model, tier, score, reasoning }
  - `ResponseGenerated` { correlation_id, hook_type, latency_ms, error }
  - `MemorySaved` { entry_id, namespace, key }
- [ ] `3.2` Implement `EventBus` struct using `tokio::sync::broadcast`
- [ ] `3.3` Implement `emit(event)` method (fire-and-forget)
- [ ] `3.4` Implement `subscribe()` method returning a receiver
- [ ] `3.5` Implement in-memory event buffer (last 1000 events)
- [ ] `3.6` Implement `get_recent_events(n)` for CLI inspection
- [ ] `3.7` Implement `get_events_by_correlation(id)` for replay
- [ ] `3.8` Write unit tests for emit/subscribe/buffer

**References:**
- Spec: `docs/01-architecture/COMPONENTS.md` (Module 8: Event Bus)
- Spec: `docs/01-architecture/DATA_FLOW.md` (Flow 9: Event Bus)

---

## Phase 4: Local Storage (SQLite)

**Goal:** Implement SQLite database with schema, migrations, and connection pooling.

- [ ] `4.1` Implement `Database` struct in `crates/coderun-storage/src/lib.rs`
- [ ] `4.2` Implement database initialization (open, WAL mode, connection pool)
- [ ] `4.3` Implement migration 001: create `files`, `symbols`, `token_usage`, `schema_migrations` tables
- [ ] `4.4` Implement migration runner (idempotent, ordered)
- [ ] `4.5` Implement `files` table operations:
  - `insert_file(path, hash, size, language)`
  - `update_file(id, hash, size)`
  - `delete_file(path)`
  - `get_all_files() -> Vec<(path, hash)>`
  - `get_file(path) -> Option<FileRecord>`
- [ ] `4.6` Implement `symbols` table operations:
  - `insert_symbol(file_id, name, kind, line_start, line_end, parent_id)`
  - `get_symbols_for_file(file_id) -> Vec<Symbol>`
  - `find_symbol(name) -> Vec<Symbol>`
- [ ] `4.7` Implement `token_usage` table operations:
  - `insert_usage(correlation_id, request_type, input_tokens, output_tokens, model, tier)`
  - `get_usage_stats() -> UsageStats`
- [ ] `4.8` Implement slow query logging (>100ms)
- [ ] `4.9` Write unit tests for all database operations
- [ ] `4.10` Write integration tests for migration idempotency

**References:**
- Spec: `docs/01-architecture/RUNTIME.md` (SQLite Schema section)
- Spec: `docs/01-architecture/COMPONENTS.md` (Module 9: Local Storage)

---

## Phase 5: Repository Intelligence

**Goal:** Implement incremental AST parsing, structural search, text search, and metadata storage.

- [ ] `5.1` Implement `RepositoryIntelligence` struct in `crates/coderun-repo-intel/src/lib.rs`
- [ ] `5.2` Implement directory walker with ignore patterns (.git, node_modules, target, etc.)
- [ ] `5.3` Implement language detection from file extensions
- [ ] `5.4` Integrate tree-sitter for AST parsing:
  - Create parser cache (one parser per language)
  - Parse source files into ASTs
  - Extract symbols (functions, classes, structs, enums, imports)
- [ ] `5.5` Implement incremental indexing:
  - Compute content hash (SHA-256) for each file
  - Compare with SQLite stored hashes
  - Parse only new/changed files
  - Remove deleted files from index
- [ ] `5.6` Implement `search_text(query, filters) -> SearchResults`:
  - Execute ripgrep search (via `grep_searcher` or `std::process::Command`)
  - Parse results with line numbers and context
  - Rank by relevance
- [ ] `5.7` Implement `search_structural(pattern) -> SearchResults`:
  - Execute ast-grep pattern matching
  - Parse results
- [ ] `5.8` Implement `search_fulltext(query) -> SearchResults`:
  - Search BM25/tantivy index
  - Return ranked results with snippets
- [ ] `5.9` Implement `get_file_content(path, line_range) -> String`
- [ ] `5.10` Implement `get_file_info(path) -> FileInfo`
- [ ] `5.11` Implement `get_symbol_info(query) -> SymbolInfo`
- [ ] `5.12` Emit `RepositoryUpdated` event on indexing complete
- [ ] `5.13` Log indexing progress every 100 files
- [ ] `5.14` Handle binary files gracefully (skip, log warning)
- [ ] `5.15` Write unit tests for symbol extraction
- [ ] `5.16` Write unit tests for incremental indexing (delta detection)
- [ ] `5.17` Write integration tests for full indexing pipeline
- [ ] `5.18` Write tests for text search and structural search

**References:**
- Spec: `docs/01-architecture/COMPONENTS.md` (Module 3: Repository Intelligence)
- Spec: `docs/01-architecture/DATA_FLOW.md` (Flow 1: Repository Indexing)
- Spec: `docs/01-architecture/RUNTIME.md` (Tantivy Index Schema)

---

## Phase 6: Skill Engine

**Goal:** Implement deterministic tag-based skill matching from community-format files.

- [ ] `6.1` Implement `SkillEngine` struct in `crates/coderun-skills/src/lib.rs`
- [ ] `6.2` Define `Skill` struct (name, tags, instructions, examples, constraints, description)
- [ ] `6.3` Implement Markdown skill parser:
  - Parse `# Name` as skill name
  - Parse `## Tags` section as comma-separated tags
  - Parse `## Instructions` section as instructions
  - Parse `## Examples` section as example list
  - Parse `## Constraints` section as constraint list
- [ ] `6.4` Implement TOML skill parser
- [ ] `6.5` Implement YAML skill parser
- [ ] `6.6` Implement skill validation (required fields: name, tags, instructions)
- [ ] `6.7` Implement `load_skills(directory) -> Vec<Skill>`:
  - Scan directory for .md, .toml, .yaml files
  - Parse each file
  - Validate schema
  - Return valid skills
- [ ] `6.8` Implement `match_skills(task_description, max_skills) -> Vec<SkillMatch>`:
  - Tokenize task description (lowercase, split on whitespace/punctuation)
  - For each skill: compute tag overlap score
  - Apply category bonus (1.2 if match, 1.0 otherwise)
  - Sort by score descending
  - Filter: score > 0.3
  - Take top N
- [ ] `6.9` Implement conflict detection (contradictory constraints)
- [ ] `6.10` Implement priority resolution (higher score wins)
- [ ] `6.11` Implement `reload_skills()` for hot-reload
- [ ] `6.12` Implement `list_skills() -> Vec<String>`
- [ ] `6.13` Write unit tests for skill parsing (all 3 formats)
- [ ] `6.14` Write unit tests for skill matching scoring
- [ ] `6.15` Write unit tests for conflict detection
- [ ] `6.16` Create sample skill files in `.coderun/skills/` for testing

**References:**
- Spec: `docs/01-architecture/COMPONENTS.md` (Module 5: Skill Engine)
- Spec: `docs/01-architecture/DATA_FLOW.md` (Flow 5: Skill Selection)

---

## Phase 7: Knowledge Hub

**Goal:** Implement unified knowledge storage and retrieval with BM25, FlashRank, and engram.

- [ ] `7.1` Implement `KnowledgeHub` struct in `crates/coderun-knowledge/src/lib.rs`
- [ ] `7.2` Implement SQLite knowledge table operations:
  - `store_knowledge(entry) -> id`
  - `get_knowledge(id) -> Option<KnowledgeEntry>`
  - `get_all_knowledge() -> Vec<KnowledgeEntry>`
  - `update_confidence(id, confidence)`
  - `decay_confidence(min_age_days, decay_amount)`
- [ ] `7.3` Implement BM25/tantivy knowledge index:
  - Create index with schema (id, category, key, value, confidence)
  - Add documents on store
  - Search documents on query
- [ ] `7.4` Implement `retrieve_knowledge(query, category_filter, max_results) -> Vec<KnowledgeEntry>`:
  - Search BM25 index
  - Filter by confidence >= 0.3
  - Rerank with FlashRank if available
  - Return top 10
- [ ] `7.5` Integrate FlashRank via `ort` (ONNX Runtime):
  - Load int8 quantized reranker model
  - Cache model in memory
  - Implement rerank(query, candidates) -> reranked_candidates
  - Fall back to BM25 ranking if FlashRank unavailable
- [ ] `7.6` Integrate engram for memory:
  - Implement `memory_search(query) -> Vec<MemoryEntry>` via HTTP API
  - Implement `memory_save(entry) -> Result` via HTTP API
  - Handle engram unreachable gracefully (continue without memory)
- [ ] `7.7` Implement `extract_knowledge(index_results)`:
  - Detect naming patterns (snake_case, camelCase, etc.)
  - Detect architectural patterns (controller-service-repo, etc.)
  - Detect domain terms
  - Store with confidence based on evidence strength
- [ ] `7.8` Implement confidence decay background task
- [ ] `7.9` Write unit tests for knowledge storage/retrieval
- [ ] `7.10` Write unit tests for skill matching via Knowledge Hub
- [ ] `7.11` Write integration tests for BM25 search + FlashRank rerank
- [ ] `7.12` Write tests for engram integration (mock HTTP)

**References:**
- Spec: `docs/01-architecture/COMPONENTS.md` (Module 4: Knowledge Hub)
- Spec: `docs/01-architecture/DATA_FLOW.md` (Flow 4: Knowledge Retrieval)

---

## Phase 8: Model Router

**Goal:** Implement heuristic complexity scoring and tier-based model selection.

- [ ] `8.1` Implement `ModelRouter` struct in `crates/coderun-router/src/lib.rs`
- [ ] `8.2` Implement `select_model(request) -> RoutingDecision`:
  - Compute structural complexity (file count, symbol count)
  - Compute semantic complexity (task length, technical terms, action verbs)
  - Compute scope (token count, knowledge entries, skills matched)
  - Apply weights from config
  - Compute final score
  - Map score to tier (fast/balanced/capable)
- [ ] `8.3` Implement technical term detection:
  - Define list: middleware, refactor, migrate, database, schema, API, etc.
  - Count occurrences in task description
- [ ] `8.4` Implement action verb detection:
  - Define list: implement, fix, add, remove, refactor, migrate, etc.
  - Count occurrences in task description
- [ ] `8.5` Implement tier-to-model mapping from config
- [ ] `8.6` Implement model override support (from request)
- [ ] `8.7` Implement fallback chain logic:
  - Try primary model
  - On failure: try next tier down
  - On all exhausted: return error
- [ ] `8.8` Emit `ModelSelected` event on completion
- [ ] `8.9` Log scoring breakdown at DEBUG level
- [ ] `8.10` Log final routing decision at INFO level
- [ ] `8.11` Write unit tests for complexity scoring
- [ ] `8.12` Write unit tests for tier mapping
- [ ] `8.13` Write unit tests for fallback logic

**References:**
- Spec: `docs/01-architecture/COMPONENTS.md` (Module 6: Model Router)
- Spec: `docs/01-architecture/DATA_FLOW.md` (Flow 7: Model Routing)

---

## Phase 9: Execution Optimizer

**Goal:** Implement tool-output compression via RTK with tee-on-failure pattern.

- [ ] `9.1` Implement `ExecutionOptimizer` struct in `crates/coderun-optimizer/src/lib.rs`
- [ ] `9.2` Implement `compress_output(tool_output) -> CompressedOutput`:
  - Detect output type (FileRead, SearchResult, ShellOutput, Other)
  - Dispatch to type-specific compressor
- [ ] `9.3` Implement file read compression:
  - Remove boilerplate/imports-only lines
  - Preserve function/class definitions
  - Preserve meaningful comments
  - Deduplicate patterns
  - Truncate to max lines
- [ ] `9.4` Implement search result compression:
  - Group by file
  - Keep top N per file
  - Remove duplicates
  - Preserve context lines
- [ ] `9.5` Implement shell output compression:
  - Remove ANSI escape codes
  - Remove repetitive progress indicators
  - Preserve errors/warnings
  - Preserve final output
- [ ] `9.6` Integrate RTK:
  - Call RTK binary/library for compression
  - Handle RTK failure with tee-on-failure
- [ ] `9.7` Implement tee-on-failure:
  - Save full output to log file on failure
  - Return original output (fail-open)
- [ ] `9.8` Integrate tiktoken-rs for token counting:
  - Count original tokens
  - Count compressed tokens
  - Return compression stats
- [ ] `9.9` Emit `ToolExecuted` event with compression stats
- [ ] `9.10` Log compression ratio at DEBUG level
- [ ] `9.11` Write unit tests for each compression type
- [ ] `9.12` Write unit tests for tee-on-failure
- [ ] `9.13` Write integration tests for RTK integration

**References:**
- Spec: `docs/01-architecture/COMPONENTS.md` (Module 7: Execution Optimizer)
- Spec: `docs/01-architecture/DATA_FLOW.md` (Flow 3: Pre-Tool Compression)

---

## Phase 10: Context Engine

**Goal:** Implement `BuildContext(task)` — the central pipeline that assembles Context Packs.

- [ ] `10.1` Implement `ContextEngine` struct in `crates/coderun-context/src/lib.rs`
- [ ] `10.2` Implement `build_context(task) -> Result<(ContextPack, RoutingDecision)>`:
  - Initialize token budget from config
  - Search code via Repository Intelligence
  - Retrieve knowledge via Knowledge Hub
  - Match skills via Knowledge Hub → Skill Engine
  - Assemble context pack with cache-aware ordering
  - Enforce token budget
  - Select model via Model Router
- [ ] `10.3` Implement cache-aware ordering:
  - Section 1: behavioral_skills (20% budget)
  - Section 2: docs_context (15% budget)
  - Frozen-prefix boundary
  - Section 3: code_context (55% budget)
- [ ] `10.4` Implement deduplication:
  - Compute SHA-256 hash of each content block
  - Check against session fingerprint (HashSet)
  - Skip if already sent
- [ ] `10.5` Implement token budget enforcement:
  - Track remaining budget per section
  - Truncate content that exceeds budget
  - Log budget usage at each stage
- [ ] `10.6` Implement YAML Context Pack serialization:
  - Serialize to YAML with three sections
  - Include token_usage metadata
- [ ] `10.7` Implement session fingerprint management:
  - Store in-memory per session
  - Clear on daemon restart
- [ ] `10.8` Implement token counting integration:
  - Use tiktoken-rs for accurate counting
  - Fallback to character-based estimation
- [ ] `10.9` Emit `ContextBuilt` event on completion
- [ ] `10.10` Log token usage at every stage
- [ ] `10.11` Write unit tests for budget enforcement
- [ ] `10.12` Write unit tests for deduplication
- [ ] `10.13` Write unit tests for YAML serialization
- [ ] `10.14` Write integration tests for full BuildContext pipeline
- [ ] `10.15` Write integration tests for fail-open behavior

**References:**
- Spec: `docs/01-architecture/COMPONENTS.md` (Module 2: Context Engine)
- Spec: `docs/01-architecture/DATA_FLOW.md` (Flow 2: Pre-Generation, Flow 6: Context Construction)
- Spec: `docs/01-architecture/REQUEST_LIFECYCLE.md` (Stage 5: Context Assembly)

---

## Phase 11: Adapter Layer (UDS Server)

**Goal:** Implement Unix domain socket server with MessagePack IPC and fail-open behavior.

- [ ] `11.1` Implement `AdapterLayer` struct in daemon crate
- [ ] `11.2` Implement UDS server:
  - Create Unix socket at configured path
  - Set socket permissions (owner read/write only)
  - Accept connections with tokio
- [ ] `11.3` Implement MessagePack decoding:
  - Read MessagePack-encoded `AgentRequest` from socket
  - Deserialize into typed struct
- [ ] `11.4` Implement request validation:
  - Validate hook_type is known
  - Validate payload matches hook_type
  - Return OriginalPassthrough on invalid
- [ ] `11.5` Implement correlation ID generation (`req_{uuid}`)
- [ ] `11.6` Implement PreGeneration handler:
  - Call Context Engine `build_context`
  - Wrap result in `RewrittenMessage` response
  - Apply 30s timeout
  - Return OriginalPassthrough on timeout/error
- [ ] `11.7` Implement PreToolCall handler:
  - Call Execution Optimizer `compress_output`
  - Wrap result in `CompressedOutput` response
  - Return OriginalPassthrough on error
- [ ] `11.8` Implement MessagePack encoding:
  - Serialize `AgentResponse` to MessagePack
  - Write to socket
- [ ] `11.9` Implement fail-open behavior:
  - On any error: return OriginalPassthrough
  - Log failure with correlation_id
  - Never block the agent
- [ ] `11.10` Implement 30s timeout for Claude Code hooks
- [ ] `11.11` Emit `ResponseGenerated` event
- [ ] `11.12` Log every request and response at INFO level
- [ ] `11.13` Write unit tests for request parsing
- [ ] `11.14` Write unit tests for fail-open behavior
- [ ] `11.15` Write integration tests for full UDS round-trip

**References:**
- Spec: `docs/01-architecture/COMPONENTS.md` (Module 1: Adapter Layer)
- Spec: `docs/01-architecture/RUNTIME.md` (IPC Protocol, Fail-Open Behavior)
- Spec: `docs/01-architecture/DATA_FLOW.md` (Flow 10: Fail-Open)

---

## Phase 12: Daemon Lifecycle

**Goal:** Implement daemon startup, shutdown, signal handling, and process management.

- [ ] `12.1` Implement `serve` command in CLI crate:
  - Parse CLI args with clap
  - Load config
  - Initialize logging
  - Open database
  - Start engram (if not running)
  - Open Tantivy index
  - Load skills
  - Start background indexing
  - Start UDS server
  - Wait for shutdown signal
- [ ] `12.2` Implement signal handling:
  - SIGINT/SIGTERM: graceful shutdown
  - SIGHUP: reload config
  - SIGUSR1: force re-index
- [ ] `12.3` Implement graceful shutdown:
  - Set shutdown flag (atomic bool)
  - Stop accepting new connections
  - Drain in-flight requests (max 30s)
  - Flush Tantivy index
  - Close SQLite connection
  - Flush logs
  - Remove socket file
  - Exit with code 0
- [ ] `12.4` Implement force shutdown (second signal):
  - Exit with code 1
- [ ] `12.5` Print startup banner with socket path
- [ ] `12.6` Write integration tests for startup/shutdown sequence

**References:**
- Spec: `docs/01-architecture/RUNTIME.md` (Process Lifecycle, Startup Sequence, Shutdown)
- Spec: `docs/01-architecture/COMPONENTS.md` (Module 10: CLI)

---

## Phase 13: CLI Commands

**Goal:** Implement all CLI commands for daemon management and inspection.

- [ ] `13.1` Implement `coderun serve` (Phase 12)
- [ ] `13.2` Implement `coderun init`:
  - Create `.coderun/` directory
  - Create default config
  - Create skills directory
  - Initialize database
  - Create Tantivy index
  - Run initial indexing
  - Print success message
- [ ] `13.3` Implement `coderun index`:
  - Connect to daemon
  - Trigger re-index
  - Print statistics
- [ ] `13.4` Implement `coderun preview <prompt>`:
  - Connect to daemon
  - Send PreGeneration request
  - Print formatted preview (skills, knowledge, code, tokens, model)
- [ ] `13.5` Implement `coderun replay <correlation_id>`:
  - Connect to daemon
  - Request event history
  - Print formatted replay
- [ ] `13.6` Implement `coderun status`:
  - Query database for stats
  - Print file count, symbol count, token usage
- [ ] `13.7` Implement `coderun skills list`:
  - Load skills from config path
  - Print skill names and tags
- [ ] `13.8` Implement `coderun skills validate`:
  - Load and validate all skills
  - Print validation results
- [ ] `13.9` Implement `coderun config show`:
  - Load and print effective config
- [ ] `13.10` Implement `coderun config validate`:
  - Load and validate config
  - Print validation results
- [ ] `13.11` Implement `coderun doctor`:
  - Check SQLite availability
  - Check Tantivy availability
  - Check tree-sitter grammars
  - Check engram connectivity
  - Check LiteLLM connectivity
  - Check RTK availability
  - Print health report
- [ ] `13.12` Write unit tests for each CLI command
- [ ] `13.13` Write integration tests for CLI → daemon communication

**References:**
- Spec: `docs/01-architecture/COMPONENTS.md` (Module 10: CLI)
- Spec: `docs/01-architecture/RUNTIME.md` (CLI Commands section)

---

## Phase 14: Agent Adapters (Tier 1)

**Goal:** Implement agent-specific adapter configurations for Tier 1 agents.

- [ ] `14.1` Research and document opencode hook API:
  - `chat.message` hook format and payload
  - `tool.execute.before` hook format and payload
  - Create `.opencode/hooks/` configuration files
- [ ] `14.2` Research and document Claude Code hook API:
  - `UserPromptSubmit` hook format and payload
  - `PreToolUse` hook format and payload
  - Create Claude Code hook configuration
- [ ] `14.3` Create adapter configuration for opencode:
  - Hook scripts that call daemon via UDS
  - Message format translation
- [ ] `14.4` Create adapter configuration for Claude Code:
  - Hook scripts that call daemon via UDS
  - Message format translation
- [ ] `14.5` Write integration tests for opencode adapter
- [ ] `14.6` Write integration tests for Claude Code adapter

**References:**
- Spec: `docs/00-project/PROJECT.md` (Tier 1 Agents table)
- Spec: `docs/01-architecture/COMPONENTS.md` (Module 1: Agent-Specific Adapters)

---

## Phase 15: Evaluation Framework

**Goal:** Set up Promptfoo evaluation for model routing accuracy and context quality.

- [ ] `15.1` Create `eval/` directory structure
- [ ] `15.2` Create Promptfoo configuration:
  - Custom provider hitting Context Engine API
  - Test cases for model routing accuracy
  - Test cases for context quality
- [ ] `15.3` Create evaluation dataset:
  - Sample tasks with expected model tier
  - Sample tasks with expected context files
- [ ] `15.4` Implement evaluation runner script
- [ ] `15.5` Run baseline evaluation
- [ ] `15.6` Document evaluation results and thresholds

**References:**
- Spec: `docs/00-project/PROJECT.md` (Success Criteria)
- Spec: `docs/01-architecture/COMPONENTS.md` (Module 10: CLI — eval commands)

---

## Phase 16: Hardening and Documentation

**Goal:** Final hardening, documentation, and packaging.

- [ ] `16.1` Add comprehensive error messages for all failure modes
- [ ] `16.2` Add structured logging to all modules
- [ ] `16.3` Add correlation ID propagation to all log entries
- [ ] `16.4` Add performance benchmarks:
  - Repository indexing time (target: <30s for 100k lines)
  - BuildContext latency (target: <5s typical, <30s hard limit)
  - Tool-output compression time (target: <20ms per tool)
- [ ] `16.5` Add memory usage benchmarks
- [ ] `16.6` Write README with:
  - Installation instructions
  - Quick start guide
  - Configuration reference
  - Architecture overview
- [ ] `16.7` Write CONTRIBUTING.md
- [ ] `16.8` Create release configuration (cargo-dist or similar)
- [ ] `16.9` Run full test suite
- [ ] `16.10` Run clippy with all lints
- [ ] `16.11` Run `cargo audit` for security vulnerabilities

**References:**
- Spec: `docs/00-project/PROJECT.md` (Success Criteria, timing targets)
- Spec: `docs/01-architecture/RUNTIME.md` (Logging, Error Handling)

---

## Dependency Graph

```
Phase 0 (Scaffolding)
    │
    ├──→ Phase 1 (Config) ──→ Phase 2 (Types)
    │                              │
    │                              ├──→ Phase 3 (Event Bus)
    │                              ├──→ Phase 4 (Storage)
    │                              │         │
    │                              │         └──→ Phase 5 (Repo Intel)
    │                              │                    │
    │                              │                    └──→ Phase 7 (Knowledge Hub)
    │                              │                              │
    │                              ├──→ Phase 6 (Skills) ────────┤
    │                              │                              │
    │                              ├──→ Phase 8 (Model Router) ──┤
    │                              │                              │
    │                              └──→ Phase 9 (Optimizer) ─────┤
    │                                                             │
    │                                                             └──→ Phase 10 (Context Engine)
    │                                                                           │
    │                                                                           └──→ Phase 11 (Adapter/UDS)
    │                                                                                         │
    │                                                                                         └──→ Phase 12 (Daemon)
    │                                                                                                   │
    │                                                                                                   └──→ Phase 13 (CLI)
    │
    └──→ Phase 14 (Agent Adapters)
              │
              └──→ Phase 15 (Evaluation)
                        │
                        └──→ Phase 16 (Hardening)
```

---

## File Reference Index

| Phase | Primary Crate | Key Files |
|-------|---------------|-----------|
| 0 | workspace | `Cargo.toml`, `crates/*/Cargo.toml` |
| 1 | `coderun-core` | `src/config.rs` |
| 2 | `coderun-core` | `src/error.rs`, `src/ipc.rs`, `src/types.rs` |
| 3 | `coderun-events` | `src/lib.rs` |
| 4 | `coderun-storage` | `src/lib.rs`, `src/migrations.rs` |
| 5 | `coderun-repo-intel` | `src/lib.rs`, `src/parser.rs`, `src/search.rs` |
| 6 | `coderun-skills` | `src/lib.rs`, `src/parser.rs`, `src/matcher.rs` |
| 7 | `coderun-knowledge` | `src/lib.rs`, `src/bm25.rs`, `src/rerank.rs`, `src/memory.rs` |
| 8 | `coderun-router` | `src/lib.rs`, `src/scorer.rs` |
| 9 | `coderun-optimizer` | `src/lib.rs`, `src/compress.rs`, `src/rtk.rs` |
| 10 | `coderun-context` | `src/lib.rs`, `src/pipeline.rs`, `src/yaml.rs` |
| 11 | `coderun-daemon` | `src/adapter.rs`, `src/server.rs` |
| 12 | `coderun-daemon` | `src/main.rs`, `src/lifecycle.rs` |
| 13 | `coderun-cli` | `src/main.rs`, `src/commands/*.rs` |
| 14 | `coderun-cli` | `src/commands/adapter_opencode.rs`, `src/commands/adapter_claude.rs` |
| 15 | `eval/` | `promptfoo.yaml`, `eval.sh` |
| 16 | workspace | `README.md`, `CONTRIBUTING.md` |

---

## Estimated Timeline

| Phase | Estimated Effort | Dependencies |
|-------|------------------|--------------|
| Phase 0: Scaffolding | 0.5 days | None |
| Phase 1: Configuration | 1 day | Phase 0 |
| Phase 2: Core Types | 1 day | Phase 0 |
| Phase 3: Event Bus | 0.5 days | Phase 2 |
| Phase 4: Storage | 1.5 days | Phase 2 |
| Phase 5: Repo Intelligence | 3 days | Phase 2, 4 |
| Phase 6: Skill Engine | 1.5 days | Phase 2 |
| Phase 7: Knowledge Hub | 2.5 days | Phase 2, 4 |
| Phase 8: Model Router | 1 day | Phase 2 |
| Phase 9: Execution Optimizer | 1.5 days | Phase 2 |
| Phase 10: Context Engine | 2.5 days | Phase 2, 5, 6, 7, 8 |
| Phase 11: Adapter Layer | 2 days | Phase 2, 10 |
| Phase 12: Daemon Lifecycle | 1.5 days | Phase 11 |
| Phase 13: CLI Commands | 2 days | Phase 12 |
| Phase 14: Agent Adapters | 2 days | Phase 11 |
| Phase 15: Evaluation | 1.5 days | Phase 10 |
| Phase 16: Hardening | 2 days | All phases |
| **Total** | **~26 days** | |
