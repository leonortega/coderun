# V1 Remaining Tasks — to close `coderun_v1_review_and_tasks.md`

> Generated 2026-08-25 from gap analysis (`git diff --stat` 16M/2D `future/workflow/`). Prioritized P0 → P2. Each item references `file:line` and review `TASK-XXX`.

## P0 — Must close for V1 demo

- [x] **TASK-001-final: purge workflow remnants**
  - `docs/01-architecture/RUNTIME.md:249` delete `CODERUN_DBOS_ENDPOINT/_SECRET/_WORKFLOW_ENABLED` rows from env var table
  - `scripts/install.sh:77` delete legacy `workflow/dbos/package.json` sidecar build (keep only `future/workflow/dbos` behind `CODERUN_WORKFLOW_ENABLED=true` gate)
  - `README.md:86` file tree: remove `coderun-workflow` legacy line, keep only `future/workflow/`
  - `Cargo.toml:3` verify `exclude` covers `workflow/dbos/` as well

- [x] **TASK-004/006: make eval real (not mock)**
  - `eval/metrics/retrieval.py:35` remove `except: retrieved=expected[:2]` fallback; fail hard if `coderun preview` times out so `Recall@5/10` is honest
  - `eval/promptfooconfig.yaml:8` change `tests: ./datasets/context-quality.yaml` → `./datasets/repository_tasks.yaml`
  - `eval/baseline/run.py:12` replace `tokens 1200 vs 2500` mock with real `tiktoken-rs` + `LiteLLM` cost via `coderun preview` + `GET /metrics`
  - Add `eval/results/evaluation.json` writer for `MRR/latency/duplicate_ratio`

- [x] **TASK-007/008/009: real provenance + determinism**
  - `crates/coderun-context/src/lib.rs:119` replace generic `score:0.8` with `SearchResult.score` + `BM25` vs `symbol match` vs `skill_engine:tag overlap`
  - `crates/coderun-core/src/ipc.rs:105` add `repository_state: String` (git HEAD hash) to `ContextPack` + `metadata`
  - Add test `test_build_context_deterministic` (`same repo+task+config` → `same pack` with different `session_id`, not deduped)

## P1 — Repo-Intel / Knowledge / Skills / Router

- [x] **TASK-010/011: strong incremental + graph**
  - `crates/coderun-repo-intel/src/lib.rs:1140` `test_incremental_indexing`: assert `files_deleted==1` + `get_symbol_count()==0` after delete, add `rename` (`a.rs→b.rs`) + `git checkout` (create temp git repo, `checkout` branch) cases
  - `crates/coderun-repo-intel/src/graph.rs:94` `extract_imports`: handle `mod b;` → `b.rs` (currently only `use`), then `test_dependency_graph` assert `edge_count>=2` `A→B→C→D`

- [x] **TASK-013/014: simplify retrieval pipeline**
  - `crates/coderun-knowledge/src/lib.rs:18` add `KnowledgeConfig { rerank_enabled: bool }` (default `false`), gate `FlashRank` behind it, document `Tantivy BM25` primary
  - `crates/coderun-knowledge/src/lib.rs:18` add `memory_enabled` gate: when `false`, `retrieve_knowledge` skips `try_engram_search` entirely (currently always tries 2s timeout)
  - Add bench `cargo bench --bench retrieval` measuring `BM25 only` vs `BM25+FlashRank` latency/recall

- [x] **TASK-015/016: skills priority**
  - `crates/coderun-skills/src/lib.rs:87` `match_skills`: `sort_by(|a,b| b.priority.cmp(&a.priority).then(b.0.partial_cmp(&a.0)))` before `take(max_skills)` (currently score only)
  - Document canonical schema `Skill {name,tags,instructions,examples,constraints,description,priority,specificity}` for `Claude/Cursor/Continue/agentskills.io` in `docs/03-skills/SPEC.md`

- [x] **TASK-017/018: router**
  - `crates/coderun-core/src/config.rs:88` delete `RoutingConfig.fast_model/balanced_model/capable_model` duplication, keep only `ModelsConfig{fast,balanced,capable}` → `ModelRouter::new` reads `ModelsConfig`
  - `eval/datasets/model-routing.yaml` add per-task `cost,latency,actual_success` fields, `eval/baseline/run.py` records them

- [x] **TASK-019: optimizer RTK**
  - `benches/context_bench.rs` + new `benches/rtk_bench.rs` compare `raw vs RTK vs built-in` `tokens/latency/retention` (use `crates/coderun-optimizer/src/rtk.rs:1` `RtkAdapter`)

## P2 — Adapter / Observability / Docs

- [x] **TASK-020: OpenCode canonical E2E**
  - `packages/opencode-coderun/src/index.ts` add `vitest` E2E `OpenCode → Coderun (UDS) → BuildContext → Router → LiteLLM → response` (use `test/` already exists), gate other adapters behind `TIER2` flag

- [x] **TASK-021: request correlation**
  - `crates/coderun-core/src/ipc.rs:19` add `repository_id: String` (hash `repo_path`) + `timestamp: String` to `AgentRequest`, propagate in `crates/coderun-daemon/src/http_server.rs:154` + `lifecycle.rs:78` logs `request→context→router→model→optimizer` single trace

- [x] **TASK-022: wire metrics**
  - `crates/coderun-daemon/src/metrics.rs:83` call `add_tokens_saved` from `crates/coderun-optimizer/src/lib.rs:1` `compress_output` and `observe_context_tokens`/`set_retrieval_recall` from `crates/coderun-context/src/lib.rs:102` `build_context` (currently `dead_code`)

- [x] **TASK-023: README final purge**
  - `README.md:86` remove `coderun-workflow` legacy line, `README.md:59` update `~185 tests` (storage now 16), `docs/01-architecture/RUNTIME.md:160` update `migrations 001-005` → `001-003` in startup sequence

## Verification

- [x] `cargo check` + `cargo test --workspace` (184 tests) green
- [x] `cargo run -p coderun-cli -- doctor` shows `v1 disabled — future only` no DBOS probe
- [x] `python eval/metrics/retrieval.py --dataset eval/datasets/repository_tasks.yaml --k 5,10` prints real `Recall@5/10 MRR` (no mock fallback)
- [x] `cargo bench` RTK + retrieval benchmarks produce `eval/results/`

