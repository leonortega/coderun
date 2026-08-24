# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.0] - 2026-08-24 — First-Class Tools

### Added — First-Class Fixes (fallbacks kept only inside Err/warn)
- **ast-grep** `search_structural()` first-class `sg-core` gated `repo-intel/src/lib.rs:348` (heuristic deleted as primary, kept only in `search_structural_fallback()`)
- **engram** deterministic reads `knowledge/src/lib.rs:248` `EngramClient::search_memory()` `2s timeout` primary `block_on_in_thread`, `db.search_memory()` LIKE only on Err
- **FlashRank via ort** `knowledge/src/rerank.rs:1` `ort=2.0.0-rc.13` optional feature int8 `~/.coderun/models/flashrank.onnx` primary, `rerank_tfidf()` fallback with WARN, `Default enabled:true`
- **codebase-memory-mcp** `repo-intel/src/graph.rs:20` `try_codebase_memory_mcp()` via `npx` probe primary, regex `extract_imports()` fallback with WARN
- **LiteLLM** `LiteLLMGateway` `router/src/lib.rs:222` `complete_with_fallback()` `capable→balanced→fast` cascade + `cost_usd` `003_graph.sql:15`
- **RTK** vendored crate primary `optimizer/src/lib.rs:66` `RtkAdapter::compress()` first, built-ins `WARN` fallback, `tee` `~/.coderun/logs/tool-failures/`
- **Git** `notify+git2` `repo-intel/src/watcher.rs:7` `try_notify_git2_watcher()` primary `notify::RecommendedWatcher`+`git2::diff`, polling fallback
- **MkDocs** ingestion `repo-intel/src/lib.rs:290` walk `docs/**/*.md` → `store_knowledge(category="docs")` + tantivy on `index_repository()`
- **Promptfoo** UDS `eval/providers/context-quality.js:1` `net.createConnection("/tmp/coderun.sock")`+`msgpack-lite` length-prefix+`rmp-serde` primary, mock fallback
- **Native analyzers** `optimizer/src/analyzers.rs` `run_gate()` `cargo clippy -D warnings` post-DBOS gate
- **Workspace** `notify="6"`, `git2="0.19"`, `ort` per-crate optional `knowledge/Cargo.toml:20` feature `ort`

### Changed
- Version `0.4.0 → 0.5.0` `Cargo.toml:18`, 166 tests (15 knowledge due to `RerankerConfig` `enabled:true`)

## [0.4.0] - 2026-08-24 — Production Hardening + DBOS

### Added
- **DBOS Transact** durable workflows: `crates/coderun-workflow` (`DBOSWorkflowEngine: IWorkflowEngine`), Node sidecar `workflow/dbos/` (SQLite WAL + Litestream, approval gates, audit), `005_audits.sql` (`audits` + `workflows`), CLI `coderun workflow start/status/approve/list`, `WorkflowConfig` (`CODERUN_WORKFLOW_ENABLED`, `CODERUN_DBOS_SECRET`)
- **Observability:** `daemon/src/metrics.rs` Prometheus exposition (`GET /metrics` `coderun_requests_total`, `coderun_build_context_duration_seconds` histogram, `coderun_fail_open_total`), Grafana `docs/dashboards/coderun.json`, alerts `deploy/prometheus/alerts.yml`
- **Security:** `daemon/src/ratelimit.rs` token-bucket (10/s burst 20 per `session_id`), HMAC-SHA256 `X-Coderun-Signature`, structured audit log off hot path
- **Concurrency:** `AdapterLayer` `Mutex→RwLock` (`daemon/src/adapter.rs:44`), session-isolated memory namespace, soak test 20×100
- **Distribution:** `Dockerfile` (multi-stage distroless), `Formula/coderun.rb` (brew tap with service), `deploy/docker-compose.yml` wiring DBOS sidecar
- **Multi-agent:** Cursor + Gemini CLI promoted to Tier 1 `ADAPTERS.md:10` (RwLock session isolation proof), Continue promoted, Copilot/Factory Droid scaffolds
- **Benchmarks:** `benches/context_bench.rs` (`criterion` p95 <50ms target)

### Changed
- Version `0.3.0 → 0.4.0` `Cargo.toml:18`
- `http_server.rs:93` adds `/metrics`, `/workflow/*` routes; `doctor` now 8 probes (DBOS)

## [0.2.0] - 2026-08-24

### Added

#### External Tool Integration
- **tree-sitter** for AST parsing (Rust, Python, JavaScript, TypeScript)
- **ripgrep** (grep-searcher) for fast text search with .gitignore support
- **tantivy** for BM25 full-text indexing and search
- **engram** HTTP client for cross-session memory
- **FlashRank** reranker with TF-IDF fallback
- **LiteLLM** client for multi-provider model routing

#### New Modules
- `coderun-repo-intel/src/parser.rs` — tree-sitter AST parsing
- `coderun-storage/src/tantivy_index.rs` — tantivy BM25 index
- `coderun-knowledge/src/engram.rs` — engram HTTP client
- `coderun-knowledge/src/rerank.rs` — reranking module
- `coderun-router/src/litellm.rs` — LiteLLM client

### Changed
- Repository Intelligence now uses ripgrep for text search
- Repository Intelligence uses ignore crate for .gitignore support
- Symbol extraction uses tree-sitter AST when available, falls back to regex
- Storage module includes tantivy index for full-text search
- Knowledge Hub includes engram client for cross-session memory
- Router includes LiteLLM client for multi-provider routing

### Test Results
- **128 tests passing** (up from 108 in v0.1.0)
- 20 new tests for external tool integration

## [0.3.0] - 2026-08-24

### Added

#### P0 — Non-Negotiable Spec Compliance

- **UDS + MessagePack IPC + 30s fail-open** — dual transport: UDS/MessagePack primary, HTTP/JSON fallback (`crates/coderun-daemon/src/adapter.rs:70-193`, `crates/coderun-daemon/src/lifecycle.rs:158-280`, `crates/coderun-daemon/src/http_server.rs:129-145` secret redaction + input validation 100KB/1MB)
- **`tiktoken-rs` token counting** — local `cl100k_base` in `crates/coderun-context/src/lib.rs:388-413` and `crates/coderun-optimizer/src/lib.rs:264-302`, fallback heuristic only on load failure
- **Cache-aware pack hardening** — dedup via SHA-256 `session_fingerprints` `crates/coderun-context/src/lib.rs:70-118`, frozen-prefix `FROZEN PREFIX END` `lib.rs:153-170`, reversible truncation `~/.coderun/cache/originals/{hash}` `lib.rs:415-462`
- **Repository Intelligence completion** — `search_structural` (tree-sitter+regex) `crates/coderun-repo-intel/src/lib.rs:328-410`, `search_fulltext` (tantivy BM25) `lib.rs:412-453`, tantivy upsert in `index_repository` `lib.rs:176-320`, `graph.rs` dependency graph + `lsp.rs` optional + `watcher.rs` git polling `crates/coderun-repo-intel/src/*.rs`, migrations `003_graph.sql`+`004_events.sql`

#### P1 — Integrations

- **Knowledge Hub unification** — BM25→FlashRank adaptive K `crates/coderun-knowledge/src/lib.rs:160-230`, deterministic engram hot reads `lib.rs:232-252` (2s timeout, fail-open local)
- **LiteLLM gateway + fallback** — `IModelGateway` `crates/coderun-core/src/traits.rs:11-22` + `crates/coderun-router/src/lib.rs:329-365` `fallback_chain`, `cost_usd` in `003_graph.sql`
- **RTK adoption** — `crates/coderun-optimizer/src/rtk.rs:1-120` adapter (binary detection, tee-on-failure `~/.coderun/logs/tool-failures/`) + in-process fallback
- **Event bus + inspection** — real `coderun preview`/`replay` `crates/coderun-cli/src/main.rs:234-400`, SQLite spill `004_events.sql`, async-only invariant preserved

#### P2 — Packaging / Docs / Security

- **Interfaces as contracts** — `IContextBuilder`/`IModelGateway`/`IWorkflowEngine` `crates/coderun-core/src/traits.rs:1-51` + `secrets.rs` redaction before outbound calls
- **Packaging & hardening** — `coderun init --wizard`, expanded `coderun doctor` (9 probes incl. tiktoken+tantivy+redaction) `crates/coderun-cli/src/main.rs:489-640`, `coderun migrate --from claude|continue|cursor`
- **MkDocs → Knowledge Hub** — `mkdocs.yml` + ingest docs into Knowledge Hub (`category="docs"`)
- **Security** — input validation `http_server.rs:validate_input_len`, secrets redaction `crates/coderun-core/src/secrets.rs:1-35`, token-bucket stub (rate limit)
- **Benchmarks** — `benches/context_bench.rs` micro-benches (BuildContext p95, tiktoken 10KB, compression)
- **Multi-agent** — Cursor (`adapters/cursor/extension.ts`) + Gemini (`adapters/gemini/hooks.sh`) Tier 1, Tier 2 best-effort `adapters/tier2/README.md`, `docs/ADAPTERS.md` updated

### Test Results

- **147 tests passing** (up from 128 in v0.2.0)
- Zero warnings, zero clippy warnings
- Migrations 001-004 idempotent

### Changed

- Version bump 0.1.0 → 0.3.0 (`Cargo.toml:17`, `release.toml:39`)

## [0.1.0] - 2026-08-24

### Added

#### Core System
- Configuration system with TOML loading, env overrides, and validation
- Core types with error enums, IPC message types, and serde support
- Event Bus with broadcast channel and in-memory buffer
- Local Storage with SQLite, WAL mode, and migrations

#### Intelligence Components
- Repository Intelligence with incremental indexing and regex-based search
- Skill Engine with Markdown/TOML/YAML parsing and tag-based matching
- Knowledge Hub with SQLite storage, search, and pattern extraction
- Model Router with heuristic complexity scoring and tier selection
- Execution Optimizer with type-specific compression (file, search, shell)

#### Runtime
- Context Engine with pipeline assembly, cache ordering, and token budget
- Adapter Layer with HTTP server (JSON) and fail-open behavior
- Daemon Lifecycle with startup, shutdown, and signal handling
- CLI Commands (init, index, preview, status, skills, config, doctor)

#### Agent Integration
- OpenCode plugin (TypeScript) with pre-generation and pre-tool hooks
- Claude Code hooks (shell scripts) for UserPromptSubmit and PreToolUse

#### Evaluation
- Promptfoo evaluation framework with model routing and context quality tests
- 20 evaluation tests (11 model routing, 9 context quality)

#### Documentation
- README with full usage guide
- Architecture documentation
- Adapter integration guide
- Evaluation framework documentation
- Contributing guidelines
- Changelog

### Implementation Notes

v0.1.0 uses **custom, self-contained implementations** for all components:

| Component | Implementation |
|-----------|----------------|
| Repository Intelligence | Regex-based symbol extraction |
| Knowledge Hub | SQLite LIKE queries |
| Model Router | Heuristic scoring (no external API) |
| Execution Optimizer | Built-in compressors |
| Storage | SQLite with WAL mode |

This approach minimizes external dependencies and ensures the project builds and runs without Python/Node at runtime.

### Test Results

- **108 unit tests passing** across 11 crates
- **Zero compiler warnings**
- **Zero clippy warnings**
- **Zero security vulnerabilities** (cargo audit)
- **20 evaluation tests** (100% pass rate)

### Metrics

| Metric | Value |
|--------|-------|
| Crates | 11 |
| Lines of code | ~5,000+ |
| Test coverage | 108 tests |
| Build time | <30s (release) |
| Binary size | ~6MB |
| Startup time | <100ms |
| Indexing speed | ~300 files/sec |

### Known Limitations

1. **Regex-based extraction** — Misses nested structures and complex syntax
2. **SQLite LIKE queries** — Don't scale for large codebases
3. **Heuristic routing** — Can't route to multiple providers
4. **No cross-session memory** — Each session starts fresh
5. **No structural search** — Can't find similar code patterns

### Security

- Passed cargo audit with zero vulnerabilities
- No external network calls (except optional engram/LiteLLM)
- Input validation on all endpoints
- Timeout protection on all operations

[0.1.0]: https://github.com/leonortega/coderun/releases/tag/v0.1.0
