#!/bin/bash
# Retrieval Quality Comparison: BM25 vs codebase-memory-mcp
# Runs the 48-task eval dataset across two configurations and outputs a comparison table.
#
# Usage: ./run_comparison.sh [eshop_repo_path]
#
# Note: FlashRank was removed from the v1 runtime path per benchmark evaluation.
# See crates/coderun-knowledge/src/rerank.rs for rationale.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="${1:-C:/LeonRepository/eShopOnWeb}"
BINARY="target/release/coderun.exe"
RESULTS_DIR="$SCRIPT_DIR/results"

# Ensure binary exists
if [ ! -f "$BINARY" ]; then
    echo "ERROR: Binary not found at $BINARY — run 'cargo build --release -p coderun-cli' first"
    exit 1
fi

# Ensure eShopOnWeb is indexed
echo "=== Checking eShopOnWeb index ==="
if [ ! -d "$REPO/.coderun" ]; then
    echo "Indexing eShopOnWeb..."
    cd "$REPO" && "$BINARY" init 2>&1 | tail -5
fi

mkdir -p "$RESULTS_DIR"

echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║       Retrieval Quality Comparison (48-task eShopOnWeb)     ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# ── Configuration 1: Baseline BM25 ────────────────
echo "━━━ [1/2] Baseline BM25 (no MCP graph) ━━━"
CODERUN_MCP_ENABLED=false \
    python3 "$SCRIPT_DIR/metrics/retrieval.py" \
        --dataset "$SCRIPT_DIR/datasets/eshop_tasks.yaml" \
        --repo "$REPO" \
        --binary "$BINARY" \
        --out "$RESULTS_DIR/baseline_bm25.json" \
        --timeout 30 \
        --diag 2>&1 | tee "$RESULTS_DIR/baseline_bm25.log"
echo ""

# ── Configuration 2: With codebase-memory-mcp graph ──────────────
echo "━━━ [2/2] BM25 + codebase-memory-mcp graph ━━━"
CODERUN_MCP_ENABLED=true \
    python3 "$SCRIPT_DIR/metrics/retrieval.py" \
        --dataset "$SCRIPT_DIR/datasets/eshop_tasks.yaml" \
        --repo "$REPO" \
        --binary "$BINARY" \
        --out "$RESULTS_DIR/with_cbm_graph.json" \
        --timeout 30 \
        --diag 2>&1 | tee "$RESULTS_DIR/with_cbm_graph.log"
echo ""

# ── Summary Comparison ───────────────────────────────────────────
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║                    RESULTS COMPARISON                       ║"
echo "╠══════════════════════════════════════════════════════════════╣"

for config in baseline_bm25 with_cbm_graph; do
    if [ -f "$RESULTS_DIR/${config}.json" ]; then
        label=$(echo "$config" | sed 's/_/ /g' | sed 's/\b\(.\)/\u\1/g')
        summary=$(python3 -c "
import json, sys
with open('$RESULTS_DIR/${config}.json') as f:
    data = json.load(f)
s = data.get('summary', {})
print(f'  Recall@5:  {s.get(\"avg_recall@5\", 0):.4f}')
print(f'  Recall@10: {s.get(\"avg_recall@10\", 0):.4f}')
print(f'  MRR:       {s.get(\"avg_mrr\", 0):.4f}')
print(f'  Latency:   {s.get(\"avg_latency_ms\", 0):.0f}ms')
ma = s.get('miss_analysis', {})
if ma:
    print(f'  Misses:    {dict(ma)}')
" 2>/dev/null)
        echo "║  $label"
        echo "$summary"
        echo "║"
    fi
done

echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "Full results in: $RESULTS_DIR/"
echo "  baseline_bm25.json     — BM25 only"
echo "  with_cbm_graph.json    — BM25 + codebase-memory-mcp graph"
echo ""
echo "Note: FlashRank was removed from v1 runtime per benchmark evaluation."
echo "  Recall@5: +1.97pp but MRR: -6.78pp and 17x slower. Not worth it."
