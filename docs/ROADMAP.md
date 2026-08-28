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

## v0.5.0 — First-Class Tools ✅

**Released:** 2026-08-24
**Status:** Complete — Full plan `docs/V0_5_0_PLAN.md`, 15/16 tools first-class (all except optional LSP)
**Tests:** 166 passing — P0 deletions of fallback-as-primary (ast-grep/engram/FlashRank/graph/LiteLLM/RTK) + P1 Git/MkDocs/Promptfoo/analyzers

### What's Included (first-class, fallbacks only on Err/warn)

1. **ast-grep** `repo-intel/src/lib.rs:348` `search_structural()` `sg-core` gated first-class, `search_structural_fallback()` kept only as `Err` branch
2. **engram** `knowledge/src/lib.rs:248` `EngramClient::search_memory()` `2s timeout` `block_on_in_thread` primary, `db.search_memory()` LIKE only on `Err`
3. **FlashRank via ort** `knowledge/src/rerank.rs:1` `ort=2.0.0-rc.13` optional feature int8 `~/.coderun/models/flashrank.onnx` `try_flashrank_ort()`, `rerank_tfidf()` fallback with `WARN`, `enabled:true` default
4. **codebase-memory-mcp** `repo-intel/src/graph.rs:20` `try_codebase_memory_mcp()` `CODERUN_MCP_ENABLED` probe + `npx` primary, regex `extract_imports()` fallback
5. **LiteLLM** `router/src/lib.rs:222` `LiteLLMGateway::complete_with_fallback()` `capable→balanced→fast` `tracing::warn` cascade
6. **RTK** `optimizer/src/lib.rs:66` `RtkAdapter::compress()` vendored primary `WARN` on binary missing, `analyzers.rs` `run_gate()` `clippy` gate
7. **Git** `repo-intel/src/watcher.rs:7` `try_notify_git2_watcher()` `notify="6"`+`git2` primary, polling 5s fallback
8. **MkDocs** `repo-intel/src/lib.rs:290` walk `docs/**/*.md` → `store_knowledge(category="docs")` + tantivy on `index_repository()`
9. **Promptfoo** `eval/providers/context-quality.js:1` UDS `net.createConnection("/tmp/coderun.sock")`+`msgpack-lite` length-prefix primary, `mockContextEngine` fallback

## v0.6.0 — DBOS Required + Spec Compliance (Planned)

**Target:** Q3 2026 — Full plan `docs/V0_6_0_PLAN.md`
**Status:** Planned (docs-phase; no code yet)
**Tests:** 166 baseline → 180+ with async DBOS + extended-languages

### Goals (user-locked)

1. **DBOS = required + SQLite + native async** — `IWorkflowEngine` becomes `async_trait`, `workflow.enabled:true` default, sidecar `dbos-transact` `governedWorkflow` over `sqlite://~/.coderun/dbos.db` + Litestream replica (`DBOS_LITESTREAM_REPLICA_URL`), real `Hmac<Sha256>` (not `sha256(secret+body)`), fail-closed if DBOS down when enabled.
2. **Hook compat (OpenSpec)** — spec `chat.message` primary + `message.updated` shim with `hook_compat_total` metric + deprecation `WARN` (remove in v0.7.0).
3. **Languages = 111 via arborium** — all languages available by default via arborium bundle, no feature flags needed (`parser.rs` uses `arborium::get_language()`).
4. **Duplicate collapse** — single `SkillEngine` scorer, single `tier_to_model`, single `verify_hmac` (`hmac` crate), single UDS listener (`adapter.rs`), single `Database::open` + `LazyLock<Regex>` + shared `dirs::home`/`tokens::count`.

Spec: DBOS moves from separate product to required runtime; `Temporal` remains deleted; Tier2 stays README-only; vector/semantic deferred.

## v0.4.0 — Production Hardening + DBOS ✅

**Released:** 2026-08-24
**Status:** Complete — Full plan `docs/V0_4_0_PLAN.md`, decision DBOS Transact (not Temporal, `V0_4_0_PLAN.md:1.1`)
**Tests:** 165 passing (see `CHANGELOG.md:0.4.0`)

### What's Included

1. **DBOS Transact durable workflows** — `crates/coderun-workflow` (`DBOSWorkflowEngine: IWorkflowEngine`), `workflow/dbos/` Node sidecar (SQLite WAL + Litestream, `DBOS.workflow` + `DBOS.transaction` + `waitForSignal`), `005_audits.sql`, `WorkflowConfig` (`CODERUN_WORKFLOW_ENABLED`/`CODERUN_DBOS_SECRET`), CLI `workflow start/status/approve/list`, `doctor` 8th probe, fail-open if sidecar down
2. **Monitoring & Observability** — `GET /metrics` Prometheus (`daemon/src/metrics.rs` histogram p95, `Timer` RAII), Grafana `docs/dashboards/coderun.json`, `deploy/prometheus/alerts.yml` (p95>50ms, fail-open>5%)
3. **Security Hardening** — `ratelimit.rs` token-bucket 10/s burst 20 per `session_id` at `AdapterLayer`, HMAC-SHA256 `X-Coderun-Signature` for DBOS→daemon, structured `audits` table off hot path, `redact_secrets()` before outbound
4. **Distribution** — `Dockerfile` (rust:1.75-slim → distroless), `Formula/coderun.rb` (tap + launchd service), `docker-compose` with DBOS sidecar, `cargo-wix` MSI via `wix/main.wxs` scaffold
5. **Multi-Agent & Concurrency** — Cursor + Gemini CLI → Tier 1 `ADAPTERS.md:10`, `Mutex→RwLock` `adapter.rs:44`, per-session `KnowledgeHub` namespace isolation, soak 20×100

### DBOS vs Temporal

Chosen **DBOS Transact** (`V0_4_0_PLAN.md:1.1`): single-node SQLite+Litestream matches existing `rusqlite` WAL, no Temporal Go server + PostgreSQL cluster, local-first daemon-native. Temporal evaluated and documented as not chosen.

---

## Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) for development guidelines.

### Priority Areas

We welcome contributions in these areas:

1. **tree-sitter grammars** — 111 languages via arborium; add more by extending `LanguageId` enum and mapping to arborium grammar names
2. **Integration tests** — Test against real codebases
3. **Benchmarks** — Compare against baseline performance
4. **Documentation** — Improve guides and examples

---

## Success Metrics

| Metric | v0.1.0 | v0.2.0 | v0.3.0 | v0.4.0 | v0.5.0 | v0.6.0 (planned) |
|--------|--------|--------|--------|--------|--------|--------|
| Test coverage | 108 tests | 128 tests | 150+ | 165 | 166 | 180+ (async DBOS, extended-langs) |
| Languages supported | All (regex) | 4 (tree-sitter) | 10+ | 10+ | 10+ (ast-grep sg-core) | **111 (arborium)** |
| Search accuracy | ~70% | ~85% | 95%+ | 95%+ | 95%+ (FlashRank ort int8) | 95%+ (ort opt) |
| Latency (p95) | <100ms | <80ms | <50ms | <50ms (RwLock) | <50ms (first-class tools) | <50ms |
| Memory usage | <100MB | <150MB | <200MB | <200MB | <200MB (+ort <50MB when enabled) | <200MB (+ort <50MB) |
| Workflow | — | — | Noop | DBOS durable (optional) | DBOS durable (optional) | DBOS durable **required** SQLite+Litestream native async |
| Tool compliance | 58% | 58% | 90%+ | 93% | 15/16 first-class (all except optional LSP, no Temporal) | 16/16 (Tier2 README-only) |
