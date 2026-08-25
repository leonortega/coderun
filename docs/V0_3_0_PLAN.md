# v0.3.0 — Spec-Compliance Implementation Plan

> **Purpose:** Close every gap between the consolidated spec (`claude.md.txt` §§1-7) and the shipped codebase (`v0.2.0`, 128 tests, `README.md:432-450`). This doc is the single source of truth for the next release. Each item cites spec intent, current code location, gap severity, and concrete acceptance criteria.
>
> **Baseline:** `CHANGELOG.md:8-36` (v0.2.0) + `IMPLEMENTATION_PLAN.md:9-46` + `ROADMAP.md:63-95` — 6/7 external tools report "integrated" but audit shows partial integration. MkDocs is the only explicitly deferred item; at least 14 other spec-mandated behaviors remain incomplete or stubbed.

---

## 0. Executive Summary

| Dimension | Spec requires | v0.2.0 ships | Gap class |
|---|---|---|---|
| **IPC** (§2, §3 Adapter, §5 PRINCIPLES.md:22-30) | UDS + MessagePack, 30s hard fail-open, low-single-digit latency target | HTTP/JSON on TCP 9527 (`crates/coderun-daemon/src/http_server.rs:100-118`), `crates/coderun-core/src/ipc.rs:5-83` JSON only | **P0 — transport non-compliant** |
| **Token counting** (§3 Context Engine, §4 table) | `tiktoken-rs` compiled tokenizer, no model API round-trip | `char/4` heuristic in 3 places (`coderun-context/src/lib.rs:309-343`, `coderun-optimizer/src/lib.rs:266-273`, `coderun-context` clone) | **P0** |
| **Cache ordering** (§2 cache-aware constraint) | `skills → docs → code` + explicit `frozen-prefix` boundary + reversible compression | Ordering present (`coderun-context/src/lib.rs:219-252`) but no boundary marker, no reversible-fetch, fingerprint `session_fingerprints` allocated `lib.rs:46` never read in `assemble_context_pack` | **P0** |
| **Repository Intelligence** (§3) | tree-sitter incremental + ast-grep + ripgrep + optional LSP + git-change trigger + mmap indices | tree-sitter (4 langs) + ripgrep via `grep-searcher`+`ignore` done (`coderun-repo-intel/src/parser.rs:6-14`, `lib.rs:326-406`); **missing**: `ast-grep` crate, `search_structural`/`search_fulltext`, LSP, git watcher, incremental reparse, mmap, graph (`codebase-memory-mcp`) | **P0-P1** |
| **Knowledge Hub** (§3) | One API over BM25S/tantivy + FlashRank(`ort` int8) + engram deterministic reads | `tantivy_index.rs:1-336` exists but `coderun-storage/src/lib.rs:539-488` knowledge search still `LIKE`; `rerank.rs:52-117` is TF-IDF stub (`enabled:false` by default); `engram.rs` HTTP client exists but `KnowledgeHub::memory_search` `knowledge/src/lib.rs:269-275` hits local SQLite, not engram | **P1** |
| **Model Router** (§3, §4) | Heuristic + LiteLLM gateway, fallback chains, per-key budgets, cost tracking | Heuristic scorer done (`router/src/lib.rs:49-187`), `litellm.rs` client exists but not used as primary gateway; no fallback-chain integration test | **P1** |
| **Execution Optimizer** (§3) | Adopt RTK binary directly, tee-on-failure, honest savings reporting | Custom compressors only (`optimizer/src/lib.rs:130-262`), `RtkConfig` `core/src/config.rs:103-108` present but `doctor` reports `⚠ Not integrated` (`cli/src/main.rs:400-402`) | **P1** |
| **Event Bus + Inspection** (§3) | Async-only, never hot path; `ContextBuilt…MemorySaved` + `preview`/`replay` CLI | Bus is async (`events/src/lib.rs`) but `replay` command missing (`IMPLEMENTATION_PLAN.md:304` unchecked, `cli/src/main.rs:186-204` preview is stub) | **P1** |
| **Skill Engine** (§3) | Tag-registry behind Knowledge Hub, deterministic, full-instruction injection | Works standalone (`skills/src/lib.rs`) but not wired as Knowledge Hub sub-API; conflict detection `skills/lib` vs `knowledge/lib.rs:62-102` is simple overlap | **P2** |
| **Interfaces** (§2 portability) | `IContextBuilder`, `IModelGateway`, `IWorkflowEngine` explicit contracts | No trait definitions exist; `ARCHITECTURE.md:209-241` documents them but code has concrete structs only | **P1** |
| **Daemon** (§3 Context Engine) | Long-lived Rust daemon, UDS + MessagePack/FlatBuffers, mmap indices, quantized reranker | HTTP server + `Mutex<ContextEngine>` (`daemon/src/http_server.rs:86-88`), no mmap, no quantization | **P0** |
| **Packaging/Hardening** (§6 step 11) | `setup` wizard, `doctor`, config migration, secrets redaction before outbound calls | `doctor` partial (`cli/src/main.rs:342-412` — 3/7 checks are `⚠`), no wizard, no migration, no redaction | **P2** |
| **Docs & Eval** (§4, §6 steps 8-9) | MkDocs → Knowledge Hub, Promptfoo hitting `BuildContext` directly, scheduled job over real logs, gate promotion | `eval/` exists (`EVALUATION.md`) but no CI gate, no scheduled log job; MkDocs not started (`ROADMAP.md:71-73`) | **P2** |
| **Multi-agent** (§6 step 10) | Tier 1 adapters beyond opencode/Claude Code + Tier 2 best-effort labeled | Only opencode+Claude (`ADAPTERS.md:7-8`) | **P3** |

**Spec compliance score:** ~58% of spec §3 modules fully compliant. v0.3.0 target: ≥90%, all P0 items closed.

---

## 1. P0 — Non-Negotiable Corrections (must land in v0.3.0)

### 1.1 UDS + MessagePack IPC (spec §2, line 18-22; §3 Adapter Layer)

**Current:** `coderun-daemon/src/http_server.rs:100-118` binds `127.0.0.1:9527` JSON. `coderun-core/src/ipc.rs` derives `Serialize/Deserialize` JSON-compatible. `rmp-serde` is in `Cargo.toml:51` but unused on the wire. Hooks call HTTP (`ADAPTERS.md:134`).

**Spec:**
- Adapter Layer mandates UDS with compact binary (MessagePack or FlatBuffers), not JSON/HTTP.
- 30s hard timeout on `UserPromptSubmit`/`PreToolUse` — silently discards output, blocks session. Fail-open must emit `OriginalPassthrough` with `reason:"timeout"` and still log.

**Plan:**
1. Add `crates/coderun-daemon/src/uds_server.rs` — `tokio::net::UnixListener`, `rmp-serde` encode/decode, `tokio::time::timeout(Duration::from_secs(30))` per request. Reuse `AgentRequest`/`AgentResponse` structs (`core/src/ipc.rs:19-54`) but serialize via `rmp-serde`, not `serde_json`.
2. Keep HTTP as alternate behind feature flag `--http` for Windows dev where UDS is `named pipe` — but document that conformance tests run on UDS. On Windows, use `tokio::net::windows::named_pipe` or keep TCP + MessagePack; do NOT keep JSON default.
3. Extract shared handler: `handle_request()` `http_server.rs:219-253` → `crates/coderun-daemon/src/handler.rs` so both transports share it.
4. Add `coderun serve --socket /tmp/coderun.sock` (default per `core/src/config.rs:126`) — update `config.rs:Validator` to check socket path is writable.
5. Update adapters: `.opencode/plugins/coderun.ts` and `.claude/hooks/coderun-pregeneration.sh` to write MessagePack over UDS (fallback to HTTP if socket missing, with `warn` log). Provide migration note in `ADAPTERS.md`.

**Acceptance:**
- `cargo test -p coderun-daemon` adds `test_uds_roundtrip`, `test_timeout_returns_passthrough` (inject 31s handler, assert `OriginalPassthrough`).
- Manual spike validation per `IMPLEMENTATION_PLAN.md:6` Phase 6 step 1: intercept `chat.message` rewrite within <1s on Linux/Mac, fail-open provably logged.
- `cli/src/main.rs:cmd_doctor` checks `socket_path` writable, UDS connect succeeds.

### 1.2 `tiktoken-rs` Token Counting (spec §3 Context Engine, §4 table)

**Current:** `coderun-context/src/lib.rs:309-316` `estimate_tokens = max(chars/4, words*1.3)`. Duplicated in `coderun-optimizer/src/lib.rs:267-273`. Drift of ±40% vs real tokenizer.

**Plan:**
1. Add `tiktoken-rs = "0.6"` to `workspace.dependencies` and `coderun-context`, `coderun-optimizer`.
2. Replace `estimate_tokens` with `tiktoken_rs::cl100k_base().encode_ordinary(text).len()` — the spec-mandated local compiled tokenizer (never model API). Add `count_tokens(text, model)` wrapper that picks encoding by `routing.*_model` or defaults to `cl100k_base`.
3. Keep char-fallback only on `tiktoken` load failure, logged at `WARN`.
4. Update `assemble_context_pack` budgets (`lib.rs:219-252` 20/15/55/10 split is spec-correct per `COMPONENTS.md:218-224`) to operate on real token counts. Add per-section `token_usage` in `ContextPack::token_usage.by_source` as already structured.

**Acceptance:**
- New `test_token_count_matches_tiktoken` with fixtures (English + CJK + code).
- Benchmark: `count_tokens(10KB)` < 2ms.

### 1.3 Cache-Aware Pack, Frozen Prefix, Deduplication & Reversible Compression (spec §2 ¶5, PRINCIPLES.md:42-54)

**Current:**
- Ordering correct (`config.rs:186-190` + `context/lib.rs:218-252`).
- `session_fingerprints: Arc<Mutex<HashMap<String, HashSet<String>>>>` `context/lib.rs:46` is initialized but never populated/checked.
- YAML emits `behavioral_skills`+`docs_context`+`code_context` `context/lib.rs:258-268` but no `__frozen_prefix_end` marker, no `original_ref` indirection.

**Plan:**
1. **Dedup:** After `search_code`/`retrieve_knowledge`/`match_skills`, compute `SHA256(content)` per block, intersect with `session_fingerprints[session_id]`. Skip hit, insert miss. Add `ContextEngine::build_context` arg `session_id` from `TaskRequest` `ipc.rs:142-147`. Clear on `clear_session_fingerprint` already exists `lib.rs:295-299` — wire to daemon restart and `coderun preview --no-cache`.
2. **Frozen prefix:** Serialize YAML as three documents with comment `# --- FROZEN PREFIX END ---` after `behavioral_skills` when `behavioral_skills` is byte-identical to previous call for same session. Simpler spec-compliant alternative: always emit boundary comment line after `behavioral_skills` section — only content after boundary is allowed to change. Document boundary in `ARCHITECTURE.md` and spec.
3. **Reversible compression:** In `assemble_context_pack`, when truncating, store full content to `~/.coderun/cache/originals/{hash}.txt` and inject pointer line `... [truncated — full at {path} | request via tool `retrieve_original {hash}`]`. Add `get_original(hash)` to `ContextEngine`. This satisfies "model can retrieve an original on request, not just when something fails" (spec §2 ¶5).
4. **Fail-open:** Ensure every error branch in `build_context` returns `OriginalPassthrough`-compatible — currently `build_context` returns `Result<..., String>` and caller `http_server.rs:268` maps error to `fail-open`; audit and add `#[must_use]` + timeout wrapper.

**Acceptance:**
- `test_deduplication_skips_duplicate_block`
- `test_frozen_prefix_boundary_present_in_yaml`
- `test_reversible_compression_pointer`

### 1.4 Repository Intelligence — Complete §3 Stack

**Current baseline:** `coderun-repo-intel/src/lib.rs:148-311` + `parser.rs:28-47`.

**Gaps & fixes:**

| Gap | File | Fix |
|---|---|---|
| `ast-grep` structural search | `lib.rs:153-157` has `search_structural` stub comment; `Cargo.toml` missing `ast-grep` | Add `ast-grep = { version="0.28", features=["napi"] }` or `sg-core` + `sg-napi` per spec. Implement `search_structural(pattern, lang, max)` in `lib.rs` alongside `search_text_ripgrep`. Wire to `ContextEngine::search_code` as second pass (lexical → structural) with dedup. |
| Full-text BM25 search not wired | `tantivy_index.rs:142-208` is standalone; `lib.rs:314-406` never calls it | Add `RepositoryIntelligence::search_fulltext(query, max)` that calls `TantivyIndex::search`. On `index_repository`, upsert each file into tantivy (`add_document` `tantivy_index.rs:98-119`). Treat tantivy as *in-process* index — memory-map via `MmapDirectory` already used `tantivy_index.rs:66` ✓. |
| Git-change incremental trigger | `lib.rs:174-197` hash check is per-file but no git watcher | Add `crates/coderun-repo-intel/src/watcher.rs` using `notify = "6"` + `git2` diff. On `git head` change or `fs::watch` event, call `index_repository` incrementally. Document like language server (§2 principle). Add `coderun index --watch` and daemon background task `lifecycle.rs` spawn. |
| tree-sitter incremental reparse | `parser.rs:38-46` parses whole file each time | Cache `Parser` + `Tree` per file in `RepositoryIntelligence.file_hashes` `lib.rs:154`. On change, call `parser.parse(content, Some(&old_tree))`. Benchmark before/after. |
| Dependency graph (codebase-memory-mcp) | Missing entirely; `ROADMAP.md:81-83` planned | Add `crates/coderun-repo-intel/src/graph.rs` — build adjacency from `import`/`use`/`require` regex (`lib.rs:110-114`) + tree-sitter `import_statement` nodes. Expose `get_dependency_graph(path) -> Vec<PathBuf>` and `impact_analysis(changed_file)`. Store in new SQLite table `edges` (migration `003_graph.sql`). |
| LSP enrichment (optional) | Not started; spec says reuse agent CLI's own LSP, never hard dependency | Add `crates/coderun-repo-intel/src/lsp.rs` behind feature `lsp` — JSON-RPC client that queries `rust-analyzer`/`typescript-language-server` if `CODERUN_LSP_ENABLED=true`. Expose `get_symbol_references(name) -> Vec<Location>`. Never fail if LSP absent. |
| Binary/ignore hygiene | Done via `ignore` crate ✓ | No change. |

**Acceptance:**
- `test_structural_search_finds_pattern` (ast-grep query `function $A($$$) { $$$ }` finds `parser.rs` sample).
- `test_fulltext_search_via_tantivy`
- `test_watcher_triggers_incremental`
- `test_dependency_graph_edges`
- Benchmarks in `benches/` (see §2.5).

---

## 2. P1 — High-Priority Integrations

### 2.1 Knowledge Hub Unification (spec §3 Knowledge Hub)

**Current:** Local `LIKE` path is hot; tantivy+rerank+engram are cold.

**Plan:**
1. **Deterministic BM25 + FlashRank pipeline:** `KnowledgeHub::retrieve_knowledge` `knowledge/src/lib.rs:163-192` → replace `db.search_knowledge` LIKE with:
   - Tantivy search (`storage::TantivyIndex::search`) over `knowledge` docs (requires indexing on `store_knowledge` `lib.rs:107-115`).
   - Collect top 20 → call `Reranker::rerank` `rerank.rs:42-88` → filter `confidence >= 0.3` → return top 10. Make `K` adaptive: `K = max(5, min(20, remaining_token_budget / avg_doc_tokens))` per spec §3 Knowledge Hub ("bound the expensive reranking step").
2. **FlashRank via `ort` int8:** Current `RerankerConfig { enabled:false, endpoint:None }` `rerank.rs:18-27` is stub. Add crate `ort = "2"` + download `flashrank` ONNX `rank-T5-flan` quantized `int8` at build via `build.rs` or lazy download to `~/.coderun/models/`. Feature `rerank-onnx`. Fallback is TF-IDF path `rerank.rs:52-64` when model missing (keep but log `WARN`). Memory-map model, quantize already int8 so RAM <50MB.
3. **Engram deterministic reads:** Spec: reads happen deterministically inside pre-generation hook via HTTP API, not MCP tool-choice; writes agent-invoked. Currently `knowledge/src/lib.rs:256-275` local SQLite. Add `engram.rs` already has HTTP client — wire it:
   - `KnowledgeHub::retrieve_knowledge` first calls `db.search_knowledge` (lexical) then `EngramClient::search(query).await` if `config.memory_enabled` — merge with `confidence` boost `*1.1` for engram hits (they are cross-session). Reads are `tokio::time::timeout(2s)` — fail-open to local only if engram down (`warn` not `error`).
   - Keep writes (`memory_save`) as-is but add `engram_client.save(...)` parallel write.
4. **Unified API:** Expose `store_knowledge`, `retrieve_knowledge`, `match_skills`, `memory_search/save` all via `KnowledgeHub` as single import for `ContextEngine` — already mostly there `lib.rs:32-39`, but ensure `ContextEngine` only depends on `KnowledgeHub`, not `SkillEngine` directly. Move `skills: Vec<Skill>` `lib.rs:38` to delegate to `SkillEngine` instance (no duplication).

**Acceptance:**
- `test_retrieve_uses_tantivy_plus_rerank` (mock tantivy, assert TF-IDF fallback when `ort` off).
- `test_engram_read_in_hot_path_with_timeout`
- Doc `docs/01-architecture/COMPONENTS.md` §4 updated to reflect pipeline.

### 2.2 LiteLLM Gateway & Fallback (spec §3 Model Router, §4)

**Current:** `router/src/litellm.rs` client exists but `ModelRouter::select_model` `lib.rs:55-124` returns heuristic tier without calling gateway.

**Plan:**
1. Define `IModelGateway` trait per `ARCHITECTURE.md:220-228` in `coderun-core/src/gateway.rs`:
   ```rust
   #[async_trait]
   pub trait IModelGateway { async fn select_model(&self, req: &RoutingRequest) -> Result<RoutingDecision, String>; async fn complete(&self, req: CompletionRequest) -> Result<...>; }
   ```
   `ModelRouter` implements `IModelGateway`; alternative `LiteLLMGateway` also implements. `ContextEngine` takes `Box<dyn IModelGateway>` (local heuristic vs remote gateway) — satisfies portability principle `PRINCIPLES.md:148-156`.
2. Wire fallback chains: LiteLLM config `litellm: { endpoint, timeout_ms, max_retries }` `core/src/config.rs:95-101` already present → in `select_model`, try primary tier's model; on `reqwest::Error` or `5xx`, cascade `capable→balanced→fast` as documented `COMPONENTS.md:762-768`, logging each attempt (`info! fallback attempt`). Use `reqwest` with `timeout`.
3. Per-key budgets & cost tracking: delegate to LiteLLM's `/cost` endpoint; `storage/src/lib.rs:272-309` `token_usage` table already tracks per-request — add `cost_usd` column (migration `003_cost.sql`).
4. No-LLM tiering guard: add unit test that asserts no `reqwest`/LLM call inside `compute_*_complexity` — heuristic only.

**Acceptance:**
- `test_fallback_chain_logs`
- Integration test with mocked LiteLLM (wiremock) — primary 500 → fallback succeeds.

### 2.3 RTK Adoption (spec §3 Execution Optimizer)

**Current:** Built-in compressors; RTK not adopted.

**Plan:**
1. Vendor RTK: `cargo add rtk --git https://github.com/rtk-ai/rtk` or embed `rtk` crate if published. Spec says adopt directly as single Rust binary, zero deps, 10ms overhead, intercepts via same hooks.
2. Add `crates/coderun-optimizer/src/rtk.rs` adapter: `RtkCompressor::compress(content, output_type) -> Result<String, String>` shelling to `rtk` binary if installed, else in-process `rtk` lib. Config `rtk.enabled` `core/src/config.rs:103-108` already present — honor it. When `enabled && rtk_available`, run RTK first; fallback to existing `compress_file_read` etc.
3. Tee-on-failure: spec pattern — on failure, save full output to `~/.coderun/logs/tool-failures/{correlation_id}.log` and return pointer line. Current `compress_with_fallback` `optimizer/src/lib.rs:96-125` uses `catch_unwind` but not tee. Implement tee + honest reporting (`original_tokens` vs `compressed_tokens` vs bill impact note).
4. Update `doctor` to detect `rtk` binary: `which rtk` → `✓` else `⚠`.

**Acceptance:**
- `test_rtk_compress_with_tee_on_failure`
- Benchmark RTK <10ms per call (spec).

### 2.4 Event Bus Hardening + Inspection CLI (spec §3 Event Bus, §6 step 8)

**Current:** `events/src/lib.rs` broadcast; `cli preview` stub.

**Plan:**
1. Ensure hot path never awaits bus: audit `context/lib.rs:117-125` `emit(ContextBuilt)` is `fire_and_forget` (no `.await`). Already non-blocking ✓.
2. Add `coderun preview <prompt>` real impl: connect via UDS, send `TaskRequest`, print `ContextPack` YAML + `RoutingDecision` + token breakdown. Replace stub `cli/src/main.rs:186-204`.
3. Add `coderun replay <correlation_id>`: query in-memory ring buffer `events/src/lib.rs` `get_recent_events` + `get_events_by_correlation`. Spec says preview/replay generalizes OpenViking session-trace pattern. Need persistent buffer — extend `EventBus` to spill last 1000 events to `SQLite table events` (migration `004_events.sql`) for replay across restarts. Wire `adapter.rs` to emit `ResponseGenerated` per `COMPONENTS.md:935-941`.
4. Add `coderun events tail --follow` (bonus).

**Acceptance:**
- `test_preview_connects_to_daemon` (uses `wiremock` UDS)
- `test_replay_returns_context_built_event`

---

## 3. P2 — Packaging, Docs, Eval, Security

### 3.1 Interfaces as Contracts (spec §2 portability)

Create `crates/coderun-core/src/traits.rs`:
```rust
pub trait IContextBuilder { fn build_context(&self, task: &TaskRequest) -> Result<(ContextPack, RoutingDecision), CoderunError>; }
pub trait IModelGateway { fn select_model(&self, req: &RoutingRequest) -> RoutingDecision; }
pub trait IWorkflowEngine { fn start_workflow(&self, req: WorkflowRequest) -> Result<WorkflowId, CoderunError>; }
```
Implement for `ContextEngine`, `ModelRouter`/`LiteLLMGateway`. Reference `ARCHITECTURE.md:209-241`. Keep swappable without modifying runtime.

### 3.2 Packaging & Hardening (IMPLEMENTATION_PLAN.md:9-12 Phase 16 gap)

- **Setup wizard:** `coderun init --wizard` interactive (detect languages, suggest `max_tokens`, ask LiteLLM endpoint, init skills from `agentskills.io`).
- **`doctor` expansion** `cli/src/main.rs:342-412` → check all 7 integrations: SQLite ✓/✗, tree-sitter grammars (4 langs) ✓/✗, tantivy index readable, engram reachable (probe `/health` 2s timeout), FlashRank model present, LiteLLM reachable, RTK binary. Exit code 0 only if critical pass.
- **Config migration:** `coderun migrate --from claude|Continue|cursor` ingests community formats already supported `skills/src/lib.rs:63-65` — just add path auto-discovery.
- **Secrets redaction:** Before any outbound HTTP (engram, LiteLLM), scan payload with `regex: (api[_-]?key|secret|token)\s*[:=]` → `[REDACTED]` at `WARN`. Add test `test_secrets_redacted_before_outbound`.

### 3.3 MkDocs → Knowledge Hub (spec §4 doc source)

Currently `mkdocs.yml` absent.

1. Add `mkdocs.yml` + `docs-site/` with `mkdocs-material`, `pymdownx`.
2. On `coderun index`, walk `docs/` Markdown, push to `KnowledgeHub::store_knowledge` with `category="docs"` + index in tantivy — so docs feed lexicon retrieval as spec says.
3. CI: `mkdocs build` → `gh-pages`.

### 3.4 Offline Evaluation Gating (spec §4, PRINCIPLES.md:178)

Existing `eval/` + `EVALUATION.md` runs promptfoo locally.

Close loop:
- Add `eval/providers/context-quality.js` provider that hits `BuildContext` via UDS (custom provider per spec §6 step 9) — already claims to but check actual HTTP vs UDS.
- Add scheduled job: `systemd timer` / `cron` nightly pulling last 24h `token_usage` + `events` as eval dataset (`eval/datasets/auto-$(date).yaml`).
- Add promotion gate: `eval/run-evaluation.sh` returns non-zero if accuracy < thresholds (`EVALUATION.md:212-219` ≥90% routing, ≥85% context quality). Block `config promotion` (write new `routing` weights) on failure.

### 3.5 Security & Benchmarks (ROADMAP.md:96-95, IMPLEMENTATION_PLAN.md:16.4-16.5)

- **Input validation:** `adapter.rs`/`http_server.rs:109-163` already validates hook type; add length limits (`message <= 100KB`, `content <= 1MB`), sanitize paths (`..` rejected).
- **Rate limiting:** Token-bucket per `session_id` at adapter layer (lite, in-memory) — LiteLLM handles provider side.
- **Benchmarks:** `benches/context_bench.rs` (`criterion`): indexing time (300 files/sec target `CHANGELOG.md:118`), `BuildContext` latency p95 <100ms → v0.3.0 target <50ms `ROADMAP.md:152`, compression ratio. Add `cargo bench` to CI.

---

## 4. P3 — Multi-Agent Expansion (Tier 1 + Tier 2)

Spec §3 Adapter Layer: Tier 1 first (opencode, Claude Code, Cursor, Gemini CLI, Copilot, OpenClaw, Pi, Factory Droid). Tier 2 best-effort (Codex, Windsurf, etc.).

v0.3.0: Add **Cursor** and **Gemini CLI** adapters to prove interface portability:

- `adapters/cursor/extension.ts` (uses Cursor `UserPromptSubmit` equivalent)
- `adapters/gemini/hooks/pre-generation.sh`
- Update `ADAPTERS.md:6-8` table — new rows with `✅ Supported` and `⏳` → `✅`.
- Document Tier 2 as separate `adapters/tier2/README.md` with disclaimer.

---

## 5. v0.3.0 Work Breakdown & Dependencies

```
Phase 0 (week 1) — Foundations (no dependencies)
  ☐ traits.rs (IContextBuilder/IModelGateway/IWorkflowEngine)
  ☐ tiktoken-rs integration (context + optimizer)
  ☐ handler extraction (http → handler + uds_server)

Phase 1 (week 1-2) — P0 blocks release
  ☐ UDS + MessagePack transport + 30s fail-open
  ☐ frozen-prefix + dedup + reversible compression
  ☐ doctrine: adapter migration + docs

Phase 2 (week 2-3) — Repository Intelligence completion (depends Phase 0)
  ☐ ast-grep structural search
  ☐ tantivy full-text wiring + incremental git watcher + tree-sitter incremental
  ☐ dependency graph edges table + LSP optional

Phase 3 (week 3  ) — Knowledge Hub unification (depends Phase 2)
  ☐ tantivy → rerank pipeline + ort int8 FlashRank
  ☐ engram deterministic reads in hot path

Phase 4 (week 3-4) — Router + Optimizer parity (depends Phase 0)
  ☐ LiteLLM gateway + fallback chain + cost column
  ☐ RTK adoption + tee-on-failure

Phase 5 (week 4  ) — Observability + packaging (depends Phase 1)
  ☐ preview/replay real + events persistence
  ☐ doctor expansion + secrets redaction + setup wizard
  
Phase 6 (week 4  ) — Docs & eval hardening (parallel)
  ☐ MkDocs → Knowledge Hub ingestion + site
  ☐ promptfoo gate + scheduled dataset job
  ☐ benchmarks (criterion) + input validation + rate limiting

Phase 7 (week 5  ) — Multi-agent + release
  ☐ Cursor + Gemini CLI Tier 1 adapters
  ☐ CHANGELOG, ROADMAP, version bump 0.3.0, clippy/audit clean, 150+ tests
```

**Critical path:** Phase 0 → Phase 1 → Phase 2 → Phase 3 → Phase 7 (4 weeks).

---

## 6. File-Level Change Map

| File | Action |
|---|---|
| `Cargo.toml` (workspace) | add `tiktoken-rs`, `ast-grep`/`sg-core`, `ort`, `notify`, `git2`, `criterion` dev-dep |
| `crates/coderun-core/src/traits.rs` | **new** IContextBuilder/IModelGateway/IWorkflowEngine |
| `crates/coderun-core/src/config.rs:103-108` | add `graph_enabled`, `lsp_enabled` |
| `crates/coderun-core/src/lib.rs:1-15` | export traits |
| `crates/coderun-daemon/src/uds_server.rs` | **new** UDS + MessagePack + timeout |
| `crates/coderun-daemon/src/handler.rs` | **new** shared PreGeneration/PreToolCall handler |
| `crates/coderun-daemon/src/http_server.rs:100-340` | keep as alt transport, delegate to handler |
| `crates/coderun-context/src/lib.rs:309-343` | replace estimate with tiktoken, wire dedup+frozen prefix+reversible |
| `crates/coderun-optimizer/src/lib.rs:266-273` | tiktoken; `rtk.rs` new |
| `crates/coderun-optimizer/src/rtk.rs` | **new** RTK adapter + tee |
| `crates/coderun-repo-intel/src/lib.rs:314-406` | add `search_structural`, `search_fulltext`, graph, LSP hooks |
| `crates/coderun-repo-intel/src/watcher.rs` | **new** git notify watcher |
| `crates/coderun-repo-intel/src/graph.rs` | **new** dependency graph |
| `crates/coderun-repo-intel/src/lsp.rs` | **new** optional LSP client |
| `crates/coderun-storage/src/lib.rs:34-55` | migrations 003_graph, 004_events, cost column |
| `crates/coderun-storage/src/migrations/003_graph.sql` | **new** |
| `crates/coderun-storage/src/migrations/004_events.sql` | **new** |
| `crates/coderun-storage/src/tantivy_index.rs:142-208` | wire language_filter, ensure MmapDirectory |
| `crates/coderun-knowledge/src/lib.rs:163-275` | tantivy+rerank pipeline + engram hot reads |
| `crates/coderun-knowledge/src/rerank.rs:1-235` | ort int8 load, adaptive K |
| `crates/coderun-knowledge/src/engram.rs` | deterministic read in hot path |
| `crates/coderun-router/src/lib.rs` | implement IModelGateway |
| `crates/coderun-router/src/litellm.rs` | fallback chain + cost |
| `crates/coderun-events/src/lib.rs` | spill to SQLite |
| `crates/coderun-cli/src/main.rs:186-412` | real preview/replay, expanded doctor, wizard, migration |
| `docs/01-architecture/COMPONENTS.md` | update §§1-7 to reflect UDS, rerank, graph |
| `docs/ADAPTERS.md:6-8` | add Cursor/Gemini rows |
| `mkdocs.yml` | **new** |
| `eval/providers/*.js` | point to UDS |
| `benches/*.rs` | **new** criterion benches |
| `CHANGELOG.md`, `ROADMAP.md` | 0.3.0 entries |

---

## 7. Acceptance Checklist (release gate)

- [ ] `cargo test` ≥150 tests (ROADMAP.md:149 target) all green, `cargo clippy` 0 warnings, `cargo audit` 0 vulns
- [ ] `cargo bench` — `BuildContext` p95 <50ms (`ROADMAP.md:152`), indexing ≥300 files/s, RTK <10ms
- [ ] `coderun doctor` all critical ✓, optional `⚠` with actionable hint
- [ ] UDS spike: `chat.message` rewrite <2s, timeout 31s → `OriginalPassthrough` logged
- [ ] Promptfoo gate: routing ≥90%, context quality ≥85% (`EVALUATION.md:212`), blocks promotion
- [ ] No LLM call in any of: retrieval, compression, skill activation, tier selection (grep `reqwest`/`openai` absence test)
- [ ] `tiktoken` token counts within 5% of ground truth on fixtures
- [ ] `coderun preview "add auth"` + `coderun replay <id>` exercises full pack + event
- [ ] MkDocs site builds and is ingested into Knowledge Hub (tantivy hit for doc query)
- [ ] Secrets redaction test passes on fake `api_key: sk-...`
- [ ] Tier 1 adapter docs updated, Tier 2 disclaimer present

---

## 8. What Is Explicitly NOT v0.3.0

Per `docs/00-project/SCOPE.md:24-51` and spec §1 — these remain out of scope and must not be smuggled into v0.3.0:

- External orchestration (Temporal/DBOS) — separate product, §5 spec.
- Neo4j-style graph retrieval or vector/semantic memory — deferred (§7 open questions).
- Multi-tenancy, dashboards, auth, human approvals, audit trails, governance.
- Any LLM-based classifier for routing/retrieval (deterministic only).

---

## 9. References

- Spec: `claude.md.txt` §§1-7 (consolidated source).
- Architecture: `docs/01-architecture/ARCHITECTURE.md:8-113`, `COMPONENTS.md:9-1168`, `PRINCIPLES.md:1-196`, `SCOPE.md:1-201`.
- Current gaps filed in `docs/IMPLEMENTATION_PLAN.md:359-375` (marked `Planned`) + this doc §0 table.
- Baseline metrics: `CHANGELOG.md:120-135` (v0.2.0 128 tests, 0 warnings).
