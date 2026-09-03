# Knocode v1 Plan — Repository-Aware Context Layer

> **Principle:** Knocode does not compete with `rg`, `grep`, `read`, or `glob`. It orchestrates them and adds semantic/context capabilities above them.
> See `docs/00-project/SCOPE.md:15`, `docs/01-architecture/ARCHITECTURE.md:73`, `docs/01-architecture/FLASHRANK_REMOVAL.md:5`, `docs/01-architecture/ENGRAM_CBM_REMOVAL.md:5`.

This plan distills the DefinitelyTyped (53,832 files / 37,370 symbols / 133.8s bootstrap) and eShopOnWeb benchmarks.

## 1. What the benchmark tells us

- `rg` ~1001 ms vs `knocode` ~5561 ms on literal search → **5.5x slower is expected**. `rg` is the correct tool for literal search. Do not optimize Knocode to beat it (`crates/knocode-storage/src/tantivy_index.rs:593`, `crates/knocode-repo-intel/src/lib.rs:622`).
- Bootstrap 133.8s for 53k files is acceptable **iff** it is one-time and incremental (`crates/knocode-repo-intel/src/lib.rs:164`, `lib.rs:225` `is_file_unchanged_fast`, `lib.rs:324` batch commits).
- `preview` BM25 ~4201 ms is too high. Agent doing 20 context requests pays 110s overhead. Must be milliseconds, not seconds.

## 2. V1 Product Boundary

**Existing tools (excellent, keep):**

```
rg / grep, glob, read, bash, git
```

**Knocode specializes in:**

```
repository indexing (tree-sitter 111 langs via arborium)
symbol extraction (PascalCase/camelCase decomposition)
code-aware search (field-aware BM25)
candidate generation (candidateK)
deterministic ranking + graph expansion (optional, file-level)
context selection + token budgeting → Context Pack
```

Single public API: `BuildContext(task)` `crates/knocode-context/src/lib.rs:426`.

## 3. Priorities

### P0 — Fix retrieval latency

Profile decomposition (instrumented via `KNOCODE_PROFILE=1`):

```
cmd_preview  crates/knocode-cli/src/main.rs:1076
  → search_fulltext  crates/knocode-repo-intel/src/lib.rs:622
    → TantivyIndex::open  crates/knocode-storage/src/tantivy_index.rs:394 (MmapDirectory)
    → reader()  crates/knocode-storage/src/tantivy_index.rs:423 (ReloadPolicy::OnCommitWithDelay)
    → parse_query  crates/knocode-storage/src/tantivy_index.rs:626
    → searcher.search  crates/knocode-storage/src/tantivy_index.rs:664
    → materialization + file_class_boost/directory_boost  crates/knocode-storage/src/tantivy_index.rs:708
```

Fixes (in order):

1. Gate per-query `build_dependency_graph()` `crates/knocode-context/src/lib.rs:333` — skip if `doc_count>5000` unless `KNOCODE_BUILD_GRAPH=1` (matches init deferral `crates/knocode-cli/src/main.rs:311`). Walk of 53k files per query is the likely 4.2s contributor.
2. Reuse `IndexReader`/`Searcher` — avoid per-query `MmapDirectory::open` + `Index::open`.
3. Avoid STORED `content_field` for 60 candidates (`fetch_limit = max*3` `crates/knocode-storage/src/tantivy_index.rs:663`) — load file content only for final Top 20.

Target: `preview` < 1s on 53k repo; `BuildContext` p95 < 50ms on small repos (see `benches/context_bench.rs`).

### P0 — Incremental indexing

After `knocode init`, changing 5 files must not rebuild 53k.

- Already: `is_file_unchanged_fast` `crates/knocode-repo-intel/src/lib.rs:1001`, `KNOCODE_INDEX_THREADS`, `begin_batch`/`commit_batch`.
- Verify: cold 133.8s is one-time; warm re-index touches only changed files (`KNOCODE_INDEX_DIR` isolation for tests).

### P0 — Code-aware indexing (field-aware retrieval)

Fix queries like `"find type NextFunction"` without giant OR expansion:

- `sanitize_code_query` `crates/knocode-storage/src/tantivy_index.rs:332` already filters `find`/`type` via `STOP_WORDS` `tantivy_index.rs:119`.
- Boosts: `symbol_name` 3.0×, `path` 2.5×, `symbols` 2.0×, `filename` 2.0×, `content` 1.0× `tantivy_index.rs:610-624`.
- Preprocessing: `split_pascal_case` `tantivy_index.rs:138`, `preprocess_code_content` `tantivy_index.rs:205`, `tokenize_path` `tantivy_index.rs:433`.
- `NextFunction` scores high on `symbol_name:NextFunction`, not on noisy `find`/`type` in content.
- Do NOT re-introduce large `expand_code_vocabulary` `tantivy_index.rs:229` or LLM expansion — diluted recall per benchmark.

### P1 — Candidate pool evaluation

```
Tantivy → Top 50/100 candidates → deterministic scorer → Top 20 → token budget → Context Pack
```

- Make `candidateK` configurable (test 20/50/100/200) before fixing default. Never return 100 files to model (`ContextConfig.max_files = 20` `crates/knocode-context/src/lib.rs:71`).
- Dataset: `eval/datasets/repository_tasks.yaml` (50 tasks), metrics `eval/metrics/retrieval.py` (`Recall@5`, `MRR`).

### P1 — Path/filename weighting

- `file_class_boost` `crates/knocode-storage/src/tantivy_index.rs:68` (Source 1.5× vs Config 0.3×) + `directory_boost` `tantivy_index.rs:86` (domain/infrastructure vs views/wwwroot). Extend for large repos (e.g. `express/...`).

### P1 — File graph (modest, optional)

- `DependencyGraph` `crates/knocode-repo-intel/src/graph.rs:4` — nodes=files, edges=`file A imports file B`. Deferred on >5k files (`crates/knocode-cli/src/main.rs:311`). Benchmark only after basic retrieval is fast. Currently 0 edges = unmeasured, not failed.

## 4. Not in v1

- FlashRank / custom reranker / `ort` — removed (`crates/knocode-knowledge/src/rerank.rs:3`, `FLASHRANK_REMOVAL.md:24` 18.94% R@5 but 8532 ms, worse MRR). Offline eval only.
- Giant synonym dictionaries, LLM query expansion, vector DB.
- `codebase-memory-mcp` / `engram` — removed (`ENGRAM_CBM_REMOVAL.md:52` equal R@5 16.97%). Keep abstraction, don't depend.
- Replacing `rg`/`read`/`glob`/`bash`.

## 5. Architecture

```
                         CODING AGENT
                              │
              ┌───────────────┼────────────────┐
              │               │                │
             read            rg              bash
              │               │                │
              └───────────────┼────────────────┘
                              │ direct repository work
                              ▼
                         ┌─────────┐
                         │ KNOCODE │
                         └────┬────┘
                              │ BuildContext
                              ▼
                     Query Understanding
                              │
                              ▼
                    ┌──────────────────┐
                    │ Repository Index │
                    │ Tantivy BM25     │
                    │ Symbols (tree-   │
                    │ sitter + regex)  │
                    │ Paths/Files      │
                    └────────┬─────────┘
                             │ candidateK 50/100
                             ▼
                    Deterministic Ranking
                             │
                             ▼
                      Graph Expansion (optional, file imports only)
                             │
                             ▼
                     Context Selection (Top 20)
                             │
                             ▼
                      Token Budgeting (tiktoken-rs cl100k_base)
                             │
                             ▼
                       Context Pack (YAML, FROZEN PREFIX END, dedup)
```

## 6. Evolution steps

1. Add profiling harness for DefinitelyTyped clone (`KNOCODE_PROFILE=1 knocode preview`).
2. Apply P0 latency fixes (graph gating done, reader caching, STORED field reduction).
3. Verify field-aware scoring on `NextFunction` + eShopOnWeb 48-task eval.
4. Sweep `candidateK` 20/50/100/200.
5. Re-enable file graph only for small repos or `KNOCODE_BUILD_GRAPH=1` and measure lift.
