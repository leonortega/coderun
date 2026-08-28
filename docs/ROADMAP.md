# Coderun Roadmap

## Current Version: v0.8.0

**Released:** August 28, 2026
**Status:** Active
**Crates:** 12 workspace members (+ `coderun-workflow` excluded, in `future/workflow/`)

---

## Release History

### v0.1.0 — Initial Release ✅

**Released:** August 24, 2026

- 11 Rust crates, 108 unit tests
- HTTP server for agent integration
- OpenCode and Claude Code adapters
- Promptfoo evaluation framework
- Custom implementations for all components (regex, SQLite LIKE, heuristic scoring, built-in compressors)

### v0.2.0 — External Tool Integration ✅

**Released:** August 24, 2026 | **Tests:** 128

- tree-sitter AST parsing (Rust, Python, JavaScript, TypeScript)
- ripgrep text search with .gitignore support
- tantivy BM25 full-text indexing and search
- *engram HTTP client for cross-session memory — removed in v0.7.6 (see ENGRAM_CBM_REMOVAL.md, replaced by SQLite+tantivy local)*
- *FlashRank reranker with TF-IDF fallback — removed in v0.7.6 (see FLASHRANK_REMOVAL.md, SQLite+tantivy local)*
- LiteLLM client for multi-provider model routing

### v0.3.0 — Spec-Compliance ✅

**Released:** August 24, 2026 | **Tests:** 147

- UDS + MessagePack IPC with 30s fail-open
- tiktoken-rs local token counting (cl100k_base)
- Cache-aware pack: SHA-256 dedup, frozen-prefix boundary, reversible compression
- Repository Intelligence: structural search, full-text tantivy, dependency graph, git watcher
- Knowledge Hub: BM25 → FlashRank pipeline *(FlashRank removed in v0.7.6 — see FLASHRANK_REMOVAL.md, reranker is passthrough)*, *engram deterministic reads — removed (see ENGRAM_CBM_REMOVAL.md)*
- LiteLLM gateway with fallback chains
- RTK adoption with tee-on-failure
- Event bus + `coderun preview`/`replay` CLI
- Interface contracts: `IContextBuilder`, `IModelGateway`, `IWorkflowEngine`

### v0.4.0 — Production Hardening + DBOS ✅

**Released:** August 24, 2026 | **Tests:** 165

- DBOS Transact durable workflows (optional sidecar)
- Prometheus metrics (`GET /metrics`), Grafana dashboard
- Token-bucket rate limiting, HMAC-SHA256 request signing
- Distribution: Dockerfile, Homebrew Formula, docker-compose
- Multi-agent: Cursor + Gemini CLI promoted to Tier 1
- Concurrency: `RwLock<ContextEngine>`, per-session isolation, soak 20×100
- Benchmarks: `criterion` context bench (p95 <50ms target)

### v0.5.0 — First-Class Tools ✅

**Released:** August 24, 2026 | **Tests:** 166

- ast-grep (`sg-core`) first-class, heuristic deleted as primary
- *engram deterministic reads — removed (see ENGRAM_CBM_REMOVAL.md, SQLite local)*
- *FlashRank via `ort` int8, TF-IDF fallback only on model load fail — removed in v0.7.6 (see FLASHRANK_REMOVAL.md, offline eval only)*
- *codebase-memory-mcp probe — removed (see ENGRAM_CBM_REMOVAL.md, local AST+regex)*
- LiteLLM `IModelGateway` with `capable→balanced→fast` cascade
- RTK vendored crate primary, built-in compressors fallback only
- Git `notify`+`git2` incremental watcher, polling fallback only
- MkDocs → Knowledge Hub ingestion (`category="docs"`)
- Promptfoo UDS custom provider

### v0.6.0 — DBOS Required + Spec Compliance ✅

**Released:** August 24, 2026 | **Tests:** 193

- DBOS promoted to **required** (`enabled:true` default), native async `#[async_trait]`
- Real `Hmac<Sha256>` (was `sha256(secret+body)`)
- OpenSpec hook compat: `chat.message` primary + `message.updated` shim
- Extended languages feature (Go, Java, C, C++ behind `extended-languages` flag)
- Duplicate collapse: single skill scorer, single HMAC, single UDS listener

### v0.7.0 — Single-Command Bootstrap ✅

**Released:** August 25, 2026

- `coderun init` full bootstrap: scaffold → discovery → indexing → knowledge → engram → profile
- Repository discovery: language census by extension, framework detection from manifests
- Knowledge seeding at init: README + ADRs → `store_knowledge(category="docs")`
- *Engram bootstrap — removed (see ENGRAM_CBM_REMOVAL.md)*
- Repository profile artifact: `.coderun/profile.json`
- ast-grep via npm prebuilt, RTK prebuilt binary installers

### v0.7.5 ✅

**Released:** August 27, 2026

- 12 workspace crates (`coderun-workflow` excluded to `future/workflow/`)
- Event persistence removed from hot path (in-memory ring buffer only)
- DBOS isolated to `future/workflow/` — not required for v1
- `coderun doctor` works without DBOS

### v0.8.0 — Current ✅

**Released:** August 28, 2026

- Minimal v1 stack: Tree-sitter + Tantivy + SQLite(metadata) + Git (RTK/LiteLLM optional)
- Removed MkDocs ingestion (docs remain as plain markdown), Knowledge Hub collapsed to Repository Context, FlashRank/Engram/cbm already removed
- Retrieval latency: global INDEX_CACHE + cached_reader, avoid STORED content fetch for candidates (CODERUN_CANDIDATE_K sweep), graph gated for doc_count>5000
- `coderun init --community-skills` opt-in (default OFF)
- `docs/00-project/V1_PLAN.md` + `V1_MINIMAL_STACK_PLAN.md` + `ENGRAM_CBM_REMOVAL.md`/`FLASHRANK_REMOVAL.md` ADRs

---

## Current Architecture

See [Architecture](01-architecture/ARCHITECTURE.md) and [Components](01-architecture/COMPONENTS.md) for the full specification.

### Core Pipeline

```
Coding Agent → Adapter Layer (UDS/MessagePack) → Context Engine → Context Pack (YAML)
                ↓                                      ↓              ↓
          Execution Optimizer              Repository Intel    Model Router → LiteLLM
          (tool output compression)        Knowledge Hub       → Model Provider
                                            Skill Engine
```

### Workspace Crates

| Crate | Purpose |
|-------|---------|
| `coderun-core` | Shared types, config, IPC, traits |
| `coderun-daemon` | HTTP/UDS server, adapter, metrics |
| `coderun-cli` | CLI commands (init, index, preview, doctor, etc.) |
| `coderun-context` | BuildContext pipeline, token budgeting |
| `coderun-repo-intel` | tree-sitter, ripgrep, tantivy, graph, watcher |
| `coderun-knowledge` | Knowledge Hub, retrieval (engram removed) |
| `coderun-skills` | Skill matching engine |
| `coderun-router` | Model routing, LiteLLM gateway |
| `coderun-optimizer` | RTK compression, tool output optimization |
| `coderun-events` | Event bus (in-memory ring buffer) |
| `coderun-storage` | SQLite + tantivy persistence |
| `coderun-bench` | Criterion benchmarks |

### External Integrations

| Tool | Role | Status |
|------|------|--------|
| tree-sitter | AST parsing | ✅ First-class |
| ripgrep | Text search | ✅ First-class |
| ast-grep (`sg-core`) | Structural search | ✅ First-class |
| tantivy | BM25 full-text index | ✅ First-class |
| tiktoken-rs | Local token counting | ✅ First-class |
| RTK | Tool output compression | ✅ First-class (binary) |
| LiteLLM | Model gateway | ✅ First-class |
| engram | Cross-session memory | ❌ Removed — see `01-architecture/ENGRAM_CBM_REMOVAL.md` |
| codebase-memory-mcp | Dependency graph | ❌ Removed — see `01-architecture/ENGRAM_CBM_REMOVAL.md` |

---

## Agent Support

| Agent | Tier | Status |
|-------|------|--------|
| OpenCode | 1 | ✅ Canonical integration |
| Claude Code | 1 | ✅ Supported |
| Cursor | 1 | ✅ Supported |
| Gemini CLI | 1 | ✅ Supported |
| Continue | 1 | ✅ Supported |
| Copilot / Factory Droid / OpenClaw / Pi | 2 | ⏳ Scaffold |
| Codex / Windsurf / Cline / Kilo / Antigravity / Kimi | 2 | ⚠️ Best-effort |

---

## Future Plans

### v0.8.0 — Retrieval Quality

- Evaluation-driven retrieval improvements
- Recall@5 target: 0.4 on 50-task eval dataset (current: ~0.29)
- Index-time representation improvements (PascalCase splitting, symbol fields)
- Query sanitization for code-aware tokenization

### v2.0 — Platform Extensions

- Multi-repository support
- Conversation memory (engram removed — deferred)
- Plugin system
- Web dashboard
- Distributed deployment

---

## Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) for development guidelines.

### Priority Areas

1. **Retrieval quality** — Improve Recall@5 via index-time representation
2. **Integration tests** — Test against real codebases (eShopOnWeb, etc.)
3. **Benchmarks** — Context build p95, indexing throughput, RTK compression
4. **Documentation** — Guides, examples, architecture clarification

---

## Success Metrics

| Metric | v0.1.0 | v0.3.0 | v0.5.0 | v0.7.5 (current) |
|--------|--------|--------|--------|-------------------|
| Test coverage | 108 | 147 | 166 | ~184 |
| Languages | 4 (tree-sitter) | 10+ | 111 (arborium) | 111 (arborium) |
| Latency (p95) | <100ms | <50ms | <50ms | <50ms |
| Workflow | — | Noop | DBOS (optional) | DBOS → `future/` |
| Tool compliance | 58% | 90%+ | 15/16 | 15/16 (LSP optional) |
