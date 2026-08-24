# v0.5.0 — First-Class Tools Plan

> **Purpose:** Make every row of the Tech Stack table a true first-class implementation. Fallbacks stay only inside `Err/timeout` branches with `warn!`; otherwise they are deleted. `Temporal` is excluded forever — `DBOS Transact` is the only external orchestrator (`V0_4_0_PLAN.md:1.1` locked).
>
> **Baseline:** `v0.4.0` `Cargo.toml:18` `0.4.0` `165 tests` — 9/16 stack rows already first-class (UDS/MessagePack, tree-sitter, ripgrep, tantivy, Custom Skill/Context/Router, DBOS). `V0_4_0` closed production hardening; `v0.5.0` closes correctness: no heuristic stubs pretending to be a tool.
>
> **Rule:** `Tools are first citizen for implementation, any other implementations can be deleted or left as a fallback.` (§1). If two impls exist, the named tool runs first; the other runs only on `Err`/`timeout`.

---

## 0. Executive Summary

| Module | Tool (table) | v0.4.0 ships | Gap class | v0.5.0 target |
|---|---|---|---|---|
| **Repo Intel — structural** | `ast-grep` | heuristic `tree-sitter node-type + regex` `repo-intel/src/lib.rs:352-419` | **P0 — crate missing** | `sg-core = "0.28"` (or `ast-grep` napi) first-class `search_structural()`; delete heuristic as primary |
| **Knowledge — memory** | `Gentleman-Programming/engram` `SQLite+FTS5` MCP-native | `EngramClient` exists `knowledge/src/engram.rs` but `knowledge/src/lib.rs:249` short-circuits to `db.search_memory()` `LIKE` | **P0 — never calls MCP** | `EngramClient::search_memory()` `POST /api/memory/search` with `2s timeout` is primary; `LIKE` only on `Err/timeout` |
| **Knowledge — retrieval** | `FlashRank (via ort)` | `RerankerConfig{enabled:false}` `knowledge/src/rerank.rs:21` `TF-IDF fallback` `53` | **P0 — ort missing** | `ort="2"` + int8 `rank-T5-flan` ONNX `~/.coderun/models/` first-class; `TF-IDF` only on model load fail |
| **Knowledge — graph** | `codebase-memory-mcp` | `graph.rs:4` `codebase-memory-mcp style, AST + regex fallback` but `extract_imports() 69-118` regex only | **P0 — MCP not called** | Call real `codebase-memory-mcp` via MCP stdio/HTTP; delete pure-regex path |
| **Model Router** | `LiteLLM` | `router/src/litellm.rs` client exists but `router/src/lib.rs:189` `ModelRouter` never calls it; `fallback_chain()` only helper | **P0 — gateway not wired** | `LiteLLMGateway: IModelGateway` first-class `POST /v1/chat/completions` with `capable→balanced→fast` cascade + `cost_usd` `003_graph.sql:15` |
| **Execution Optimizer** | `RTK` | `optimizer/src/lib.rs:68` `RtkAdapter::detect()` only if binary on `PATH`, else built-ins `80` | **P0 — not vendored** | Vendor `rtk` crate first-class; built-ins only on `Err` |
| **Repo Intel — Git trigger** | `Git` `notify`+`git2` | `watcher.rs:9` polling `HEAD`+`mtime` `5s` | **P1 — notify/git2 missing** | `notify="6"` + `git2` incremental diff first-class |
| **Documentation** | `MkDocs → Knowledge Hub` | `mkdocs.yml:1` site exists but ingest only comment `mkdocs.yml:40` | **P1 — not wired** | `coderun index` walks `docs/**/*.md` → `store_knowledge(category="docs")` + tantivy |
| **Offline eval** | `Promptfoo` | `eval/promptfooconfig.yaml:4` `file://providers/...` but provider still `file://` not UDS | **P1 — provider not UDS** | Custom provider hits `BuildContext` via UDS MessagePack |
| **Static analysis** | `Native per-language analyzers` | Not implemented | **P1 — gate missing** | `cargo clippy`/`eslint` gate after `DBOS.workflow` |
| **Repo Intel — LSP** | `(+ optional reused LSP)` | `repo-intel/src/lsp.rs:31` stub `WARN not yet wired` | **P2 — optional** | Keep optional; wire only if `CODERUN_LSP_ENABLED=true` |
| **External orchestrator** | `Temporal` | Excluded per your rule | — | Deleted; `DBOS Transact` already first-class `crates/coderun-workflow/src/dbos.rs:46` |

**Spec compliance:** 9/16 → 15/16 first-class (LSP stays optional). `Temporal` row removed.

---

## 1. P0 — Non-Negotiable Tool Fixes

### 1.1 `ast-grep` structural search

**Current:** `Cargo.toml` no `ast-grep`; `repo-intel/src/lib.rs:348` `Future: embed ast-grep-core directly` + `352 pub fn search_structural()` maps `function|fn|def` keywords to `tree-sitter` symbols, falls back to `393 strip ast-grep metavariables` regex.

**Plan:**
1. `Cargo.toml` add `sg-core = "0.28"` (or `ast-grep = { version="0.28", features=["napi"] }` per `V0_3_0_PLAN.md:1.4`). `repo-intel/Cargo.toml` add `sg-core`.
2. `repo-intel/src/lib.rs:352` delete keyword→symbol heuristic as primary. Implement:
   ```rust
   pub fn search_structural(&self, pattern: &str, lang: &str, max: usize) -> Result<SearchResults> {
     let sg = SGLang::from(lang); let matcher = sg.parse(pattern)?; // sg-core
     // walk via `ignore::WalkBuilder`, for each file run `matcher.match_all(&content)`
   }
   ```
   Wire as second pass after `search_text_ripgrep()`: `ContextEngine::search_code()` lexical → structural dedup `V0_3_0_PLAN.md:1.4`.
3. Keep regex fallback only inside `matcher.parse().err()` with `warn!(pattern, error)` and return `Err`.

**Acceptance:** `test_search_structural_finds_pattern` `lib.rs:1013` with real `ast-grep` query `fn $FUNC($$$) { $$$ }` finds `parser.rs` sample; `cargo test -p coderun-repo-intel -- search_structural` green; heuristic code deleted (grep `want_fn|want_class` absent except in test).

### 1.2 `engram` deterministic reads (MCP-native)

**Current:** `knowledge/src/engram.rs` `EngramClient` full HTTP client `POST /api/memory/*` with `timeout_ms`+`max_retries`, but `knowledge/src/lib.rs:249 try_engram_search()` comment `251 Real engram HTTP client would be …` and `256 short-circuit to local memory` — never calls `EngramClient`.

**Plan:**
1. `knowledge/src/lib.rs:32` add `engram_client: Option<EngramClient>` to `KnowledgeHub` constructed from `KnowledgeConfig{ memory_endpoint, timeout_ms: 2000 }`. Keep `config.memory_enabled` guard.
2. `retrieve_knowledge()` `lib.rs:221` delete `db.search_memory()` as primary. New pipeline:
   ```rust
   let engram_hits = tokio::time::timeout(2s, self.engram_client.search(&MemoryQuery{namespace:"default", query, max_results:3})).await;
   match engram_hits { Ok(Ok(hits)) => merge with confidence*1.1, Ok(Err(e))|Err(timeout) => { warn!(error=%e, "engram fail-open"); db.search_memory("default", query, 3) } }
   ```
   Since `KnowledgeHub` is sync and `BuildContext` is sync `context/src/lib.rs:70`, spawn via `tokio::task::block_in_place` or make `retrieve_knowledge` async via `context` bridge same as `workflow/src/dbos.rs:46 block_on_in_thread` pattern. Prefer async bridge in `ContextEngine::build_context()` (already `RwLock`).
3. Delete `try_engram_search` local-only shortcut; keep it only inside `Err` branch with `WARN`.

**Acceptance:** `test_engram_read_in_hot_path_with_timeout` `V0_3_0_PLAN.md:2.1` — wiremock `POST /api/memory/search` → 200 with `[{key, value}]` asserts `retrieve_knowledge("snake", None, 10)` contains `source="engram"` with `confidence 0.75` and `relevance 0.9`; when wiremock 500 or `timeout 2s` falls back to `LIKE` and logs `WARN` not `ERROR`.

### 1.3 `FlashRank (via ort)` int8 reranking

**Current:** No `ort` dep, `rerank.rs:21 enabled:false`, `48 if !enabled => return candidates`, `53 TF-IDF fallback` primary.

**Plan:**
1. `Cargo.toml` add `ort = { version="2", features=["onnx"] }`, `hf-hub`. `knowledge/Cargo.toml` add `ort`.
2. `rerank.rs:22 Default enabled:false → true` when model present; add `build.rs` or lazy download `flashrank rank-T5-flan int8` ONNX to `~/.coderun/models/flashrank.onnx` (quantized already per `V0_3_0_PLAN.md:2.1`). `Reranker::new()` tries `ort::Session::builder().with_intra_threads(1).commit_from_file(path)` then `session.run(inputs)`; on `Err` log `WARN "FlashRank model missing, TF-IDF fallback"` and keep current `compute_relevance_score()` `91-117` only there.
3. Adaptive `K` stays: `K = clamp(remaining_budget/avg_doc_tokens,5,20)` `knowledge/src/lib.rs:171` before `reranker.rerank()`.

**Acceptance:** `test_retrieve_uses_tantivy_plus_rerank` with `ort` feature on (`cargo test -p coderun-knowledge --features rerank-onnx`) asserts `Reranker` loads `flashrank.onnx` `session.run()` called; without model asserts `WARN` fallback; RAM `<50MB` quantized.

### 1.4 `codebase-memory-mcp` dependency graph

**Current:** `graph.rs:4 AST + regex fallback` but `extract_imports 69-118` regex `use crate::`, `import ... from`, dedup `sort/dedup`.

**Plan:**
1. Add `codebase-memory-mcp` as MCP tool: `crates/coderun-repo-intel/src/mcp.rs` (**new**) — stdio/HTTP client `POST /mcp/call {tool:"get_dependency_graph", path}` to `npx codebase-memory-mcp` if `CODERUN_MCP_ENABLED=true`, else local fallback. Or vendor its Rust AST graph builder if published; delete pure-regex path.
2. `graph.rs:20 build_from_files()` delete regex as primary: try `McpClient::get_graph(repo_root, files)` first (tree-sitter `import_statement` nodes per `V0_3_0_PLAN.md:1.4`); on `Err` with `WARN` use `extract_imports()` only as fallback. Populate `edges` table `003_graph.sql` already exists `from_path/to_path`.

**Acceptance:** `test_build_from_files_tmp` `graph.rs:146` asserts `graph.edge_count() >=1` via MCP/AST, not via `format!("{}.rs", dep_part)` regex.

### 1.5 `LiteLLM` gateway + fallback

**Current:** `router/src/litellm.rs` `LiteLLMClient` full but `router/src/lib.rs:189 IModelGateway for ModelRouter` only heuristic scorer; `fallback_chain()` helper exists but never drives HTTP; no `cost_usd` usage.

**Plan:**
1. Define `LiteLLMGateway: IModelGateway` `router/src/litellm.rs` implementing `select_model()` + `complete()` that `POST /v1/chat/completions` with `timeout_ms`, `Authorization: Bearer api_key`, and `capable→balanced→fast` cascade `router/src/lib.rs:223` per `COMPONENTS.md:762`. `ContextEngine::new()` takes `Box<dyn IModelGateway>` (local heuristic vs gateway) per `PRINCIPLES.md:148`.
2. On `5xx`/`reqwest::Error` iterate `fallback_chain(tier)` `lib.rs:223`, `info!(fallback attempt)` each, `reqwest::timeout` `config.litellm.timeout_ms`. Delegate `GET /cost` to fill `token_usage.cost_usd` `003_graph.sql:15`.
3. Keep `compute_*_complexity` `lib.rs:128-186` with `assert!(no reqwest/openai import)` test `V0_3_0_PLAN.md:2.2`.

**Acceptance:** Wiremock `LiteLLM` primary `500` → fallback `200` succeeds `test_fallback_chain_logs`; `SELECT cost_usd FROM token_usage` populated.

### 1.6 `RTK` adopted not built

**Current:** `optimizer/Cargo.toml:14 tiktoken-rs` only; `optimizer/src/lib.rs:68 RtkAdapter::detect()` `which rtk` binary scan, `80 fallback to compress_file_read/search/shell`.

**Plan:**
1. `Cargo.toml` add `rtk = { git="https://github.com/rtk-ai/rtk" }` or publish crate; `optimizer/src/rtk.rs:38 compress()` delete `Command::new("rtk compress")` binary shell-out as primary; instead call `rtk::compress(content, tool_name)` in-process. Keep `RtkAdapter::detect()` probe only to log `WARN` if feature disabled.
2. `compress_output()` `lib.rs:66` delete built-ins as primary: `if enabled { rtk.compress() } else` only on `Err` run `compress_*`. Tee-on-failure `115` `~/.coderun/logs/tool-failures/{correlation_id}.log` already `warn!`.

**Acceptance:** `test_rtk_compress_with_tee_on_failure` `rtk.rs:148` with `rtk` feature on asserts `compress()` via library `<10ms` `cargo bench`; without feature asserts `WARN` fallback.

---

## 2. P1 — High-Priority Integrations

### 2.1 `Git` trigger + `tantivy`/`BM25S` wiring (carry-over)

**Current:** `watcher.rs:9 polling + mtime 5s`, no `notify`/`git2`; `tantivy` BM25 already first-class `storage/src/tantivy_index.rs:66 MmapDirectory` and wired in `repo-intel/src/lib.rs:422 search_fulltext()` but `knowledge/src/lib.rs:170` lexical path still `LIKE` primary.

**Plan:**
1. `Cargo.toml` add `notify="6"` + `git2="0.19"`. `repo-intel/src/watcher.rs:9` delete polling as primary: `Watcher::spawn()` via `notify::RecommendedWatcher` + `git2::Repository::diff_tree_to_workdir` on `HEAD` move, calls `index_repository` incrementally `lib.rs:219` (hash+old_tree `parser.rs` incremental).
2. `knowledge/src/lib.rs:170` ensure `TantivyIndex::search()` over `knowledge` docs is primary `lib.rs:171-182` (already there after `v0.4.0`); delete `LIKE` as primary keep inside `tantivy Err`.

### 2.2 `MkDocs → Knowledge Hub`

**Current:** `mkdocs.yml:1` site + comment `mkdocs.yml:40 # Knowledge Hub ingestion: on coderun index, walk docs/**/*.md` but no code.

**Plan:** `repo-intel/src/lib.rs:219 index_repository()` after file loop walk `docs/**/*.md` → `KnowledgeHub::store_knowledge(category="docs", key=path, value=content, confidence 0.8, source="docs")` + `TantivyIndex::add_document()`. `V0_3_0_PLAN.md:3.3`.

### 2.3 `Promptfoo` UDS wiring

**Current:** `eval/promptfooconfig.yaml:4` `file://providers/context-quality.js` `2766` lines but still HTTP not UDS `V0_3_0_PLAN.md:3.4` custom provider claim unchecked.

**Plan:** `eval/providers/context-quality.js` delete `fetch http://127.0.0.1:9527/hook` as primary; use `net.createConnection("/tmp/coderun.sock")` + `rmp-serde` encode `AgentRequest` `adapter.rs:204` MessagePack length-prefix. Add scheduled `systemd timer` `cron` nightly `token_usage`+`events` dataset `eval/datasets/auto-$(date).yaml` + promotion gate `routing≥90%, context≥85%` `EVALUATION.md:212`.

### 2.4 `Native per-language analyzers`

**Current:** Not implemented; `SCOPE.md:51` says external but table expects `Quality gates`.

**Plan:** `crates/coderun-optimizer/src/analyzers.rs` (**new**) `post-DBOS` gate: `rust: cargo clippy -- -D warnings`, `ts: eslint`, `python: ruff`. Called from `workflow/dbos/src/workflows/governed.ts:step3` after `BuildContext`; failure blocks `Completed` and writes `audits` with `gate_failed`.

---

## 3. P2 — Packaging, Docs, Eval, Security

- **Interfaces** already `traits.rs:33 IContextBuilder/IModelGateway/IWorkflowEngine` from `V0_3_0_PLAN.md:3.1` — audit that no concrete structs leak outside `ContextEngine` hot path `context/src/lib.rs:70`.
- **Docs & Eval** + **Security & benchmarks** `V0_3_0_PLAN.md:3.5` carry-over if deferred: `benches/context_bench.rs` `criterion` `BuildContext p95 <50ms` `ROADMAP.md:160`.
- **LSP** `repo-intel/src/lsp.rs:31` stays optional behind `lsp` feature — no deletion needed.

---

## 4. P3 — Multi-Agent (already Tier 1 in v0.4.0)

`ADAPTERS.md:10` Cursor+Gemini already **✅ Supported (v0.4.0)** `ADAPTERS.md:10`. No v0.5.0 work unless adding `Continue`/`Copilot` beyond `tier2/README.md`.

---

## 5. Work Breakdown & Dependencies

```
Phase 0 (week 1) — Foundations (no dependencies)
  ☐ Add deps: sg-core, ort, notify, git2, rtk, codebase-memory-mcp client to Cargo.toml
  ☐ Keep DBOS first-class (no Temporal) — verify V0_4_0_PLAN.md:1.1 DBOS sidecar still health probes
  ☐ Delete grep of fallback-as-primary (assert fallback only in Err branches)

Phase 1 (week 1-2) — P0 ast-grep + FlashRank + RTK (parallel)
  ☐ ast-grep sg-core search_structural() + delete heuristic 1.1
  ☐ ort int8 load + adaptive K + delete TF-IDF primary 1.3
  ☐ vendor rtk crate + delete built-ins primary 1.6

Phase 2 (week 2-3) — P0 engram + graph + LiteLLM (depends Phase 0)
  ☐ engram MCP deterministic reads 2s timeout 1.2
  ☐ codebase-memory-mcp graph via MCP 1.4 + edges table
  ☐ LiteLLMGateway IModelGateway fallback cascade + cost_usd 1.5

Phase 3 (week 3-4) — P1 Git/MkDocs/Promptfoo/Analyzers (depends Phase 2)
  ☐ notify+git2 incremental watcher + tantivy WIRING check 2.1
  ☐ MkDocs walk docs/ → category=docs 2.2
  ☐ Promptfoo UDS custom provider + nightly job 2.3
  ☐ native analyzers gate post-DBOS 2.4

Phase 4 (week 4-5) — Release
  ☐ CHANGELOG, ROADMAP, version bump 0.5.0, clippy/audit clean, 180+ tests
```

**Critical path:** Phase 0 → Phase 1 → Phase 2 → Phase 4 (4 weeks).

---

## 6. File-Level Change Map

| File | Action |
|---|---|
| `Cargo.toml` | add `sg-core`/`ast-grep`, `ort`, `notify`, `git2`, `rtk`, `codebase-memory-mcp` client, `criterion` dev-dep |
| `crates/coderun-repo-intel/Cargo.toml` | add `sg-core`, `notify`, `git2` |
| `crates/coderun-repo-intel/src/lib.rs:352-419` | delete heuristic, wire `sg-core` `search_structural()` |
| `crates/coderun-repo-intel/src/watcher.rs:9` | delete polling, use `notify`+`git2` incremental `V0_3_0_PLAN.md:1.4` |
| `crates/coderun-repo-intel/src/mcp.rs` | **new** codebase-memory-mcp MCP client |
| `crates/coderun-repo-intel/src/graph.rs:69-118` | delete regex primary, call MCP/AST |
| `crates/coderun-repo-intel/src/lsp.rs` | keep optional |
| `crates/coderun-knowledge/Cargo.toml` | add `ort` |
| `crates/coderun-knowledge/src/rerank.rs:21-53` | delete TF-IDF primary, load `ort` int8 `build.rs` |
| `crates/coderun-knowledge/src/lib.rs:249-268` | delete `db.search_memory` primary, wire `EngramClient::search_memory()` `2s timeout` |
| `crates/coderun-knowledge/src/engram.rs` | keep HTTP client, add MCP-native path if `engram` exposes MCP |
| `crates/coderun-storage/src/lib.rs:448` | ensure `search_knowledge` not LIKE primary for retrieval (already tantivy in `knowledge`) |
| `crates/coderun-router/Cargo.toml` | add `reqwest` used; keep |
| `crates/coderun-router/src/litellm.rs` | add `LiteLLMGateway: IModelGateway` |
| `crates/coderun-router/src/lib.rs:189-223` | wire `fallback_chain()` HTTP cascade + cost |
| `crates/coderun-optimizer/Cargo.toml` | add `rtk` crate |
| `crates/coderun-optimizer/src/rtk.rs:38` | delete `Command::new("rtk compress")` binary primary, call `rtk` lib |
| `crates/coderun-optimizer/src/lib.rs:68-80` | delete built-ins primary |
| `crates/coderun-optimizer/src/analyzers.rs` | **new** native analyzers gate |
| `workflow/dbos/src/workflows/governed.ts` | call `analyzers` gate after BuildContext |
| `eval/providers/context-quality.js` | delete HTTP fallback primary, use UDS MessagePack |
| `mkdocs.yml:40` | implement `docs/` walk on `coderun index` |
| `benches/*.rs` | criterion p95 <50ms |
| `CHANGELOG.md`, `ROADMAP.md`, `Cargo.toml:18` | `0.5.0` entries |

---

## 7. Acceptance Checklist (release gate)

- [ ] `cargo test` ≥180 tests all green, `cargo clippy` 0 warnings, `cargo audit` 0 vulns
- [ ] `cargo bench` `BuildContext` p95 <50ms `ROADMAP.md:160`, `RTK` <10ms, `FlashRank ort` session <50ms RAM
- [ ] `coderun doctor` all critical ✓, `ast-grep`, `ort`, `engram`, `codebase-memory-mcp`, `LiteLLM`, `RTK`, `notify`/`git2` probes `✓` with `WARN` only on `Err`
- [ ] Grep `fallback-only` passes: `ast-grep` heuristic, `TF-IDF`, `db.search_memory` as primary, `compress_file_read` as primary, polling watcher — none found outside `Err/timeout` + `warn!`
- [ ] `coderun preview "add auth"` hits `engram` MCP + `tantivy`+`FlashRank ort` `rerank` + `ast-grep` structural + `graph` MCP edges
- [ ] `DBOS` still first-class: `workflow start --require-approval` → `awaiting_approval` → `approve` → `completed` + `audits` row, kill mid-`sleep` → WAL recovery same `workflow_id`
- [ ] No `Temporal` code — grep `temporal` `0` hits

---

## 8. What Is Explicitly NOT v0.5.0

Per `SCOPE.md:24-51` — must not be smuggled into v0.5.0:

- `Temporal` (deleted) — `DBOS Transact` is the only orchestrator.
- Vector/semantic recall — deferred; lexical `tantivy`+`FlashRank` stays.
- Multi-tenancy, dashboards with auth, model fine-tuning — deferred.

---

## 9. References

- Table: user-provided Tech Stack `Module→Tool→Role` (Adapter…External orchestrator)
- Baseline `v0.4.0` `Cargo.toml:18` `165 tests` `V0_4_0_PLAN.md:1.1` DBOS chosen
- Spec `claude.md.txt` §§1-7, `IMPLEMENTATION_PLAN.md:359`, `SCOPE.md:96`, `ARCHITECTURE.md:261` technology stack
