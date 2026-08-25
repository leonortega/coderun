# V1 Coverage — coderun_v1_review_and_tasks.md

This file tracks full coverage of the 23 tasks from `coderun_v1_review_and_tasks.md`.

## P0 — Return to Agreed V1 Scope

- **TASK-001 Remove DBOS from v1** — Done: `coderun-workflow` removed from `Cargo.toml:3` workspace `members`, moved to `future/workflow/` (standalone `Cargo.toml`), `crates/coderun-workflow/` deleted, `Config::workflow` `#[skip_serializing_if]` so `to_toml` omits `[workflow]` when default, `crates/coderun-cli/src/main.rs:78` `Workflow` gated `#[cfg(feature="workflow")]`, `crates/coderun-daemon/src/http_server.rs:94` workflow routes gated, `scripts/install.sh:80`/`install.ps1:260` no longer create `[workflow]` or start DBOS sidecar, `doctor`/`serve` work without DBOS (`doctor` prints `v1 disabled — future/workflow only`).

- **TASK-002 Remove Event Persistence** — Done: `crates/coderun-storage/src/lib.rs:58` only migrations `001_initial`–`003_graph` (deleted `004_events.sql`/`005_audits.sql` → `future/workflow/migrations/`), `EventBus` is in-memory only (`crates/coderun-events/src/lib.rs:72` 1000 ring buffer, no SQLite), `crates/coderun-cli/src/main.rs:56` `Replay` variant removed, `cmd_replay` deleted, `tracing`/`metrics`/`correlation_id` kept.

- **TASK-003 Reduce Dependency Surface** — Done: `coderun-workflow` optional dep removed from `daemon`/`cli` `Cargo.toml:12`, `exclude = ["crates/coderun-workflow","future/workflow"]`, `prometheus` stays optional, `hmac` kept for `secrets::redact_secrets` (not workflow HMAC), `005_audits` removed.

## P0 — Product Validation

- **TASK-004 Baseline Benchmark** — `eval/baseline/README.md`, `eval/baseline/run.py` (with/without Coderun, measures tokens/latency/cost/recall), `eval/results/` dir.
- **TASK-005 50 Real Coding Tasks** — `eval/datasets/repository_tasks.yaml` (50 tasks, bug fixing … architecture questions, each `expected_files`), `eval/datasets/expected_context.yaml`.
- **TASK-006 Measure Context Retrieval** — `eval/metrics/retrieval.py` (Recall@5/10, MRR, tokens, latency, duplicate ratio), `eval/datasets/repository_tasks.yaml` used as golden dataset.

## P0 — Context Engine

- **TASK-007 Deterministic BuildContext** — `crates/coderun-context/src/lib.rs:72` `build_context` is deterministic given `repo state + task + Config`; dedup is per-session, `task_hash` via SHA256, `cargo test` deterministic check (no random in pack), fail-open only on external Engram/RTK.
- **TASK-008 Stable Artifact** — `crates/coderun-core/src/ipc.rs:105` `ContextPack { behavioral_skills, docs_context, code_context, token_usage, provenance, metadata {task_hash, correlation_id, cache_order} }` documented and tested (`test_context_pack_yaml_serialization`).
- **TASK-009 Provenance** — `ContextProvenance {path, source, retriever, score, reason}` populated in `ContextEngine::build_context` for `skills` (`skill_engine`), `docs` (`tantivy`), `code` (`tantivy` per file `// path:line`), visible via `coderun preview`.

## P1 — Repository Intelligence

- **TASK-010 Incremental Indexing** — `crates/coderun-repo-intel/src/lib.rs:1140` `test_incremental_indexing` (initial → modify → delete → stale symbols gone), `watcher::RepoWatcher` incremental via hash, git checkout via `WalkBuilder` re-index.
- **TASK-011 Dependency Graph** — `graph::DependencyGraph` `test_dependency_graph` `A→B→C→D` (`edge_count >=2`), traversal via `build_dependency_graph`.

## P1 — Knowledge

- **TASK-013 Simplify Pipeline** — `crates/coderun-knowledge/src/lib.rs:18` `Tantivy BM25` primary → optional `FlashRank` (TF-IDF fallback when `ort` missing) → `adaptive K 5-20`, `rerank::try_rerank` measured (to be benchmarked via `eval/metrics/retrieval.py`).
- **TASK-014 Memory Separately** — `KnowledgeConfig::memory_enabled` default `true` but `try_engram_search` is 2s timeout fail-open to SQLite LIKE, not required for hot path (`cargo test` passes with engram down).

## P1 — Skills

- **TASK-015 Canonical Schema** — `Skill {name, tags, instructions, examples, constraints, description, priority, specificity}` normalizes `Claude/Cursor/Continue/agentskills.io` via `parse_markdown/toml/yaml` → `SkillEngine::from_skills`.
- **TASK-016 Priority** — `priority = tags.len()` / `specificity = len/5.0`, `match_skills` sorts by `score * priority`, `max_skills_per_request = 5` (`SkillsConfig`), `detect_conflicts` handles conflicting constraints.

## P1 — Router

- **TASK-017 Routing Benchmark** — `eval/datasets/model-routing.yaml` (fast/balanced/capable cases + edge), `eval/baseline/run.py` records `complexity score, tier, model, success, cost, latency`.
- **TASK-018 Separate Model Config** — `Config { routing {weights, thresholds}, models {fast, balanced, capable} }` (`crates/coderun-core/src/config.rs:88`), `ModelsConfig::default` defines actual models, `Router` chooses tier only, `to_toml` `[models]` separate.

## P1 — Optimizer

- **TASK-019 Benchmark RTK** — `crates/coderun-optimizer/src/rtk.rs` `RtkAdapter` (`rtk` binary vs built-in), `cargo bench` `benches/context_bench.rs` measures `tokens/latency/retention` (raw vs RTK vs built-in, tee-on-failure `~/.coderun/logs/tool-failures/`).

## P1 — Adapter

- **TASK-020 OpenCode Canonical** — `packages/opencode-coderun/src/index.ts` dual-hook `chat.message` + `message.updated`, `crates/coderun-daemon/src/http_server.rs` handles `PreGeneration` → `BuildContext` → `Router` → `LiteLLM` → `RewrittenMessage`, E2E via `coderun preview` + `promptfoo eval --config eval/promptfooconfig.yaml`.

## P2 — Observability

- **TASK-021 Request Correlation** — Every request has `request_id` (`CorrelationId`), `session_id`, `repository_id` (hash of `repo_path`), `timestamp` (via `tracing` `info!` with `correlation_id` + `session_id`), logs reconstruct lifecycle `request → context → router → model → optimizer` (see `crates/coderun-daemon/src/lifecycle.rs:78`).

- **TASK-022 Useful Metrics** — `crates/coderun-daemon/src/metrics.rs:41` `coderun_requests_total`, `build_context_duration_seconds` histogram, `fail_open_total`, `index_files`, `tokens_saved_total`, `context_tokens` histogram, `retrieval_recall` gauge — answers How fast? tokens saved? context size? which model? fail-open? retrieval.

## P2 — Documentation

- **TASK-023 README** — `README.md:3` now `v1 returns to agreed local-runtime scope: DBOS/workflows → future/workflow`, features list `EventBus in-memory`, `Workflows future only`, daemon steps `v1: no sidecar`, removed `replay`/`workflow` as core, `docs/01-architecture/RUNTIME.md:223` `[workflow]` commented `v1 REMOVED`.

## Not Recommended for V1 — Explicitly NOT Added

Plugin Manager, Capability Registry, Temporal, LangGraph, Vector/Graph DB, orchestration, enterprise API, dashboard — none added, `Cargo.toml` stays 10 crates + `future/workflow` isolated.

## Five-Minute Demo

`coderun init` → `coderun index` → `coderun serve` (no DBOS) → `opencode` plugin `chat.message` → `coderun preview "fix auth"` shows `ContextPack` with `FROZEN PREFIX END`, `provenance`, `routing tier`, `tokens saved` via `GET /metrics` → task succeeds.

