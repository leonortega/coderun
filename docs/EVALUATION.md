# Evaluation Framework

This document describes the evaluation framework for Knocode AI Runtime.

## Overview

The evaluation framework tests two core capabilities:

1. **Context Retrieval Quality** — Does BuildContext retrieve the files actually needed?
2. **Model Routing Accuracy** — Does the router select the appropriate tier?

## Quick Start

```bash
# Run all evaluations
./eval/run-evaluation.sh

# Run specific evaluation
./eval/run-evaluation.sh model    # Model routing only
./eval/run-evaluation.sh context  # Context quality only

# Compare BM25 vs MCP vs FlashRank (historical — FlashRank/MCP removed from v1 runtime, see FLASHRANK_REMOVAL.md / ENGRAM_CBM_REMOVAL.md)
./eval/run_comparison.sh  # offline eval only

# View results in web UI
npx promptfoo view -c eval/promptfooconfig.yaml
```

## Evaluation Types

### 1. Context Retrieval Quality

Tests that the Context Engine retrieves relevant files for real coding tasks.

**Dataset:** `eval/datasets/repository_tasks.yaml` (50 tasks against eShopOnWeb)
**Metrics:** `eval/metrics/retrieval.py`
**Provider:** `eval/providers/context-quality.js` (UDS MessagePack primary)

| Metric | Target | Description |
|--------|--------|-------------|
| Recall@5 | ≥ 0.4 | % of expected files in top 5 results |
| Recall@10 | ≥ 0.5 | % of expected files in top 10 results |
| MRR | ≥ 0.4 | Mean reciprocal rank of first relevant result |
| Latency | < 2s | Average retrieval time |

### 2. Model Routing Accuracy

Tests that the Model Router correctly selects the appropriate tier.

**Dataset:** `eval/datasets/model-routing.yaml`
**Provider:** `eval/providers/model-routing.js`

| Task Type | Expected Tier | Examples |
|-----------|---------------|----------|
| Simple | fast | Fix typo, add comment, rename variable |
| Moderate | balanced | Add API endpoint, implement middleware |
| Complex | capable | Refactor architecture, implement OAuth2 |

### 3. Retrieval Comparison

Compares different retrieval strategies head-to-head.

```bash
./eval/run_comparison.sh
```

Compares: BM25 only vs BM25 + FlashRank vs MCP fallback *(historical offline eval — v1 runtime is BM25-only, FlashRank/MCP removed)*.

## Evaluation Scripts

| Script | Purpose |
|--------|---------|
| `eval/run-evaluation.sh` | Main eval runner (model routing + context quality) |
| `eval/run_comparison.sh` | BM25 vs MCP vs FlashRank comparison |
| `eval/run_4way.sh` | 4-way retrieval comparison |
| `eval/metrics/retrieval.py` | Recall@5/10, MRR, latency, duplicate ratio |
| `eval/metrics/mcp_comparison.py` | MCP vs BM25 comparison |
| `eval/metrics/mcp_vs_local.py` | MCP vs local retrieval |

## Datasets

| Dataset | Tasks | Purpose |
|---------|-------|---------|
| `repository_tasks.yaml` | 50 | Real coding tasks against eShopOnWeb (ASP.NET) |
| `eshop_tasks.yaml` | 48 | eShopOnWeb-specific tasks for retrieval eval |
| `model-routing.yaml` | varies | Model routing accuracy tests |
| `context-quality.yaml` | varies | Context quality tests |
| `expected_context.yaml` | — | Expected files per task |

## Baseline Benchmark

`eval/baseline/run.py` measures with and without Knocode:

- Task success rate
- Input/output/tool tokens
- Latency and cost
- Context recall

## Adding New Tests

### Retrieval Tests

Add to `eval/datasets/repository_tasks.yaml`:

```yaml
- task: "Description of the coding task"
  expected_files:
    - src/path/to/file.rs
    - src/other/file.rs
  category: bug_fix  # or feature, refactor, etc.
```

### Model Routing Tests

Add to `eval/datasets/model-routing.yaml`:

```yaml
- description: "New test case"
  vars:
    task: "description of the task"
    file_count: 5
    symbol_count: 20
    knowledge_entries: 3
    skills_matched: 1
    token_count: 2000
  assert:
    - type: equals
      value: "balanced"
```

## Interpreting Results

### Console Output

```
✓ Model Routing: 11/11 passed (100%)
✓ Context Quality: 9/9 passed (100%)
```

### JSON Results

Results are saved to `eval/results/`:
- `evaluation.json` — Full evaluation results with Recall@5/10, MRR, latency
- `retrieval_bench.json` — Retrieval benchmark results

## Current Results

| Metric | Value | Target | Notes |
|--------|-------|--------|-------|
| avg_recall@5 | ~0.29 | ≥ 0.4 | Limited by vocabulary mismatch |
| avg_mrr | ~0.44 | ≥ 0.4 | On target |
| avg_latency_ms | ~3000 | < 2000 | Needs optimization |
| Tool compression | −67% | ≥ 50% | RTK + built-in compressors |

## Troubleshooting

### Daemon Not Running

```bash
# Start the daemon
knocode serve

# Check health
curl http://127.0.0.1:9527/health
```

### Tests Failing

1. Check the daemon is running and indexed the repo
2. Run `knocode doctor` to verify all components
3. Check `eval/results/` for detailed error logs
4. Run with `--nocapture` for detailed Rust test output
