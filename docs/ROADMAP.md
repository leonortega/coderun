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

## v0.3.0 — Spec-Compliance Release (Full Implementation Plan: `docs/V0_3_0_PLAN.md`)

**Target:** Q1 2027
**Status:** Planned — spec audit complete, 14 gaps triaged (P0-P3)
**Spec compliance:** 58% → 90%+

> This release closes every P0/P1 gap from the spec audit. See `docs/V0_3_0_PLAN.md:0-9` for gap matrix, file-level map, and acceptance gates.

### P0 — Non-Negotiable (blocks release)

1. **UDS + MessagePack IPC + 30s fail-open** (`docs/V0_3_0_PLAN.md:1.1`)
   - `crates/coderun-daemon/src/uds_server.rs` (UnixListener + `rmp-serde` + `tokio::time::timeout(30s)`)
   - Extract shared handler `crates/coderun-daemon/src/handler.rs`; keep HTTP behind `--http` flag for Windows fallback
   - Migrate `.opencode/plugins/coderun.ts` + `.claude/hooks/*.sh` to UDS; spike-validate `chat.message` rewrite <1s, fail-open provable
2. **`tiktoken-rs` token counting** (`docs/V0_3_0_PLAN.md:1.2`)
   - Replace `char/4` heuristics in `crates/coderun-context/src/lib.rs:309-343` and `crates/coderun-optimizer/src/lib.rs:266-273` with compiled tokenizer (`cl100k_base`), <2ms/10KB
3. **Cache-aware pack hardening** (`docs/V0_3_0_PLAN.md:1.3`)
   - Wire `session_fingerprints` `crates/coderun-context/src/lib.rs:46` into `assemble_context_pack` (SHA-256 dedup), emit `__frozen_prefix_end` boundary, reversible compression via `~/.coderun/cache/originals/{hash}`
4. **Repository Intelligence completion** (`docs/V0_3_0_PLAN.md:1.4`)
   - `ast-grep` structural search (`sg-core`), tantivy full-text wiring, git `notify`+`git2` incremental watcher, tree-sitter incremental reparse, `graph.rs` dependency edges (`003_graph.sql`), optional `lsp.rs` behind `lsp` feature

### P1 — High-Priority Integrations

5. **Knowledge Hub unification** (`docs/V0_3_0_PLAN.md:2.1`)
   - BM25→FlashRank pipeline (tantivy top 20 → `crates/coderun-knowledge/src/rerank.rs:42-88` with adaptive K + `ort` int8 model), engram deterministic reads in hot path (2s timeout, fail-open to local LIKE)
6. **LiteLLM gateway + fallback** (`docs/V0_3_0_PLAN.md:2.2`)
   - Define `IModelGateway` `crates/coderun-core/src/traits.rs`, implement fallback `capable→balanced→fast`, add `cost_usd` to `token_usage`, wire `crates/coderun-router/src/litellm.rs`
7. **RTK adoption** (`docs/V0_3_0_PLAN.md:2.3`)
   - Vendor `rtk` crate (`crates/coderun-optimizer/src/rtk.rs`), tee-on-failure to `~/.coderun/logs/tool-failures/`, honest savings reporting
8. **Event bus + inspection CLI** (`docs/V0_3_0_PLAN.md:2.4`)
   - Real `coderun preview <prompt>` + `coderun replay <correlation_id>` (ring buffer → SQLite `004_events.sql`), prove async-only invariant (never in `BuildContext` hot path)

### P2 — Packaging, Docs, Eval, Security

9. **Interfaces as contracts** (`docs/V0_3_0_PLAN.md:3.1`) — `IContextBuilder`/`IModelGateway`/`IWorkflowEngine` in `crates/coderun-core/src/traits.rs`
10. **Packaging & hardening** (`docs/V0_3_0_PLAN.md:3.2`) — `coderun init --wizard`, expanded `coderun doctor` (7 probes), `coderun migrate --from claude|Continue|cursor`, secrets redaction before outbound calls
11. **MkDocs → Knowledge Hub** (`docs/V0_3_0_PLAN.md:3.3`) — `mkdocs.yml` + ingest `docs/` into Knowledge Hub (`category="docs"`) + `gh-pages` deploy
12. **Offline eval gating** (`docs/V0_3_0_PLAN.md:3.4`) — promptfoo custom provider over UDS, nightly scheduled job over real `token_usage`/`events` logs, promotion gate (routing ≥90%, context ≥85%)
13. **Security & benchmarks** (`docs/V0_3_0_PLAN.md:3.5`) — input validation (message ≤100KB), token-bucket per `session_id`, `criterion` benches (`BuildContext` p95 <50ms, indexing ≥300 files/s, RTK <10ms)

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
