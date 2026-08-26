#!/usr/bin/env python3
"""
Retrieval quality metrics (TASK-006): Recall@5, Recall@10, MRR, tokens, latency, duplicate ratio.
Compares BuildContext code_context vs expected_files from repository_tasks.yaml
No mock fallback — fails hard if coderun preview times out so Recall@5/10 is honest.

Usage:
  python eval/metrics/retrieval.py --dataset eval/datasets/repository_tasks.yaml --k 5,10
  python eval/metrics/retrieval.py --dataset eval/datasets/repository_tasks.yaml --k 5,10 --out eval/results/evaluation.json
"""

import argparse
import yaml
import time
import re
import pathlib
import subprocess
import json
import sys
import io

# Force UTF-8 output on Windows
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8', errors='replace')
sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8', errors='replace')


def recall_at_k(expected, retrieved, k):
    if not expected:
        return 1.0
    topk = set(retrieved[:k])
    hits = len(set(expected) & topk)
    return hits / len(expected)


def mrr(expected, retrieved):
    for i, r in enumerate(retrieved, 1):
        if r in expected:
            return 1.0 / i
    return 0.0


def duplicate_ratio(retrieved):
    if not retrieved:
        return 0.0
    return 1.0 - (len(set(retrieved)) / len(retrieved))


def count_tokens_heuristic(text: str) -> int:
    # fallback heuristic ~ chars/4, matches tiktoken fallback in Rust
    return max(len(text) // 4, len(text.split()) if text else 0)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dataset", default="eval/datasets/repository_tasks.yaml")
    ap.add_argument("--k", default="5,10", help="Comma-separated k values for recall@k")
    ap.add_argument("--out", default="eval/results/evaluation.json")
    ap.add_argument("--timeout", type=int, default=10, help="Seconds before preview times out")
    ap.add_argument("--binary", default=None, help="Path to coderun binary (default: target/release/coderun)")
    args = ap.parse_args()

    ks = [int(x) for x in args.k.split(",")]
    data = yaml.safe_load(open(args.dataset))
    if not isinstance(data, list):
        print(f"ERROR: dataset {args.dataset} did not parse as list (got {type(data)})", file=sys.stderr)
        sys.exit(1)

    print(f"Loaded {len(data)} tasks from {args.dataset}")
    results = []
    total_mrr = 0.0
    total_latency_ms = 0.0
    recall_sums = {k: 0.0 for k in ks}

    for task in data[:50]:  # limit to 50 as per golden dataset
        expected = task.get("expected_files", [])
        task_str = str(task.get("task") or "")
        retrieved = []
        latency_ms = 0
        t0 = time.time()
        try:
            # Ensure task_str is string even if yaml has None
            task_str = str(task_str or "")
            # Use pre-built binary for speed (skip cargo build)
            import os
            if args.binary:
                binary = args.binary
            else:
                binary = os.path.join("target", "release", "coderun.exe")
                if not os.path.exists(binary):
                    binary = os.path.join("target", "release", "coderun")
            proc = subprocess.run(
                [binary, "preview", task_str],
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=args.timeout,
            )
            latency_ms = int((time.time() - t0) * 1000)
            if proc.returncode != 0 and not proc.stdout:
                raise RuntimeError(
                    f"coderun preview failed (rc={proc.returncode}) stderr={(proc.stderr or '')[:500]}"
                )
            out = (proc.stdout or "") + (proc.stderr or "")
            for line in out.splitlines():
                stripped = line.strip()
                # Normalize Windows backslashes to forward slashes for honest comparison
                if stripped.startswith("// ") and ":" in stripped:
                    p = stripped[3:].split(":")[0].strip().replace("\\", "/")
                    if p and p not in retrieved:
                        retrieved.append(p)
                elif stripped.startswith("// ") and "/" in stripped:
                    p = stripped[3:].strip().split()[0].replace("\\", "/")
                    if p and p not in retrieved:
                        retrieved.append(p)
            # Honest metric: if preview succeeded but returned 0 files, that's 0 recall (not fallback)
        except subprocess.TimeoutExpired:
            print(
                f"ERROR: coderun preview timeout ({args.timeout}s) for task '{task_str[:60]}' — failing hard, no mock fallback (TASK-004/006)",
                file=sys.stderr,
            )
            sys.exit(2)
        except FileNotFoundError as e:
            print(f"ERROR: cargo not found: {e}", file=sys.stderr)
            sys.exit(2)
        except RuntimeError:
            raise
        except Exception as e:
            print(f"ERROR: preview exception for '{task_str[:60]}': {e}", file=sys.stderr)
            sys.exit(2)

        total_latency_ms += latency_ms
        m = mrr(expected, retrieved)
        total_mrr += m
        recs = {}
        for k in ks:
            r = recall_at_k(expected, retrieved, k)
            recs[f"recall@{k}"] = round(r, 4)
            recall_sums[k] += r
        dup = duplicate_ratio(retrieved)
        tokens = count_tokens_heuristic("\n".join(retrieved))

        print(
            f"task={task_str[:40]:40} recall@5={recs.get('recall@5', 0):.2f} recall@10={recs.get('recall@10', 0):.2f} mrr={m:.2f} latency={latency_ms}ms retrieved={retrieved[:3]}"
        )
        results.append(
            {
                "task": task_str,
                "category": task.get("category", ""),
                "expected_files": expected,
                "retrieved": retrieved,
                "mrr": round(m, 4),
                "latency_ms": latency_ms,
                "duplicate_ratio": round(dup, 4),
                "tokens_heuristic": tokens,
                **recs,
            }
        )

    n = len(results) or 1
    summary = {
        "total_tasks": len(results),
        "avg_mrr": round(total_mrr / n, 4),
        "avg_latency_ms": round(total_latency_ms / n, 1),
        "avg_duplicate_ratio": round(sum(r["duplicate_ratio"] for r in results) / n, 4) if results else 0,
    }
    for k in ks:
        summary[f"avg_recall@{k}"] = round(recall_sums[k] / n, 4)

    # Writer for MRR/latency/duplicate_ratio (TASK-004/006)
    out_path = pathlib.Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    payload = {"summary": summary, "ks": ks, "results": results}
    out_path.write_text(json.dumps(payload, indent=2))
    print(f"\nSummary: {json.dumps(summary, indent=2)}")
    print(f"Wrote {len(results)} results to {out_path} (honest, no mock fallback)")


if __name__ == "__main__":
    main()
