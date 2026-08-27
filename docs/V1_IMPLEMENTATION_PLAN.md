# Coderun v1 Implementation Plan

> **Source of truth:** This document defines the target v1 architecture and the implementation plan to reach it.
>
> **Key principle:** Keep the good concrete implementation, correct the architecture/documentation where the diagram was too ambitious, and implement only the missing behavior that is essential to our v1 goal.

## v1 Goal

> **Given a real repository and a natural-language development task, Coderun must reliably build a small, relevant Context Pack from the repository before asking the model to work.**

If we can make this work reliably, everything else—router, skills, Engram, RTK, MCP fallback—is supporting infrastructure rather than the core product.

---

## Target Architecture

```
                              CODING AGENT
                                   │
                                   │
                              Coderun API
                                   │
                                   ▼
                         ┌───────────────────┐
                         │   Coderun Daemon  │
                         └─────────┬─────────┘
                                   │
                                   ▼
                         ┌───────────────────┐
                         │   Context Engine  │
                         └─────────┬─────────┘
                                   │
                  ┌────────────────┼────────────────┐
                  │                │                │
                  ▼                ▼                ▼
          Repository Intel    Knowledge Hub       Skills
                  │                │                │
        ┌─────────┼──────┐         │                │
        │         │      │         │                │
        ▼         ▼      ▼         ▼                ▼
   Tree-sitter Symbols Graph     SQLite        Skill registry
        │         │      │         │
        └─────────┼──────┘         │
                  │                │
                  ▼                ▼
             Repository       Engram
              metadata        optional
                  │
                  ▼
             Tantivy/BM25
                  │
                  │
                  └───────────────┐
                                  │
                           local retrieval
                                  │
                                  ▼
                           ┌─────────────┐
                           │ Merge/Rank  │
                           └──────┬──────┘
                                  │
                          insufficient?
                              │      │
                             NO     YES
                              │      │
                              │      ▼
                              │ codebase-memory-mcp
                              │      │
                              │ semantic fallback
                              │      │
                              └──┬───┘
                                 ▼
                           Context Pack
                                 │
                                 ▼
                           Model Router
                                 │
                                 ▼
                              LiteLLM
                                 │
                                 ▼
                              Model
                                 │
                                 ▼
                           Coding Agent
```

### Repository Initialization

```
                       coderun init
                            │
                            ▼
                    Repository Detector
                            │
                            ▼
                    Language Detection
                            │
                            ▼
                    Parser Registry
                            │
                            ▼
                       Tree-sitter
                            │
                            ▼
                    Symbols + Graph
                            │
                            ▼
                       Tantivy index
                            │
                            ▼
                    Knowledge initialization
                            │
                            ▼
                     Validation / Doctor
                            │
                            ▼
                      REPOSITORY READY
```

### Command Execution

```
                      Coding Agent
                           │
                           ▼
                     Command Adapter
                           │
                     ┌─────┴─────┐
                     ▼           ▼
                    RTK       fallback
                  external     normal
                   binary      command
                     │           │
                     └─────┬─────┘
                           ▼
                      command output
                           │
                           ▼
                      Coding Agent
```

---

## Current State vs Target

| Component | Current State | Target State | Gap |
|-----------|---------------|--------------|-----|
| Retrieval | ✅ Parallel via `tokio::task::spawn_blocking` (code + knowledge + skills) | Parallel via `tokio::task::spawn_blocking` | **DONE** |
| Init pipeline | ✅ 7 steps with structured validation | 7 steps with structured validation | **DONE** |
| RetrievalStatus | ✅ `Found`/`NoMatch`/`IndexNotBuilt`/`ParserFailed`/`KnowledgeHubUnavailable`/`FallbackUsed` | Distinguish no-match from index/parser failure | **DONE** |
| SQLite | ✅ Documented as persistence backbone | Documented as persistence backbone | **DONE** |
| File-level graph | Implemented | Keep for v1 | **KEEP** |
| Engram | Optional, fail-open | Keep optional | **KEEP** |
| RTK | External binary adapter | Keep as-is, document correctly | **KEEP** |
| Parser registry | Implicit, lazy | Explicit, extensible | **P1** |
| MCP fallback | Scaffolded, not implemented | Implement as semantic fallback | **P1** |
| Call graph | Not implemented | Consider for v2 | **P2** |

---

## Implementation Plan

### P0: Critical Path Items

#### 1. Parallel Retrieval in Context Engine

**Status:** ✅ DONE

**Current state:** `build_context()` is `async fn` and uses `tokio::task::spawn_blocking` to run code search (repo lock), knowledge (kh lock), and skills (kh lock) in parallel.

**Target (achieved):**
```rust
let (code_result, knowledge_result, skills_result) = tokio::join!(
    self.search_code_scored(...),
    self.retrieve_knowledge_scored(...),
    self.match_skills_scored(...)
);
```

**Changes made:**
- Changed `build_context` from `pub fn` to `pub async fn`
- Updated `IContextBuilder` trait to use `#[async_trait]` async
- Refactored `search_code_scored`/`retrieve_knowledge_scored`/`match_skills_scored` into standalone associated functions (`*_standalone`) that don't require `&self`, enabling `spawn_blocking`
- Updated all callers (daemon HTTP handler, CLI preview, adapter, lifecycle) to use `.await`
- CLI wraps call in `tokio::runtime::Runtime::block_on`
- Benchmarks use `tokio::runtime::Runtime::new().block_on(...)`

**Files:**
- `crates/coderun-context/src/lib.rs` — `build_context()`, `IContextBuilder` trait
- `crates/coderun-core/src/lib.rs` — `IContextBuilder` trait definition
- `crates/coderun-daemon/src/http_server.rs` — HTTP handler calls `build_context`
- `crates/coderun-cli/src/main.rs` — `cmd_preview()` calls `build_context`

**Priority:** P0

---

#### 2. Real Initialization Pipeline

**Current state:** `cmd_init()` has 6 steps but skips tree-sitter validation, Knowledge Hub initialization, skill loading, and structured validation.

**Target pipeline:**
```
[1/7] Scaffold
[2/7] Repository discovery + language detection
[3/7] Parser registry validation (verify all expected grammars load)
[4/7] Indexing (symbols + graph + Tantivy)
[5/7] Knowledge Hub initialization + skill loading
[6/7] Validation queries (smoke test all components)
[7/7] Repository status report
```

**Status:** ✅ DONE

**Changes made:**
- Added explicit tree-sitter grammar validation step using `coderun_repo_intel::parser::validate_grammar()`
- Instantiated `KnowledgeHub` during init, called `load_skills_from_dirs()`
- Added structured validation queries for each component (Tantivy via `validate_index()`, SQLite symbol count, graph edges, knowledge entries, skills loaded)
- Added final status report table (READY/PARTIAL)
- Extracted `RepositoryIntelligence::validate_index()` and `parser::validate_grammar()` as reusable library functions

**Files:**
- `crates/coderun-cli/src/main.rs` — `cmd_init()` orchestrator
- `crates/coderun-repo-intel/src/lib.rs` — `validate_index()` method
- `crates/coderun-repo-intel/src/parser.rs` — `validate_grammar()` function

**Priority:** P0

---

#### 3. Distinguish No-Match from Index/Parser Failure

**Status:** ✅ DONE

**Current state:** When retrieval returns empty, `build_context` returns structured `RetrievalStatus` distinguishing `IndexNotBuilt`/`IndexUnavailable`/`ParserFailed`/`KnowledgeHubUnavailable`/`FallbackUsed`.

**Target (achieved):**
```rust
pub enum RetrievalStatus {
    Found(usize),
    NoMatch,
    IndexNotBuilt,
    IndexUnavailable,
    ParserFailed(Vec<String>),  // list of failed languages
    KnowledgeHubUnavailable,
    RetrievalFailed(String),
    FallbackUsed(String),
}
```

**Changes made:**
- Added new variants to `RetrievalStatus` (`IndexNotBuilt`, `IndexUnavailable`, `ParserFailed`, `KnowledgeHubUnavailable`, `RetrievalFailed`, `FallbackUsed`)
- In `search_code_scored_standalone`, proactive check via `repo_intel.validate_index()` before searching → returns `IndexNotBuilt` or `IndexUnavailable`
- In `retrieve_knowledge_scored_standalone`, proactive check via `knowledge_hub.is_initialized()` → returns empty (logged) if hub not seeded
- Propagated structured status to `ContextPack.code_retrieval_status` for debugging

**Files:**
- `crates/coderun-core/src/ipc.rs` — `RetrievalStatus` enum
- `crates/coderun-context/src/lib.rs` — `search_code_scored_standalone()`, `retrieve_knowledge_scored_standalone()`
- `crates/coderun-repo-intel/src/lib.rs` — `validate_index()`
- `crates/coderun-knowledge/src/lib.rs` — `is_initialized()`, `Database::count_knowledge()`

**Priority:** P0

---

#### 4. SQLite as Persistence Backbone (Documentation)

**Current state:** SQLite stores files, symbols, knowledge, sessions — but architecture diagram doesn't show it.

**Target:** Update architecture documentation to show:
```
SQLite → system metadata/state/persistence
Tantivy → search index
Tree-sitter → parsing
Graph → relationships
```

**Changes required:**
- Update `docs/01-architecture/ARCHITECTURE.md` with corrected diagram
- Add comments in `crates/coderun-storage/src/lib.rs` explaining role
- No code changes needed — already implemented

**Priority:** P0

---

### P1: Important Items

#### 5. Parser Registry Extensibility

**Current state:** Language support is implicit via `LANGUAGE_REGISTRY` static and `get_ts_language()`. Adding a language requires modifying `registry.rs`.

**Target:**
```
Language Detector
       │
       ▼
Parser Registry
       │
 ┌─────┼──────┐
 ▼     ▼      ▼
C#    TS    Python
```

**Changes required:**
- Make `ParserRegistry` a struct with `register_language()` method
- Move grammar loading to registry (not lazy per-file)
- Add `list_available_languages()` for init reporting
- **Priority:** P1 — current approach works, this is for extensibility

---

#### 6. MCP Semantic Fallback

**Current state:** `try_codebase_memory_mcp()` always returns `None` — not implemented.

**Target:**
```
Local retrieval → good enough? → YES → done
                              → NO → codebase-memory-mcp → merge
```

**Changes required:**
- Implement `try_codebase_memory_mcp()` to call external MCP server
- Add threshold: only use MCP if local retrieval returns < 3 results
- Merge MCP results with local results
- **Priority:** P1 — only after local retrieval is reliable

---

#### 7. Retrieval Ranking Improvements

**Current state:** BM25 + symbols + RRF fusion works but limited by vocabulary mismatch.

**Target:** Merge symbols + BM25 + graph results with better scoring.

**Changes required:**
- Add graph-based boosting: files connected to BM25 results get boost
- Implement code-behind pairing (`.cshtml` → `.cshtml.cs`)
- Consider embeddings for vocabulary mismatch (long-term)
- **Priority:** P1 — current approach is generalizable, improvements are incremental

---

### P2: Future Items

#### 8. Call Graph

- Only after evaluation proves file-level graph is insufficient
- Would require deeper AST analysis
- **Priority:** P2 — file-level graph sufficient for v1

---

## Implementation Order

| Week | Focus | Items |
|------|-------|-------|
| 1 | Parallel retrieval | P0 #1 |
| 2 | Init pipeline | P0 #2 |
| 3 | RetrievalStatus | P0 #3 |
| 4 | Documentation | P0 #4 |
| Later | Extensibility | P1 items as needed |

---

## Evaluation Results

### Current State (v0.75)

| Metric | Value | Target |
|--------|-------|--------|
| avg_recall@5 | 0.2396 | 0.6 |
| avg_recall@10 | 0.2535 | — |
| avg_mrr | 0.3882 | — |
| avg_latency_ms | 2180 | < 2000 |

### Retrieval Quality Progression

| Version | recall@5 | Change | Notes |
|---------|----------|--------|-------|
| Baseline | 0.182 | — | Initial BM25 only |
| + Filename field | 0.193 | +6% | Added filename to Tantivy schema |
| + File-class boost | 0.210 | +9% | Source=1.5x, Config=0.3x |
| + Field boosts | 0.210 | — | symbols=2.0x, path=1.5x |
| + Symbol-match boost | 0.210 | — | STOP_WORDS filtering |
| + RRF fusion | 0.210 | — | BM25 + symbols merged |
| + CSS exclusion | 0.240 | +14% | Stylesheet files excluded |
| + Directory boost | 0.240 | — | ApplicationCore=1.3x |
| + Junk filter | 0.240 | — | Invalid paths filtered |

### Key Learnings

1. **BM25 cannot handle vocabulary mismatch** — users say "wishlist", code has "basket"
2. **Hardcoded synonyms work but are not generalizable** — project-specific
3. **Dynamic fallback adds noise** — filename matching, directory expansion make results worse
4. **The 0.6 recall@5 target requires embeddings** — semantic similarity that bridges vocabulary gaps
5. **For v1, BM25 + symbols + RRF is the best generalizable approach**

---

## Success Criteria

For v1 to be considered complete:

1. ✅ `coderun init` produces a clear READY/NOT READY status
2. ✅ `coderun preview` returns results in < 2s (with parallel retrieval)
3. ✅ Retrieval status distinguishes "no match" from "index not built"
4. ✅ Architecture documentation matches implementation
5. ⬜ Average recall@5 ≥ 0.4 on eShopOnWeb eval (current: 0.24)

> **Note:** The 0.6 recall@5 target requires embeddings, which is a v2 feature. The v1 target is 0.4, which is achievable with BM25 + symbols + RRF + parallel retrieval.

---

## Appendix: Files to Modify

### P0 Changes

| File | Change |
|------|--------|
| `crates/coderun-context/src/lib.rs` | Async `build_context`, parallel retrieval, RetrievalStatus |
| `crates/coderun-core/src/lib.rs` | Async `IContextBuilder` trait |
| `crates/coderun-core/src/ipc.rs` | Extended `RetrievalStatus` enum |
| `crates/coderun-daemon/src/http_server.rs` | Async handler |
| `crates/coderun-cli/src/main.rs` | Init pipeline, async preview |
| `crates/coderun-repo-intel/src/lib.rs` | `validate_index()` method |
| `crates/coderun-repo-intel/src/parser.rs` | `validate_grammar()` function |
| `docs/01-architecture/ARCHITECTURE.md` | Corrected diagram |

### P1 Changes

| File | Change |
|------|--------|
| `crates/coderun-repo-intel/src/registry.rs` | Extensible parser registry |
| `crates/coderun-repo-intel/src/lib.rs` | MCP fallback integration |
| `crates/coderun-context/src/lib.rs` | Graph-based boosting |
