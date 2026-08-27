# FlashRank Removal — Architectural Decision Record

**Date:** August 2026
**Status:** Accepted
**Decision:** FlashRank removed from v1 runtime path. Offline evaluation only.

---

## Context

FlashRank is a cross-encoder neural reranker that reorders BM25 search results using a small ONNX model (`rank-T5-flan`, int8 quantized). It was integrated into Coderun as an optional reranking step after BM25 candidate generation.

The question was: **does FlashRank improve retrieval enough to justify its runtime cost in v1?**

## Benchmark Evaluation

We ran a 48-task evaluation against the eShopOnWeb repository (a realistic ASP.NET e-commerce codebase) across three configurations:

### Results

| Configuration | Recall@5 | Recall@10 | MRR | Latency |
|---|---:|---:|---:|---:|
| Baseline BM25 | 16.97% | 20.19% | **0.5003** | **507ms** |
| + FlashRank | **18.94%** | 20.19% | 0.4325 | 8,532ms |
| + codebase-memory-mcp | 16.97% | 20.19% | 0.5003 | 510ms |

### Analysis

FlashRank provided:

- **+1.97 percentage points** Recall@5 (16.97% → 18.94%)
- **Zero improvement** Recall@10 (20.19% → 20.19%)

But it also caused:

- **-6.78 percentage points MRR** (0.5003 → 0.4325) — results were reranked *worse*
- **17x latency increase** (507ms → 8,532ms) — nearly 8 seconds added per query
- **Additional dependency** — `ort` crate, ONNX model file, tokenizer

### Why MRR degraded

FlashRank's cross-encoder scoring didn't align with what developers consider "relevant." When it reranked, it moved some correct results down and incorrect results up. The BM25 ranking was actually better for this codebase.

### Why latency was unacceptable

For a real-time coding agent context engine, 8.5 seconds per retrieval is far too slow. The entire context build should complete in under 1 second. FlashRank alone consumed 80× the latency budget.

## What actually improves retrieval

The same benchmark showed that **index-time representation** improvements beat any post-processing reranker:

| Improvement | Recall@5 | Cost |
|---|---:|---|
| Baseline BM25 | 16.97% | — |
| + PascalCase splitting | 22.19% (+5.22pp) | Zero runtime cost |
| + Symbol name field | 22.62% (+0.43pp) | Zero runtime cost |
| + Path tokenization | 24.08% (+1.46pp) | Zero runtime cost |
| **Total improvement** | **+7.11pp (+41.9%)** | **-7ms latency** |

These are deterministic, fast, and explainable. No neural reranker needed.

## Decision

```text
FlashRank
└── offline evaluation only
```

### Removed from

- **Runtime path** — `rerank.rs` is now a passthrough
- **Cargo dependencies** — `ort` feature removed from `coderun-knowledge`
- **Context engine** — FlashRank reranking section removed
- **Eval scripts** — `--rerank` flag removed
- **Benchmarks** — BM25+FlashRank benchmark replaced with BM25-only
- **Install/uninstall scripts** — model copy/removal removed

### Kept for

- **Module documentation** — `rerank.rs` explains the removal rationale with benchmark numbers
- **Historical records** — CHANGELOG, version plans, and architecture docs retain FlashRank references as historical context

## Consequences

1. **Simpler codebase** — no `ort` dependency, no model file management, no TF-IDF fallback logic
2. **Faster retrieval** — ~500ms instead of ~8.5s
3. **Better MRR** — 0.5003 instead of 0.4325
4. **Clearer architecture** — retrieval quality comes from index representation, not post-processing
5. **Extensible** — the `Reranker` struct remains as a passthrough; if a future reranker demonstrably helps, it can be re-added

## References

- `crates/coderun-knowledge/src/rerank.rs` — module docs with benchmark numbers
- `eval/run_comparison.sh` — the evaluation script
- `eval/metrics/retrieval.py` — the metrics implementation
- Benchmark dataset: `eval/datasets/eshop_tasks.yaml` (48 tasks)
