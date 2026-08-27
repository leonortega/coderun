# Context Engine Gaps — Implementation Plan

## Current State

The context engine (`crates/coderun-context/src/lib.rs`) already has:

| Component | Status | Location |
|-----------|--------|----------|
| Tantivy/BM25 retrieval | ✅ Implemented | `search_code_scored_standalone` |
| Symbol index (Tree-sitter) | ✅ Implemented | `search_structural` in repo-intel |
| RRF fusion | ✅ Implemented | `search_code_scored` lines 247-269 |
| Graph-based boosting | ✅ Implemented | `search_code_scored` lines 297-328 |
| MCP fallback (threshold) | ✅ Implemented | `search_code_scored` lines 330-357 |
| FlashRank reranker | ⚠️ Exists but not wired | `coderun-knowledge/src/rerank.rs` |

## The Gap

**FlashRank reranker is not invoked in the context assembly flow.**

Current flow:
```
BM25 + Symbols → RRF fusion → Graph boost → MCP fallback → Token budget
                                                    ↑
                                            Reranker NOT called here
```

Proposed flow:
```
BM25 + Symbols → RRF fusion → Graph boost → MCP fallback → FlashRank rerank → Token budget
```

## Implementation

### Step 1: Add Reranker to ContextConfig

Add reranker configuration to `ContextConfig`:

```rust
pub struct ContextConfig {
    pub max_tokens: usize,
    pub max_files: usize,
    pub max_lines_per_file: usize,
    pub cache_order: Vec<String>,
    pub reranker_enabled: bool,        // NEW
    pub reranker_max_candidates: usize, // NEW
}
```

### Step 2: Add Reranker to ContextEngine

```rust
pub struct ContextEngine {
    // ... existing fields ...
    reranker: Reranker,  // NEW
}
```

### Step 3: Convert SearchResult → RerankCandidate

Add conversion function:

```rust
fn search_result_to_rerank_candidate(result: &SearchResult) -> RerankCandidate {
    RerankCandidate {
        id: result.path.clone(),
        content: result.content.clone(),
        path: result.path.clone(),
        language: detect_language(&result.path),
        symbols: vec![],  // Could extract from content
        original_score: result.score as f32,
    }
}
```

### Step 4: Invoke Reranker in search_code_scored

After MCP fallback (line 357), before building final results:

```rust
// ── FlashRank reranking ──
if self.reranker.config.enabled && !merged.is_empty() {
    let candidates: Vec<RerankCandidate> = merged.iter()
        .map(|(path, score)| {
            let result = all_by_path.get(path).cloned().unwrap_or(...);
            search_result_to_rerank_candidate(&result)
        })
        .collect();
    
    let reranked = self.reranker.rerank(&query, candidates);
    
    // Rebuild merged from reranked order
    merged = reranked.iter().enumerate().map(|(i, c)| {
        (c.path.clone(), c.original_score as f64 / 1000.0 + (reranked.len() - i) as f64 * 0.001)
    }).collect();
}
```

### Step 5: Update search_code_scored signature

The function needs access to the reranker. Options:
- Pass `&Reranker` as parameter
- Make it a method on `ContextEngine` (already is)

Since `search_code_scored_standalone` is a static method, we need to either:
1. Make it non-static and use `self.reranker`
2. Pass reranker as parameter

**Chosen**: Pass `Option<&Reranker>` as parameter to keep parallelism.

## Files to Modify

1. `crates/coderun-context/src/lib.rs` — Main changes
2. `crates/coderun-knowledge/src/rerank.rs` — No changes needed (already complete)

## Testing

1. Unit test: Reranker integration with mock candidates
2. Integration test: Full context build with reranker enabled/disabled
3. Benchmark: Compare context quality with/without reranking

## Rollback

Feature flag via `reranker_enabled: false` in config. No behavior change when disabled.
