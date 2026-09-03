# Baseline vs Knocode Benchmark (TASK-004)

Directory structure per review `eval/baseline/`, `eval/datasets/`, `eval/results/` :

- `baseline/` — harness that runs each task with and without Knocode, measuring:
  - task_success, input_tokens, output_tokens, tool_tokens, total_tokens, latency, cost, context_recall
- `datasets/repository_tasks.yaml` — 50 golden tasks (bug fixing … architecture questions)
- `results/` — JSON outputs per run

Run:

```bash
python eval/baseline/run.py --dataset eval/datasets/repository_tasks.yaml --out eval/results/baseline_vs_knocode.json
python eval/metrics/retrieval.py --dataset eval/datasets/repository_tasks.yaml --k 5,10
python eval/metrics/baseline.py --results eval/results/baseline_vs_knocode.json
```

Primary KPI: `With Knocode` should show `better context (Recall@5 ↑), fewer tokens (total ↓), appropriate model tier, no agent breakage` per §22.
