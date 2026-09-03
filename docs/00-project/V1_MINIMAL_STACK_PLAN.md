# V1 Minimal Stack Plan — Tree-sitter + Tantivy + SQLite + Git

> Derived from benchmark feedback: do not duplicate `rg`; remove undemonstrated tools; keep the smallest stack that solves `repository → BuildContext`.
> Replaces broad `Knowledge Hub` + `MkDocs` + `LiteLLM` runtime with repository context only.
> History: `docs/01-architecture/ENGRAM_CBM_REMOVAL.md`, `docs/01-architecture/FLASHRANK_REMOVAL.md`, `docs/00-project/V1_PLAN.md:1`.

## 1. Objective

```
Knocode indexes a repository once, understands files + symbols, maintains the index incrementally,
and produces a small ranked BuildContext for a coding agent's task.
```

Agent keeps its own tools outside Knocode:

```
CODING AGENT → read / rg / grep / glob / bash / git
                ↓
             KNOCODE → BuildContext → Context Pack
```

## 2. Keep vs Remove (v1 decision)

| Tool | Decision | Summary reason | Current location |
|------|----------|----------------|------------------|
| Tree-sitter | ✅ Keep | `source → AST → symbols` validated (PascalCase + Symbols) | `crates/knocode-repo-intel/src/parser.rs`, `crates/knocode-storage/src/tantivy_index.rs:138,205` |
| Tantivy / BM25 | ✅ Keep | Core candidate generation, but fix large-repo latency | `crates/knocode-storage/src/tantivy_index.rs:376,593` `crates/knocode-repo-intel/src/lib.rs:622` |
| SQLite | ✅ Keep (simplified) | Metadata only, not competing search | `crates/knocode-storage/src/lib.rs`, `crates/knocode-repo-intel/src/lib.rs:287` |
| Git | ✅ Keep | `discover repo / changed files / incremental / revision awareness` — not retrieval | `crates/knocode-repo-intel/src/lib.rs:225,1001` `is_file_unchanged_fast`, `watcher.rs`, `crates/knocode-cli/src/main.rs:311` |
| RTK | ⚠️ Optional | Tool-output compression; fallback to normal output if absent | `crates/knocode-optimizer/src/lib.rs` |
| FlashRank | ❌ Remove | Worse MRR + 17× latency — see §2.1 | Already passthrough `crates/knocode-knowledge/src/rerank.rs:9` |
| codebase-memory-mcp | ❌ Remove | No measurable R@5 lift — see §2.2 | Already removed, graph fallback deleted `crates/knocode-context/src/lib.rs:324` |
| Engram | ❌ Remove | No v1 value, operational cost > benefit — see §2.3 | Already removed `crates/knocode-knowledge/src/engram.rs` deleted |
| MkDocs | ❌ Remove from runtime | Docs publishing only; keep `docs/*.md` without build — see §2.4 | `mkdocs.yml`, `crates/knocode-repo-intel/src/lib.rs:383` ingestion |
| Knowledge Hub (broad) | ❌ Remove | `docs/ADRs/memory/skills` too broad; collapse to Repository Context — see §2.5 | `crates/knocode-knowledge/src/lib.rs:30` |
| LiteLLM / Model Router | ❌ Removed | `query → model routing` removed — see `docs/01-architecture/LLM_ROUTING_REMOVAL.md` | `crates/knocode-router` deleted |
| RRF | ❌ Remove | No demonstrated benefit — see §2.7 | — |
| Embeddings / vector DB | ❌ Not needed | No evidence lexical insufficient — see §2.8 | — |
| Custom reranker | ❌ Not needed | Candidate generation is the problem — see §2.9 | — |
| LLM query expansion | ❌ Not yet | Deterministic field-aware works — see §2.10 | `crates/knocode-storage/src/tantivy_index.rs:229` |

### 2.1 FlashRank — removed (worse ranking, 17× latency)

- **What it was:** Cross-encoder neural reranker (`rank-T5-flan` int8 via `ort`) reordering BM25 candidates `crates/knocode-knowledge/src/rerank.rs:3`.
- **Benchmark (48-task eShopOnWeb, Aug 2026) `docs/01-architecture/FLASHRANK_REMOVAL.md:24`:**
  ```
  Baseline BM25:  Recall@5 16.97%  MRR 0.5003  Latency 507ms
  + FlashRank:    Recall@5 18.94%  MRR 0.4325  Latency 8532ms
  ```
  +1.97pp recall, **-6.78pp MRR** (reranker moves correct results down), **17× slower**. Cost-benefit unacceptable for real-time `BuildContext` (<1s budget). Index-time improvements (`PascalCase` +5.22pp, `symbol_name` +0.43pp, `path` +1.46pp `crates/knocode-knowledge/src/rerank.rs:24-30`) are zero-cost and deterministic.
- **v1 decision:** Keep as passthrough `crates/knocode-knowledge/src/rerank.rs:68` `rerank()` returns input order, `ort` feature removed `crates/knocode-knowledge/Cargo.toml`. Eval only offline `eval/run_comparison.sh:7`.

### 2.2 codebase-memory-mcp — removed (no measurable contribution)

- **What it was:** Node `codebase-memory-mcp` `nicholasgasior/codebase-memory-mcp` CLI `search_graph --json --relationship imports` probed with 10s timeout `crates/knocode-repo-intel/src/graph.rs:266` + `crates/knocode-context/src/lib.rs:354` fallback, unified to `~/.knocode/bin/codebase-memory-mcp`.
- **Benchmark `docs/01-architecture/ENGRAM_CBM_REMOVAL.md:52`:**
  ```
  BM25:                   16.97% R@5
  + codebase-memory-mcp:  16.97% R@5  (identical, 0.0pp)
  ```
  Graph had `0 edges` on 53k DefinitelyTyped because deferred (`crates/knocode-cli/src/main.rs:311`) — unmeasured, but even when built local `extract_imports()` regex `graph.rs:401` already provides file-import adjacency. No evidence graph improved retrieval.
- **Costs:** Requires `npx`/`npm`/`Node`, global install, `~/.knocode/bin` unification, supply-chain bloat `scripts/install.ps1:160`, contradicts `docs/00-project/PRINCIPLES.md:56` Local-First in-process stack (`ignore`/`ripgrep`/`sg-core` crates).
- **v1 decision:** Deleted `try_codebase_memory_mcp_public` call in `ContextEngine` `crates/knocode-context/src/lib.rs:324`, kept local `DependencyGraph` regex as sole file-level source (`file A imports file B` only).

### 2.3 Engram — removed (operational cost, fallback is the product)

- **What it was:** Go binary `Gentleman-Programming/engram` SQLite+FTS5 cross-session memory `crates/knocode-knowledge/src/engram.rs:1-317` `EngramClient` `search --json` 2s timeout + local `LIKE` fallback, `memory_enabled`/`memory_endpoint` `crates/knocode-core/src/config.rs:51`.
- **Why removed `docs/01-architecture/ENGRAM_CBM_REMOVAL.md:60`:**
  1. Benchmark: no R@5 lift vs local SQLite `LIKE`; `try_engram_search` immediately falls back to `db.search_memory()` — fallback *is* implementation.
  2. Operational: extra Go lifecycle, spawn/health-check, HTTP 2s timeout, `~/.knocode/bin/engram` unification, `doctor` probe, `KNOCODE_ENGRAM_ENDPOINT` env.
  3. Excluded already: `knocode init` seeds with `memory_enabled: false`, `INDEXING_PERF_PLAN.md:3` excludes engram; install tar handling `scripts/uninstall.ps1:289`.
  4. v1 scope: no concrete `conversation memory` use case; `Repository Context` does not need cross-session memory.
- **v1 decision:** Delete `crates/knocode-knowledge/src/engram.rs`, replace with SQLite+tantivy local `crates/knocode-storage/src/lib.rs` `save_memory`/`search_memory` (local only, no subprocess).

### 2.4 MkDocs — removed from runtime (documentation, not retrieval)

- **What it was:** `mkdocs.yml` + `docs/**/*.md` published as site; runtime ingested `docs/**/*.md → store_knowledge(category="docs", key, value)` `crates/knocode-repo-intel/src/lib.rs:383-412` and tantivy `docs:…` docs for retrieval.
- **Why removed:** MkDocs contributes to `docs/architecture.md` publishing, not to `repository indexing → retrieval → BuildContext`. The v1 toolchain `knocode` `repository indexing / retrieval / context building / agent integration` does not need a site generator. Markdown remains without MkDocs — agent can `read` `docs/*.md` directly, or at most one `README.md` read, not a ranked KnowledgeHub ingestion.
- **v1 decision:** Keep `docs/*.md` as plain markdown; delete runtime `docs_dir` WalkBuilder ingestion block `lib.rs:383`; keep `mkdocs` as optional local `mkdocs build` for publishing, not a `knocode init` dependency or CI gate.

### 2.5 Knowledge Hub (broad) — removed (too broad, defer)

- **What it was:** `KnowledgeHub` `crates/knocode-knowledge/src/lib.rs:30` unified `docs + ADRs + memory + Engram + community skills + retrieval` behind one API — tag-based skill matching + BM25 docs/code + memory.
- **Why removed:** After Engram/MkDocs removal, the remaining `docs/ADRs` have no proven retrieval need beyond source files. `Repository Context` for v1 is sufficient:
  ```
  source files + symbols + paths + Git state + optional README (single file read)
  ```
  Skill matching `crates/knocode-skills/src/lib.rs` can stay as deterministic tag scorer, but does not require a `KnowledgeHub` ranking pipeline. `BuildContext` hot path `crates/knocode-context/src/lib.rs:472` currently does parallel `retrieve_knowledge` BM25 `knowledge_hub.retrieve_knowledge` — this adds latency without validated recall benefit (knowledge `is_initialized` check `lib.rs:174` already fail-opens empty).
- **v1 decision:** Collapse to `RepositoryContext`; remove `retrieve_knowledge` from hot path (allow `knowledge_context = ""` without error); inline `SkillEngine::match_skills` directly in `ContextEngine` or keep `crates/knocode-skills` alone; deprecate `crates/knocode-knowledge` for v1 or keep only for skills.

### 2.6 LiteLLM / Model Router — deferred (not core to BuildContext)

- **What it was:** `crates/knocode-router/src/lib.rs` `ModelRouter::select_model` heuristic + `LiteLLM` gateway `IModelGateway` `capable→balanced→fast` cascade, `crates/knocode-context/src/lib.rs:594` `select_model` returning `RoutingDecision` alongside `ContextPack`.
- **Why deferred:** v1 strongest validated capability is `repository → context`, not `query → model routing`. Benchmarks measure `Recall@5`/`MRR`, not tier accuracy; no working functionality depends on routing for context retrieval. Keeping `LiteLLM` adds `LITELLM_URL`, `KNOCODE_LITELLM_*`, `axum`/`reqwest` surface without proven v1 need. Challenge per feedback: does routing belong in first implementation?
- **v1 decision:** Option A (preferred minimal): feature-flag `router` default off, `BuildContext` returns `ContextPack` only. Option B: keep heuristic `select_model` (no HTTP call) as cheap default, never require `LITELLM_URL`. Either way `BuildContext` works standalone.

### 2.7 RRF — removed (no demonstrated benefit)

- **What it was:** Reciprocal Rank Fusion merging BM25 + symbol results (historical).
- **Why removed:** Current merge `crates/knocode-context/src/lib.rs:284` keeps `max score per path` (deterministic), not RRF. No benchmark showed RRF improving `Recall@5` over max-score merge; candidate generation quality dominates. Remove to keep ranking explainable.

### 2.8 Embeddings / vector DB — not yet (no evidence needed)

- **Why not yet:** Lexical `Tantivy BM25` with field boosts `tantivy_index.rs:621` + `PascalCase` `tantivy_index.rs:138` + `tokenize_path` `tantivy_index.rs:433` already addresses `find type NextFunction → NextFunction` via `symbol_name:NextFunction` without semantic search. Benchmarks show deterministic index-time representation beats reranker; no ceiling demonstrated requiring vectors. Defer until lexical `Recall@5` plateaus on 50-task eval.

### 2.9 Custom reranker — not needed (candidate generation is the problem)

- **Why not needed:** Same argument as FlashRank — `FLASHRANK_REMOVAL.md:29` reranker adds 8.5s for +1.97pp while MRR degrades. `V1_PLAN.md:6` identifies retrieval as candidate generation + field-aware BM25, not post-processing. Keep `rerank.rs:68` passthrough.

### 2.10 LLM query expansion — not yet (deterministic works)

- **Why not yet:** Natural-language queries (`find type NextFunction`) already handled by stop-word filtering `STOP_WORDS` `crates/knocode-storage/src/tantivy_index.rs:119` + field boosts. Prototype `expand_code_vocabulary` `tantivy_index.rs:229` and `symbol query expansion` diluted recall (feedback §5). Giant OR queries (`find OR type OR function …`) hurt precision. Keep vocabulary expansion minimal and deterministic; add only if field-aware baseline plateaus.

## 3. Target Architecture (minimal)

```
┌──────────────────────────────────────────┐
│              KNOCODE V1                  │
├──────────────────────────────────────────┤
│  Tree-sitter → Symbol extraction         │
│       ↓ Code tokenization (PascalCase)   │
│  Tantivy / BM25 → Deterministic ranking  │
│       ↓ candidateK → Top 20              │
│  Context selection → Token budgeting     │
│       ↓ tiktoken-rs cl100k_base          │
│  BuildContext (YAML, FROZEN PREFIX END)  │
├──────────────────────────────────────────┤
│ Supporting: SQLite (metadata), Git       │
│ Optional: RTK (compress), skills (tag)   │
└──────────────────────────────────────────┘

Supporting detail:
  SQLite → repository metadata, index state (files_indexed, mtimes, hashes), config, sessions
  Tantivy → searchable files, paths, symbols, tokens (field boosts symbol_name 3.0× path 2.5× tantivy_index.rs:621)
  Git → `git diff` → changed files → re-index only those files (lib.rs:225 mtime+size shortcut)
```

## 4. Implementation Steps (no code yet — plan only)

### Phase 0 — Docs / config cleanup (no runtime risk)
1. Keep `docs/*.md` markdown, remove `mkdocs.yml` build as runtime dep (retain for `mkdocs build` locally if needed, but not in `knocode init` or CI gate).
2. Update `docs/ROADMAP.md`, `docs/01-architecture/RUNTIME.md`, `README.md:9` to reflect MkDocs/Knowledge Hub/LiteLLM as deferred, not required.
3. Add `KNOCODE_BUILD_GRAPH=1` / `KNOCODE_CANDIDATE_K` already present `crates/knocode-storage/src/tantivy_index.rs:722` — keep as v1 tuning knobs.

### Phase 1 — Remove MkDocs ingestion from runtime
- Delete `crates/knocode-repo-intel/src/lib.rs:383-412` block `docs_dir → WalkBuilder → store_knowledge("docs",…)` + tantivy `docs:…` add. Keep `docs/` on disk for agent `read`, not indexed via KnowledgeHub.
- Remove `mkdocs` feature from `scripts/install.*` if present; keep `docs/` as plain markdown.
- Verify `cargo test -p knocode-repo-intel test_mkdocs_ingestion_is_idempotent` either removed or updated to file-read fallback.

### Phase 2 — Collapse Knowledge Hub to Repository Context
- Current: `KnowledgeHub` `crates/knocode-knowledge/src/lib.rs:30` handles `docs / ADRs / memory / skills / engram` + BM25 `retrieve_knowledge`.
- Target: `RepositoryContext` = `source files + symbols + paths + Git state + optional project README` (single file read, not ranked memory).
- Steps:
  a. Keep `knocode-skills` tag matcher for `behavioral_skills` if desired, but move out of `KnowledgeHub` or make optional (`skills_context` may be empty without failure).
  b. Remove `retrieve_knowledge` BM25 path from `BuildContext` hot path `crates/knocode-context/src/lib.rs:472-476` (keep parallel tasks but allow zero knowledge without `is_initialized` check).
  c. Keep `crates/knocode-knowledge` only if needed for skills; otherwise inline `SkillEngine` directly in `ContextEngine` and deprecate crate for v1.
  d. Migrate any `category="docs"` seeds to plain file reads via `RepositoryIntelligence::get_file_content`.

### Phase 3 — Simplify SQLite (deduplicate search)
- Today: symbols stored twice — `db.insert_symbol` `crates/knocode-repo-intel/src/lib.rs:287` + tantivy `symbols_text` `crates/knocode-storage/src/tantivy_index.rs:471`.
- Change:
  a. `SQLite` keeps: `files(path, hash, size, language, file_class, last_indexed_at)`, `index_state`, `config`, `sessions/cache`.
  b. Remove `symbols` table as retrieval source; keep only if needed for `search_symbols` deterministic fallback `lib.rs:463` — otherwise replace with tantivy `symbol_name` field query (already boosted `tantivy_index.rs:621`).
  c. Update `index_repository` to not `insert_symbol` when `KNOCODE_SYMBOLS_ENABLED=false` already, and for v1 make DB symbol insert optional/configurable (keep for `search_symbols` tests, but not required for BM25).
- Result: `SQLite` not a second search engine; `Tantivy` sole retrieval.

### Phase 4 — Make Router/RTK optional (or defer)
- `crates/knocode-router` + `LiteLLM` gateway `crates/knocode-context/src/lib.rs:594` `select_model`:
  Option A (minimal): feature-flag `router` (default off); `BuildContext` returns `ContextPack` only, no `RoutingDecision`.
  Option B: keep `ModelRouter::select_model` heuristic (no LiteLLM call) as cheap default, but do not require `LITELLM_URL` / `KNOCODE_LITELLM_*`.
- `RTK` `crates/knocode-optimizer` already fallback: if binary absent, pass through; mark docs as optional.

### Phase 5 — Preserve incremental + profiling
- Keep `validate_index` caching `crates/knocode-repo-intel/src/lib.rs:148` `open_cached` + `cached_reader` `crates/knocode-storage/src/tantivy_index.rs:419` and graph gating `crates/knocode-context/src/lib.rs:324` for 53k fix.
- Keep `candidateK` sweep via `KNOCODE_CANDIDATE_K` `tantivy_index.rs:722` (20/50/100/200) — choose final 50/100 before freezing default (was `max*3=60`).
- No `RRF`, no embeddings, no LLM expansion in v1.

## 5. Verification per phase

- `cargo test -p knocode-storage -p knocode-repo-intel -p knocode-context -p knocode-knowledge` must stay green after each phase (knowledge collapse may delete `test_retrieve_knowledge_repo_scoped` — update to file-read test).
- `KNOCODE_PROFILE=1 cargo run -p knocode-cli -- preview "find type NextFunction"` on this repo should stay <500ms; on 53k clone `tantivy.search <10ms` + `build_context.total <1s` when graph skipped.
- `rg -g '!target' "mkdocs|KnowledgeHub|LiteLLM|RRF"` after Phase 1/2/4 should only show historical ADR `FLASHRANK_REMOVAL.md`/`ENGRAM_CBM_REMOVAL.md`, not runtime imports.

## 6. Out of scope for this plan

- Graph beyond `file imports` (no call graph).
- Vector DB / reranker / query expansion.
- Multi-repo `v2`, conversation memory, web dashboard.

Proceed to implement Phase 0→1 after approval; no code changes in this file.
