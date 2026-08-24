# Coderun Roadmap

## v0.1.0 — Initial Release ✅

**Released:** August 24, 2026
**Status:** Complete

### What's Included

- 11 Rust crates in workspace
- 108 unit tests passing
- HTTP server for agent integration
- OpenCode and Claude Code adapters
- Promptfoo evaluation framework

### Implementation Notes

v0.1.0 uses **custom, self-contained implementations** for all components:

| Component | v0.1.0 Implementation |
|-----------|----------------------|
| Repository Intelligence | Regex-based symbol extraction |
| Knowledge Hub | SQLite LIKE queries |
| Model Router | Heuristic scoring (no external API) |
| Execution Optimizer | Built-in compressors |
| Storage | SQLite with WAL mode |

---

## v0.2.0 — External Tool Integration ✅

**Released:** August 24, 2026
**Status:** Complete
**Tests:** 128 passing

### What's Included

- tree-sitter for AST parsing (Rust, Python, JavaScript, TypeScript)
- ripgrep for fast text search with .gitignore support
- tantivy for BM25 full-text indexing and search
- engram HTTP client for cross-session memory
- FlashRank reranker with TF-IDF fallback
- LiteLLM client for multi-provider model routing

### New Modules

| Module | Purpose |
|--------|---------|
| `coderun-repo-intel/src/parser.rs` | tree-sitter AST parsing |
| `coderun-storage/src/tantivy_index.rs` | tantivy BM25 index |
| `coderun-knowledge/src/engram.rs` | engram HTTP client |
| `coderun-knowledge/src/rerank.rs` | reranking module |
| `coderun-router/src/litellm.rs` | LiteLLM client |

### Test Results

- **128 tests passing** (up from 108 in v0.1.0)
- 20 new tests for external tool integration
- Zero clippy warnings

---

## v0.3.0 — Advanced Features

**Target:** Q1 2027
**Status:** Planned

### Planned Features

1. **Integrate MkDocs**
   - Generate API documentation from Rust doc comments
   - Build searchable documentation site
   - Deploy to GitHub Pages

2. **Structural Search with ast-grep**
   - Pattern matching for code structures
   - Find similar code patterns across codebase

3. **Dependency Graph Analysis**
   - AST-derived code graph
   - Impact analysis for changes

4. **Static Analysis Integration**
   - Per-language analyzers (clippy, pylint, etc.)
   - Quality gates on generated artifacts

5. **Performance Benchmarks**
   - Repository indexing time
   - BuildContext latency
   - Tool-output compression time

6. **Memory Usage Benchmarks**
   - Track memory consumption
   - Optimize hot paths

---

## v0.4.0 — Production Hardening

**Target:** Q2 2027
**Status:** Planned

### Planned Features

1. **Monitoring & Observability**
   - Prometheus metrics
   - Distributed tracing
   - Health dashboards

2. **Security Hardening**
   - Input validation
   - Rate limiting
   - Audit logging

3. **Distribution**
   - Homebrew formula
   - Docker image
   - Windows installer

4. **Multi-Agent Support**
   - Concurrent agent sessions
   - Session isolation and memory

5. **Temporal Integration (Optional)**
   - Workflow orchestration for complex tasks
   - Approval gates and audit logging

---

## Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) for development guidelines.

### Priority Areas

We welcome contributions in these areas:

1. **tree-sitter grammars** — Add support for more languages (Go, Java, C++)
2. **Integration tests** — Test against real codebases
3. **Benchmarks** — Compare against baseline performance
4. **Documentation** — Improve guides and examples

---

## Success Metrics

| Metric | v0.1.0 | v0.2.0 | v0.3.0 Target |
|--------|--------|--------|---------------|
| Test coverage | 108 tests | 128 tests | 150+ tests |
| Languages supported | All (regex) | 4 (tree-sitter) | 10+ |
| Search accuracy | ~70% | ~85% | 95%+ |
| Latency (p95) | <100ms | <80ms | <50ms |
| Memory usage | <100MB | <150MB | <200MB |
