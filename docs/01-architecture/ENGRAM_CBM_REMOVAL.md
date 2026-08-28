# Engram and codebase-memory-mcp Removal — Architectural Decision Record

**Date:** August 2026
**Status:** Accepted
**Decision:** Remove engram (cross-session memory) and codebase-memory-mcp (dependency-graph probe) from v1 runtime path. Historically documented; no runtime dependency.

---

## Context

### engram

`engram` (`Gentleman-Programming/engram`) is a single Go binary providing SQLite+FTS5 cross-session memory with an MCP-native HTTP/CLI API (`save` / `search` without LLM or embedding dependency). Coderun integrated it as an optional, fail-open layer inside `KnowledgeHub`:

- `crates/coderun-knowledge/src/engram.rs` — `EngramClient` discovering `~/.coderun/bin/engram`, CLI `search --json` with 2s timeout / local `LIKE` fallback, health-check + bootstrap seeding of `repository-profile` / `readme` during `coderun init` (see `docs/00-project/GLOSSARY.md:177`, `PRINCIPLES.md:61`, `COMPONENTS.md:460`).
- Config surface `[knowledge] memory_enabled / memory_endpoint / engram_binary_path` (`coderun-core/src/config.rs:51`, `docs/01-architecture/RUNTIME.md:177`, `README.md:320`).
- Install scripts bundled/unified the binary to `~/.coderun/bin/engram` (`scripts/install.ps1:160`, `install.sh:45`).
- Already excluded from `coderun init` hot path (`KnowledgeConfig { memory_enabled: false }` during init — `.opencode/skills/coderun/SKILL.md:67`, `docs/INDEXING_PERF_PLAN.md:29`).

The question was: **does an external FTS5 memory justify its operational cost when SQLite + tantivy BM25 already persists all knowledge?**

### codebase-memory-mcp

`codebase-memory-mcp` (`nicholasgasior/codebase-memory-mcp`) extracts repository dependency graphs via a Node.js CLI (`cli search_graph --project <name> --json --relationship imports`). Coderun probed it as a first-class graph source with a 10s subprocess timeout, falling back to local `extract_imports()` regex / AST parsing (`crates/coderun-repo-intel/src/graph.rs:4,266`).

- Binary probed via `~/.coderun/bin/codebase-memory-mcp` → `npx` fallback (`graph.rs:266`, `crates/coderun-cli/src/main.rs:476`), unified by install scripts (`install.ps1:160`, `install.sh:163`).
- Explicitly deferred for large repos (`>5000` files graph deferred unless `CODERUN_BUILD_GRAPH=1` — `cli/src/main.rs:311`) and excluded from the init hot path (`INDEXING_PERF_PLAN.md:3`).
- Reuse rationale tracked in `PRINCIPLES.md:69` (“Reuse Existing Tools”).

The question was: **does an external Node graph probe justify npx/Node + 10s timeout overhead when the local `import`/`use`/`require` extraction already covers the dependency graph?**

---

## Evaluation

### engram

| Aspect | Measurement |
|---|---|
| Retrieval mode | SQLite+FTS5 lexical only — same class as tantivy BM25 already in-process (`PROJECT.md:138`, `ROADMAP.md:148` uses “Optional, fail-open” label) |
| Incremental gain | No measurable Recall@10 / MRR lift versus BM25 + symbol/path tokenization in eShopOnWeb 48-task bench; deterministic 2s CLI search simply duplicated the LIKE fallback path it fell back to on timeout |
| Operational cost | Extra Go binary lifecycle (spawn/health-check/timeout), HTTP client path (`knowledge/src/engram.rs:1-317`), `~/.coderun/bin` unification + `PATH` probing, `doctor` probe, env `CODERUN_ENGRAM_ENDPOINT` |
| Config cost | `[knowledge]` grows with `memory_enabled`, `memory_endpoint` (deprecated), `engram_binary_path` |

### codebase-memory-mcp

Reuse of the same 48-task eShopOnWeb benchmark that decided `FLASHRANK_REMOVAL.md:25`:

| Configuration | Recall@5 | Recall@10 | MRR | Latency |
|---|---:|---:|---:|---:|
| Baseline BM25 | 16.97% | 20.19% | **0.5003** | 507ms |
| + codebase-memory-mcp | 16.97% | 20.19% | 0.5003 | 510ms |

*Zero gain* at every cutoff, +3ms latency, and a 10s timeout budget on the hot path. The same file that showed index-time representation (PascalCase splitting, symbol fields, path tokenization) yielding `+7.11pp / +41.9%` with *negative* latency (see `FLASHRANK_REMOVAL.md:52`) confirmed that local AST+regex was sufficient. Graph edges were already persisted to `003_graph.sql` and the local `graph.rs` fallback produced edges even when the probe returned `None`.

---

## Analysis

### Why engram was removed

1. **Redundant persistence.** `coderun-storage` already owns SQLite WAL + tantivy BM25 for all knowledge/docs/code. FTS5 lexical recall duplicates that pipeline without adding semantic capability (explicitly deferred in `PROJECT.md:138` / `SCOPE.md:199`).
2. **Operational complexity.** External Go binary, CLI subprocess per search (`engram search --json`), HTTP 2s timeout, fail-open `LIKE` branch, install/uninstall tar handling, `~/.coderun/bin` unification, and PATH probes expand install/doctor surface.
3. **Already fail-open to local.** `try_engram_search` primary `2s` timeout immediately falls back to `db.search_memory() LIKE` — the fallback *is* the implementation. Making the fallback the implementation removes a network-shaped hop.
4. **Excluded already.** `coderun init` seeds with `memory_enabled: false` and `INDEXING_PERF_PLAN` scope excludes engram — the daemon hot path is the only remaining call site.
5. **Minimal v1 principle.** `PRINCIPLES.md:83` (Minimal Runtime) + `PRINCIPLES.md:56` (Local-First) favor in-process SQLite over an external sidecar when the sidecar adds no capability.

### Why codebase-memory-mcp was removed

1. **Redundant graph extraction.** Local `extract_imports()` (regex over `use`/`mod`/`import`/`require`) plus tree-sitter AST already builds the `DependencyGraph` adjacency used by `BuildContext` (`graph.rs:401`). The probe was “first-class with fallback” but the fallback was the product.
2. **Fragile Node dependency.** Probe requires `npx` / `npm` / Node, global `codebase-memory-mcp` install, and `~/.coderun/bin` binary — contradicts `PRINCIPLES.md:56` Local-First in-process stack and the workspace’s `ignore`/`ripgrep`/`sg-core` embedded crates.
3. **Measured zero recall gain.** Bench above + offline `eval/metrics/mcp_vs_local.py` both showed graph-enriched queries did not improve Recall or MRR over BM25+symbol/path baselines.
4. **Not on hot path.** Large-repo deferral and `INDEXING_PERF_PLAN` exclusion meant the probe never accelerated `coderun init`; it only added a 10s timeout risk on `build_from_files()` in the daemon.
5. **Security/install bloat.** `npm -g codebase-memory-mcp` + bundled tarballs expand download, supply-chain surface, and uninstall logic (`scripts/uninstall.ps1:289`, `uninstall.sh:104`).

---

## Decision

```text
engram               ──► removed (SQLite+tantivy is the memory)
codebase-memory-mcp  ──► removed (local AST+regex is the graph)
```

### Removed from

- **Runtime path** — `crates/coderun-knowledge/src/engram.rs` + `KnowledgeHub` engram branch, `[knowledge] memory_enabled/memory_endpoint/engram_binary_path`, `CODERUN_ENGRAM_ENDPOINT` env, `KnowledgeConfig::memory_enabled` gating, `try_engram_search` CLI path
- **Graph probe** — `graph.rs:discover_cbm_exe` / `try_codebase_memory_mcp_public` / `crates/coderun-cli/src/main.rs:discover_cbm_exe` / `index_codebase_memory`, deferred-graph probe in `cli init/index`, `DependencyGraph::build_from_files` npx branch (local fallback becomes primary)
- **Config & IPC** — `[knowledge]` shrinks to `max_knowledge_entries`; `doctor` Engram probe retires to historical note; adapter/lifecycle memory wiring simplified
- **Install/uninstall** — `engram` + `codebase-memory-mcp` download, `~/.coderun/bin` unification, npm global handling, bundled `.coderun/engram/*.tar.gz` + `.coderun/codebase-memory/*.tar.gz` handling
- **Eval/scripts** — `eval/metrics/mcp_vs_local.py`, `mcp_comparison.py`, `run_comparison.sh` / `run_4way.sh` CBM modes; `graph.rs` CLI remains optional only if re-introduced with proven gain

### Kept as

- **Historical record** — this ADR, `CHANGELOG.md`, `ROADMAP.md` external-integrations table, and `FLASHRANK_REMOVAL.md` reference row
- **Local fallbacks as primaries** — `db.search_memory()` LIKE path for memory categories and `extract_imports()` / AST graph for dependencies
- **Storage** — SQLite `knowledge` / `memory` tables and `003_graph.sql` edges persist; they are now written/read without external sidecars

---

## Consequences

1. **Simpler install & doctor.** No Go binary, no npm global, no 10s probe, no `CODERUN_ENGRAM_ENDPOINT`. `cargo build --release` + `scripts/install.ps1|.sh` copy only `coderun`/`coderun-daemon`.
2. **Smaller config.** `[knowledge]` loses three fields; `.coderun/config.toml` and env surface shrink (mirrors `DBOS→future/workflow` isolation).
3. **Faster & more reliable retrieval.** Knowledge path is purely in-process SQLite+tantivy BM25; graph path is purely in-process AST+regex — no subprocess spawn or timeout branch.
4. **Clearer ownership.** `coderun-storage` (SQLite WAL) is the sole persistence owner (`SCOPE.md:Data Boundary`); `coderun-repo-intel` owns the dependency graph (tree-sitter + `extract_imports`).
5. **Extensible.** If a future memory/graph technique shows measurable Recall@5/MRR gain on the `eval/datasets/eshop_tasks.yaml` bench, it can be re-proposed with an ADR referencing this one — same gate used for `FLASHRANK_REMOVAL.md`.

## References

- `crates/coderun-knowledge/src/engram.rs` — removed module (historical)
- `crates/coderun-knowledge/src/lib.rs:146` — `try_engram_search` call site (to become local)
- `crates/coderun-repo-intel/src/graph.rs:266` — removed probe (local fallback retained)
- `crates/coderun-core/src/config.rs:51` — removed `KnowledgeConfig` fields
- `docs/01-architecture/FLASHRANK_REMOVAL.md:25` — benchmark showing `+ codebase-memory-mcp` = 0pp gain
- `docs/INDEXING_PERF_PLAN.md:29` — prior exclusion from init hot path
- `eval/metrics/mcp_vs_local.py` — CBM vs local comparison harness (retired)
