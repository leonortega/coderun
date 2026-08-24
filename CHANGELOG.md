# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

## [Unreleased]

### Planned for v0.3.0

- Integrate MkDocs for documentation
- Add performance benchmarks
- Add memory usage benchmarks

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
