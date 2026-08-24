# Evaluation Framework

This document describes the evaluation framework for Coderun AI Runtime.

## Overview

The evaluation framework uses [Promptfoo](https://www.promptfoo.dev/) to test:
1. **Model Routing Accuracy** — Correct tier selection based on task complexity
2. **Context Quality** — High-quality context packs with proper budget enforcement

## Quick Start

```bash
# Install Promptfoo
npm install -g promptfoo

# Run all evaluations
./eval/run-evaluation.sh

# Run specific evaluation
./eval/run-evaluation.sh model    # Model routing only
./eval/run-evaluation.sh context  # Context quality only

# View results in web UI
./eval/run-evaluation.sh --view
```

## Evaluation Types

### 1. Model Routing Accuracy

Tests that the Model Router correctly selects the appropriate tier:

| Task Type | Expected Tier | Examples |
|-----------|---------------|----------|
| Simple | fast | Fix typo, add comment, rename variable |
| Moderate | balanced | Add API endpoint, implement middleware |
| Complex | capable | Refactor architecture, implement OAuth2 |

**Metrics:**
- Accuracy: % of correct tier selections
- Target: ≥ 90% accuracy

### 2. Context Quality

Tests that the Context Engine produces high-quality context packs:

| Test | What it Checks |
|------|----------------|
| File inclusion | Mentioned files are included |
| Token budget | Total tokens ≤ max_tokens |
| Section ordering | skills → docs → code |
| Deduplication | No repeated content |
| Skill injection | Matched skills are included |
| Knowledge retrieval | Relevant knowledge is included |
| Routing decision | Model routing is present |

**Metrics:**
- Token efficiency: Actual tokens / Budget
- Content relevance: % of relevant content included
- Target: ≥ 85% quality score

## Configuration

### Environment Variables

```bash
# Coderun daemon URL (for live evaluation)
export CODERUN_DAEMON_URL="http://127.0.0.1:9527"

# Evaluation mode
export EVAL_MODE="live"  # or "mock"
```

### Promptfoo Configuration

The main configuration is in `eval/promptfoo.yaml`:

```yaml
description: "Coderun AI Runtime Evaluation"

providers:
  - eval/providers/model-routing.js
  - eval/providers/context-quality.js

tests:
  - eval/datasets/model-routing.yaml
  - eval/datasets/context-quality.yaml

outputPath: eval/results/evaluation.json
```

## Adding New Tests

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
      value: "balanced"  # or "fast" or "capable"
```

### Context Quality Tests

Add to `eval/datasets/context-quality.yaml`:

```yaml
- description: "New quality test"
  vars:
    task: "task description"
    max_tokens: 10000
  assert:
    - type: javascript
      value: "output.token_usage.total_tokens <= 10000"
```

## Evaluation Providers

### Model Routing Provider

Located in `eval/providers/model-routing.js`:

- Calls Coderun Model Router API
- Falls back to local scoring if daemon unavailable
- Returns tier: "fast", "balanced", or "capable"

### Context Quality Provider

Located in `eval/providers/context-quality.js`:

- Calls Coderun Context Engine API
- Falls back to mock context if daemon unavailable
- Returns context pack with token usage

## Interpreting Results

### Console Output

```
✓ Model Routing: 9/10 passed (90%)
✓ Context Quality: 8/10 passed (80%)
```

### Web UI

```bash
npx promptfoo view -c eval/promptfoo.yaml
```

Shows:
- Pass/fail for each test case
- Detailed output and metadata
- Comparison between providers

### JSON Results

Results are saved to `eval/results/`:
- `model-routing.json` — Model routing test results
- `context-quality.json` — Context quality test results
- `evaluation.json` — Combined results

## Continuous Integration

### GitHub Actions

```yaml
# .github/workflows/eval.yml
name: Evaluation
on: [push, pull_request]

jobs:
  eval:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '20'
      - run: npm install -g promptfoo
      - run: cargo build --release
      - run: ./target/release/coderun serve &
      - run: sleep 5  # Wait for daemon
      - run: ./eval/run-evaluation.sh
      - uses: actions/upload-artifact@v4
        if: always()
        with:
          name: evaluation-results
          path: eval/results/
```

## Thresholds

| Metric | Target | Description |
|--------|--------|-------------|
| Model Routing Accuracy | ≥ 90% | Correct tier selection |
| Context Token Efficiency | ≤ 100% | Within token budget |
| Context Section Order | 100% | skills → docs → code |
| Skill Injection Rate | ≥ 80% | Matched skills included |
| Knowledge Retrieval Rate | ≥ 70% | Relevant knowledge included |

## Troubleshooting

### Daemon Not Running

If evaluations fail with connection errors:

```bash
# Start the daemon
cargo run --release -p coderun-daemon

# Or use the CLI
./target/release/coderun serve
```

### Tests Failing

1. Check the test case is correctly defined
2. Verify the expected tier/value matches the logic
3. Run with `--nocapture` for detailed output
4. Check `eval/results/*.log` for errors

### Adding New Evaluation Types

1. Create dataset in `eval/datasets/`
2. Create provider in `eval/providers/`
3. Add to `eval/promptfoo.yaml`
4. Update this documentation
