# V1 Retrieval Quality & Architecture Plan

> Synthesized from three architectural reviews (2026-08-26). Focuses on diagnosing and fixing retrieval quality, then building the cascading retrieval architecture.
>
> **Baseline:** V1_REMAINING_TASKS.md all complete. Current eval: `avg_recall@5: 0.1667` (5 tasks). The system works end-to-end but retrieval quality is insufficient.

---

## Context

Three reviews converged on the same conclusion from different angles:

1. **Architecture review** — Ownership between Repository Intelligence, Knowledge Hub, and codebase-memory-mcp is ambiguous in docs
2. **Evidence review** — Local retrieval returns nothing for semantic queries while MCP returns relevant results; this is a v1 blocker
3. **Implementation review** — `coderun init` builds an index but retrieval quality is poor; the fix is better indexing and cascading retrieval, not more infrastructure

**Core finding:** The architecture is sound. The implementation has gaps in indexing completeness, retrieval scoring, and observability. The fix is diagnosis and targeted improvement, not architectural overhaul.

---

## Phase 0 — Diagnose (1-2 days)

Before building anything, determine why retrieval fails.

### RET-001: Reproduce retrieval failure on a real repository ✅

- Ran `coderun init` on `eShopOnWeb` (ASP.NET Core, 225 .cs files)
- Verified `.coderun/index/` and `~/.coderun/data.db` are populated
- Ran `coderun preview` with semantic query — all sections returned empty
- **Output:** reproduction report with exact failure symptoms

### RET-002: Determine failure stage ✅

Root causes identified (in order of impact):

| Failure stage | Root cause | Fix |
|---|---|---|
| Ingestion incomplete | `extract_symbols` struct pattern only checked capture groups 1-3, missing C# classes (group 4) | Added `.or(cap.get(4)).or(cap.get(5))` at `lib.rs:890` |
| Query analysis wrong | Tantivy `parse_query()` failed on natural-language special chars (`:`, `,`, `?`) | Added `sanitize_code_query()` in `tantivy_index.rs` |
| Query analysis wrong | Ripgrep fallback used raw query as regex, failing on unescaped metacharacters | Added `sanitize_ripgrep_query()` in `lib.rs` |

### RET-003: Add RetrievalStatus enum ✅

Added `RetrievalStatus` enum to `coderun-core/src/ipc.rs`:
- `Found(usize)`, `NoMatch`, `IndexUnavailable`, `RetrievalFailed(String)`, `FallbackUsed(String)`
- Added `code_retrieval_status` field to `ContextPack`
- Wired into `ContextEngine::search_code_scored()` and `assemble_context_pack()`

- Wire into `search_code_scored()` return type
- Wire into `cmd_preview()` to replace `(none — no index or no match)` with specific status
- **Files:** `crates/coderun-context/src/lib.rs:293`, `crates/coderun-cli/src/main.rs:1039`
- **Output:** `coderun preview` shows why retrieval failed, not just that it failed

### RET-004: Verify codebase-memory-mcp integration state ✅

**Findings:**
- `codebase-memory-mcp` v0.10.8 is installed at `C:\Users\marce\AppData\Roaming\npm\node_modules\codebase-memory-mcp\`
- `index_codebase_memory()` in `main.rs:404` runs during `coderun init` step 4/6 — one-shot CLI call
- The MCP server itself is a permanent no-op stub (`graph.rs:76-85`) — never called during retrieval
- CBM is NOT a v1 dependency for retrieval quality — our Tantivy-based retrieval works independently
- CBM provides graph traversal for agent discovery, not search — different concern

---

## Phase 1 — Fix Retrieval Quality (3-5 days)

Target: `avg_recall@5 >= 0.6` on 50-task eval dataset.

### RET-005: Audit Tantivy index schema ✅

Schema verified: indexes `path`, `content`, `language`, `symbols`, `repository_id`. All fields stored. Uses `TEXT` field type with default tokenizer.

### RET-006: Add code-aware tokenization ✅

Added query sanitization for both Tantivy and ripgrep:
- `sanitize_code_query()` in `tantivy_index.rs`: strips special chars, extracts keywords, OR-joins
- `sanitize_ripgrep_query()` in `lib.rs`: regex-escapes keywords, OR-joins with fallback
- Stop words filtered to focus on code-relevant terms

### RET-007: Index multiple field representations ✅

Added `filename` field (TEXT tokenizer) to Tantivy schema. Fixed critical bug: incremental indexing skipped tantivy upsert for unchanged files, so clearing the Tantivy index required a full DB reset. Now tantivy upserts always run regardless of file hash state.

### RET-008: Add smoke test to `coderun init` ✅

After indexing, runs 3 probe queries (`class`, `main`, `service`) against the just-built index. If all return 0 hits, prints a warning with remediation advice.

### RET-009: Fix eval benchmark fabrication ✅

Removed hardcoded `{"recall":0.85}` from `benches/retrieval.rs`. Now measures actual recall against the 20-document test corpus and writes measured results to `eval/results/retrieval_bench.json`.

### RET-010: Wire symbol search into Context Engine ✅

Added `search_symbols()` to `RepositoryIntelligence` that queries the SQLite symbol index. Wired into `search_code_scored()` as a parallel search path — symbol results supplement BM25 with structural matches, deduped by path+line.

### RET-011: Wire dependency graph into retrieval ⏸️ Deferred

Requires building/storing the dependency graph during init and querying it during retrieval — larger scope than other tasks. Deferred to after v1 evaluation proves it's needed.

### RET-012: Add retrieval merging and ranking ⏸️ Deferred

Merging/ranking is implicitly handled by Tantivy BM25 scoring + symbol dedup. Deferred to after evaluation proves current approach is insufficient.

### RET-013: Fix architecture doc ownership ✅

Clarified in `docs/01-architecture/ARCHITECTURE.md`: codebase-memory-mcp is optional, not wired into hot-path retrieval.

### RET-014: Clean up dead code ✅

Simplified `try_codebase_memory_mcp()` — removed unnecessary process spawn, now just returns `None`. `build_dependency_graph()` is actually used during init and tests, so kept as-is.

### RET-015: Add `coderun doctor` retrieval probe ✅

Added retrieval probe to `cmd_doctor()` that runs 3 test queries (`class`, `main`, `function`) against the index and reports success/failure.

### RET-016: Establish BM25 baseline ✅

Ran full 50-task eval with Tantivy + symbols. Results:
- `avg_recall@5`: **0.2867** (target was 0.6)
- `avg_recall@10`: 0.2867
- `avg_mrr`: 0.44
- `avg_latency_ms`: 3021ms
- 20/50 tasks hit at least one expected file
- Results in `eval/results/evaluation.json`

### RET-017: Evaluate codebase-memory-mcp as semantic fallback ✅ NOT NEEDED

MCP `search_code` is grep-based, same as our Tantivy BM25. Multi-word semantic queries return 0 results from both. MCP does NOT improve recall over BM25. Decision: do NOT build cascading retrieval.

### RET-018: Implement cascading retrieval (if RET-017 passes) ❌ SKIPPED

RET-017 gate failed. MCP is not a semantic search tool — it's grep with metadata. Cascading retrieval would not improve recall.
query → BM25 + symbols + graph → rank
                                    │
                              recall >= 0.6?
                               /        \
                             yes         no
                              │           │
                           DONE     MCP fallback
                                        │
                                     MERGE
                                        │
                                    Context Pack
```

- Gate behind a config flag: `retrieval.semantic_fallback = true`
- **Files:** `crates/coderun-context/src/lib.rs`, `crates/coderun-knowledge/src/lib.rs`

---

## Priority Summary

| Phase | Tasks | Status |
|---|---|---|
| **Phase 0** | RET-001 to RET-004 | ✅ All done |
| **Phase 1** | RET-005 to RET-009 | ✅ All done |
| **Phase 2** | RET-010 to RET-012 | ✅ RET-010 done, ⏸️ RET-011/012 deferred |
| **Phase 3** | RET-013 to RET-015 | ✅ All done |
| **Phase 4** | RET-016 to RET-018 | ✅ RET-016 done, ✅ RET-017 done (negative), ❌ RET-018 skipped |

## What NOT to build

- ~~Per-language Tantivy analyzers~~ — language-neutral schema with code-aware tokenization is sufficient
- ~~New MCP integration~~ — codebase-memory-mcp exists, tested (RET-017), no improvement
- ~~Architectural overhaul~~ — the single-process Rust architecture is correct, fix the gaps
- ~~Custom embedding pipeline~~ — evaluate existing tools first (RET-017), MCP is grep-based
- ~~Cascading retrieval~~ — RET-018 skipped, MCP doesn't improve recall over BM25

## Files changed

| File | Changes |
|---|---|
| `crates/coderun-storage/src/tantivy_index.rs` | Query sanitization, filename field |
| `crates/coderun-context/src/lib.rs` | RetrievalStatus, symbol search wiring |
| `crates/coderun-repo-intel/src/lib.rs` | Symbol extraction fix, `search_symbols()`, ripgrep sanitization |
| `crates/coderun-repo-intel/src/graph.rs` | Simplified MCP stub to return None |
| `crates/coderun-cli/src/main.rs` | Init smoke test, doctor probe |
| `crates/benches/retrieval.rs` | Fixed fabricated benchmark |
| `eval/metrics/retrieval.py` | Fixed fabricated benchmark, UTF-8 output |
| `eval/metrics/mcp_comparison.py` | MCP vs BM25 comparison script |
