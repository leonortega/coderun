# Coderun v1 — Repository Review, Gap Analysis, and Implementation Tasks

## Executive Summary

The current Coderun repository already implements a substantial portion of the architecture defined for the AI Runtime for Coding Agents.

The main conclusion is:

> **The project is not a greenfield implementation anymore. The core architecture is substantially present, but the implementation has grown beyond the intentionally small v1 scope. The primary problem is scope drift, not lack of functionality.**

The recommended next milestone is therefore not another architecture redesign.

Instead:

1. Freeze feature development.
2. Remove or isolate functionality that belongs to the future platform.
3. Validate the existing core with end-to-end benchmarks.
4. Prove that Coderun actually improves a real coding agent.
5. Only then add further elasticity, plugins, orchestration, or platform capabilities.

---

# 1. Target Product

Coderun should be a **local AI Runtime that sits in front of coding agents and makes them more efficient and consistent**.

It is not intended to replace Claude Code, OpenCode, Gemini CLI, Cursor, Codex, or other coding agents.

Its responsibility is:

> Given a developer's task and the current repository, deterministically prepare the best possible context, skills, and model before the coding agent executes the task—and minimize the context returned by tools during execution.

The v1 runtime has four primary responsibilities:

1. **Understand the repository**
   - files
   - symbols
   - dependencies
   - project structure

2. **Build relevant context**
   - retrieve only what matters for the task
   - fit information into a token budget
   - preserve useful cache-friendly ordering

3. **Select skills and model**
   - deterministic task classification
   - applicable skills
   - cheapest sufficient model

4. **Compress execution output**
   - reduce `git`, test, lint, and shell output
   - prevent unnecessary context growth

---

# 2. Intended Architecture

```text
                         CODING AGENT
                              │
                    Claude / OpenCode /
                    Cursor / Gemini
                              │
                         Adapter
                              │
                              ▼
                    ┌─────────────────┐
                    │     Coderun     │
                    │                 │
                    │ Repository      │
                    │ Intelligence    │
                    │       │         │
                    │ Knowledge       │
                    │       │         │
                    │ Skills          │
                    │       │         │
                    │ Context         │
                    │       │         │
                    │ Router          │
                    │       │         │
                    │ Optimizer       │
                    └───────┬─────────┘
                            │
                         LiteLLM
                            │
                    ┌───────┼───────┐
                    ▼       ▼       ▼
                  Claude    GPT    Gemini
```

During execution:

```text
                    CODING AGENT
                         │
                         │ tool call
                         ▼
              ┌────────────────────┐
              │ Execution Optimizer│
              │       RTK          │
              └─────────┬──────────┘
                        │
                 compressed output
                        │
                        ▼
                   CODING AGENT
```

---

# 3. What the Current Repository Already Has

The current implementation already contains substantial versions of the core architecture.

The repository has separate components/crates for:

- Repository Intelligence
- Knowledge
- Skills
- Context
- Router
- Optimizer
- Daemon
- CLI
- Storage
- Events
- Workflow

This means the project should **not be restarted from scratch**.

The core architecture is already close to the intended product.

---

# 4. Biggest Problem: Scope Drift

The agreed v1 direction was:

> **Concrete local AI runtime. No workflow engine. No enterprise orchestration. No unnecessary platform abstractions.**

The current repository has grown beyond that scope and includes functionality such as:

- `coderun-workflow`
- DBOS
- durable workflows
- human approval
- audits
- HMAC
- workflow API
- Prometheus
- Grafana
- event persistence
- event replay

Workflow is also enabled by default in the current implementation.

This is the largest divergence from the intended v1.

## Recommendation

Remove workflow from the v1 runtime.

Do not necessarily delete the work permanently. Instead isolate it under a future area or separate branch/module, for example:

```text
future/
└── workflow/
```

The v1 runtime should be usable without:

- DBOS
- workflow sidecars
- approval services
- durable workflow infrastructure

The basic runtime should work with a minimal local installation.

---

# 5. Crate and Modularity Review

The current repository has a fairly large modular decomposition:

```text
coderun-core
coderun-daemon
coderun-cli
coderun-repo-intel
coderun-knowledge
coderun-skills
coderun-context
coderun-router
coderun-optimizer
coderun-events
coderun-storage
coderun-workflow
```

The decomposition is technically reasonable, but it is more than necessary for the concrete v1 philosophy.

A simplified target could be:

```text
coderun-core
coderun-daemon
coderun-cli

coderun-repo-intel
coderun-knowledge
coderun-context
coderun-skills
coderun-router
coderun-optimizer
```

Potentially absorb or remove from the critical architecture:

```text
coderun-events
coderun-workflow
```

Storage can remain underneath the components that own the data rather than necessarily being a standalone architectural boundary.

Principle:

> Do not create an architectural boundary simply because it might be useful in a future version.

---

# 6. Repository Intelligence

This part follows the intended architecture well.

The implementation uses technologies such as:

- tree-sitter
- ripgrep
- Tantivy
- ast-grep
- dependency graph support
- repository watching
- LSP

The implementation is separated into concepts such as:

```text
parser
graph
watcher
lsp
```

Incremental indexing is also aligned with the intended design.

## Potential Problem

The implementation is already adding substantial capabilities:

- LSP
- dependency graph
- multiple language support
- feature flags
- broad language coverage

before fully proving the most important requirement:

> Given a repository and task, retrieve the correct code.

## Recommendation

Keep v1 language support deliberately limited.

For example:

```text
Rust
TypeScript
Python
JavaScript
```

Then expand based on real usage.

The important capability is not:

> Support every language.

It is:

> Given a task, retrieve the relevant repository information.

---

# 7. Context Engine

The Context Engine is one of the most important parts of the project.

The intended pipeline is:

```text
Task
 │
 ▼
Skill retrieval
 │
 ▼
Knowledge retrieval
 │
 ▼
Code retrieval
 │
 ▼
Candidate merge
 │
 ▼
Deduplication
 │
 ▼
Ranking
 │
 ▼
Token budget
 │
 ▼
Cache ordering
 │
 ▼
Context Pack
```

The current implementation already includes concepts such as:

- skills
- docs
- code
- frozen prefix/end ordering
- deduplication
- token budgeting
- fail-open behavior

These are aligned with the intended architecture.

## Main Improvement Needed

The Context Engine needs **context quality evaluation**, not only execution tests.

It is not enough to verify:

```text
Did BuildContext execute?
```

The project must answer:

```text
Did BuildContext retrieve the files actually needed?
```

---

# 8. Golden Dataset

The most important missing product-validation piece is a golden dataset.

Create real tasks such as:

```yaml
- task: "Fix authentication timeout"
  expected_files:
    - src/auth/service.rs
    - src/auth/middleware.rs
    - tests/auth_test.rs

- task: "Add pagination to users endpoint"
  expected_files:
    - src/users/controller.rs
    - src/users/service.rs
    - tests/users_test.rs
```

Measure:

```text
Recall@5
Recall@10
MRR
Context tokens
Latency
Duplicate ratio
```

Recommended structure:

```text
eval/
├── datasets/
│   ├── repository_tasks.yaml
│   └── expected_context.yaml
├── metrics/
└── promptfoo/
```

This should become a first-class part of the project.

---

# 9. Knowledge Hub

The current implementation includes:

- SQLite
- Tantivy
- FlashRank
- Engram
- BM25
- adaptive retrieval
- fallback behavior

This is powerful, but the v1 retrieval pipeline should remain simple.

Recommended model:

```text
Primary:
Tantivy BM25

Optional:
FlashRank

Memory:
Engram

Fallback:
SQLite
```

Do not add multiple ranking algorithms unless evaluation proves they improve the result enough to justify their latency.

Guiding principle:

> Every retrieval stage must justify its latency and token savings.

---

# 10. Skill Engine

The Skill Engine is aligned particularly well with the intended design.

It supports skill sources such as:

- Claude
- Cursor
- Continue
- agentskills.io

The important architectural decision is to normalize external skills into one internal representation.

Recommended flow:

```text
Community Skill
       │
       ▼
normalize
       │
       ▼
Coderun Skill
       │
       ▼
deterministic matching
```

Do not introduce LLM-based skill selection for v1.

Keep matching deterministic.

## Required Improvement

Add:

- priority
- specificity
- maximum active skills
- conflict handling

---

# 11. Model Router

The current router follows the intended general direction.

It uses concepts such as:

- structural score
- semantic score
- scope score
- capability tiers
- fast/balanced/capable routing

This is aligned with the original goal.

However, v1 should not attempt to become a universal model-selection algorithm.

The v1 routing pipeline should be:

```text
Task complexity
+
Context size
+
Required capability
+
Configured budget

↓

Model tier

↓

LiteLLM
```

Then measure whether routing actually:

- reduces cost
- preserves task success
- improves latency where appropriate

---

# 12. Execution Optimizer

The Execution Optimizer is another strong part of the current implementation.

The implementation includes concepts such as:

- RTK
- built-in compressors
- tee-on-failure
- token counting
- fail-open behavior

Required behavior:

```text
RTK available
    ↓
use RTK

RTK unavailable
    ↓
built-in compression

compression fails
    ↓
original output
```

The optimizer must never be able to break the coding agent.

---

# 13. Event Bus

The current implementation contains:

- event bus
- broadcast
- ring buffer
- SQLite persistence
- replay

This is technically useful but not required for v1.

## Recommendation

Remove event persistence and replay from the critical architecture.

Keep:

```text
tracing
structured logs
correlation IDs
metrics
```

If event replay becomes necessary later, add it based on a real requirement.

---

# 14. Daemon

The daemon architecture is appropriate for a local runtime.

The intended model is:

```text
Coding Agent
      │
      ▼
Unix socket / MessagePack
      │
      ▼
Coderun daemon
```

HTTP fallback can remain available.

The runtime should remain a simple local process rather than becoming a distributed service.

---

# 15. Adapters

The current implementation supports multiple agent integrations.

The risk is allowing the runtime to become dependent on several agent-specific mechanisms simultaneously.

## Recommendation

Choose one canonical integration for v1.

Recommended priority:

```text
Tier 1:
OpenCode

Tier 2:
Claude Code
Gemini
Cursor
```

Prove:

```text
OpenCode
→ Coderun
→ Context
→ Router
→ Model
→ response
```

end-to-end first.

---

# 16. Missing End-to-End Product Validation

The current tests demonstrate component correctness.

That is useful, but it is not enough.

The product must prove:

```text
Without Coderun

Agent
 ↓
Model
 ↓
Task result
```

versus:

```text
With Coderun

Agent
 ↓
Coderun
 ↓
better context
 ↓
better model selection
 ↓
less tool output
 ↓
Model
 ↓
better task result
```

The core question is:

> Does Coderun actually make coding agents better, cheaper, or faster?

---

# 17. Coderun vs Baseline Benchmark

Create a benchmark across 50–100 real coding tasks.

Measure:

```text
                    Baseline    Coderun

Task success           X%          Y%

Input tokens           X            Y

Output tokens          X            Y

Tool tokens            X            Y

Total tokens           X            Y

Latency                X            Y

Cost                   X            Y

Context recall         X            Y
```

This should become the primary product KPI.

---

# 18. Recommended Task Backlog

## P0 — Return to Agreed V1 Scope

### TASK-001 — Remove DBOS from v1

Requirements:

- Remove `coderun-workflow` from the default workspace.
- Remove DBOS sidecar startup.
- Remove workflow configuration.
- Remove workflow CLI commands from the normal v1 CLI.
- Remove approval flow from the v1 runtime.
- Preserve workflow code separately if desired.
- `coderun doctor` must not require DBOS.
- `coderun serve` must work without DBOS.

Reason:

Workflow is the largest divergence from the agreed v1 architecture.

---

### TASK-002 — Remove Event Persistence from the Hot Architecture

Keep:

```text
tracing
metrics
correlation IDs
```

Remove from v1:

```text
event replay
event ring persistence
event database
```

---

### TASK-003 — Reduce V1 Dependency Surface

Audit the workspace and remove dependencies that exist only for:

- workflow
- auditing
- HMAC workflow security
- distributed durability
- event replay

---

## P0 — Product Validation

### TASK-004 — Build the Baseline Benchmark

Create:

```text
eval/baseline/
eval/datasets/
eval/results/
```

Measure:

- task success
- input tokens
- output tokens
- tool tokens
- latency
- cost

---

### TASK-005 — Create 50 Real Coding Tasks

Include:

```text
bug fixing
feature addition
refactoring
testing
documentation
code search
architecture questions
```

Each task must identify expected relevant files.

---

### TASK-006 — Measure Context Retrieval

Measure:

```text
Recall@5
Recall@10
MRR
token count
latency
```

Do not optimize the Context Engine further until these measurements exist.

---

## P0 — Context Engine

### TASK-007 — Make BuildContext Deterministic

Given the same:

```text
repository state
+
task
+
configuration
```

BuildContext should produce the same Context Pack unless an explicitly external nondeterministic dependency is involved.

---

### TASK-008 — Make Context Pack a Stable Artifact

Define an explicit structure such as:

```text
ContextPack {
    task
    repository_state
    skills
    knowledge
    code
    token_usage
    metadata
}
```

The exact schema must be documented and tested.

---

### TASK-009 — Add Context Provenance

Every included item should record why it was selected.

Example:

```text
src/auth/service.rs

source: code
retriever: tantivy
score: 0.82
reason: symbol match
```

---

## P1 — Repository Intelligence

### TASK-010 — Validate Incremental Indexing

Test:

```text
initial index
      ↓
modify file
      ↓
reindex only changed file
      ↓
delete file
      ↓
rename file
      ↓
git checkout
```

Ensure stale symbols and search entries disappear.

---

### TASK-011 — Validate Dependency Graph

Create test repositories with known relationships:

```text
A → B
B → C
C → D
```

Verify graph traversal and updates.

---

### TASK-012 — Define Supported Languages Explicitly

Do not advertise unlimited language support.

Document exactly which languages are supported in v1.

---

## P1 — Knowledge

### TASK-013 — Simplify Retrieval Pipeline

Establish:

```text
Tantivy BM25
    ↓
optional FlashRank
    ↓
top K
```

Measure whether FlashRank improves retrieval enough to justify its latency.

---

### TASK-014 — Test Memory Separately

Engram should be treated as:

```text
optional enrichment
```

not a dependency required by the core runtime.

Fail-open behavior should remain.

---

## P1 — Skills

### TASK-015 — Define Canonical Normalized Skill Schema

Regardless of source:

```text
Claude
Cursor
Continue
agentskills.io
```

normalize into one internal format.

---

### TASK-016 — Add Deterministic Skill Priority

Define:

```text
specificity
priority
maximum active skills
conflict resolution
```

---

## P1 — Router

### TASK-017 — Create Routing Benchmark

For each task record:

```text
complexity score
selected tier
selected model
actual success
cost
latency
```

Use the data to validate routing quality.

---

### TASK-018 — Separate Model Configuration from Routing Logic

The algorithm chooses a tier.

Configuration defines the actual models.

Example:

```toml
[models.fast]
...

[models.balanced]
...

[models.capable]
...
```

Do not hardcode model knowledge into the routing algorithm.

---

## P1 — Optimizer

### TASK-019 — Benchmark RTK

Compare:

```text
raw output
RTK output
built-in compressor
```

Measure:

```text
tokens
latency
information retained
```

---

## P1 — Adapter

### TASK-020 — Make OpenCode the Canonical Integration

Create one complete E2E test:

```text
OpenCode
 ↓
Coderun
 ↓
BuildContext
 ↓
Router
 ↓
LiteLLM
 ↓
response
```

Do not add additional adapters until this flow is reliable.

---

## P2 — Observability

### TASK-021 — Add Request Correlation

Every request should have:

```text
request_id
session_id
repository_id
timestamp
```

Logs must allow the complete lifecycle to be reconstructed:

```text
request
→ context
→ router
→ model
→ optimizer
```

---

### TASK-022 — Add Useful Metrics

Metrics should answer:

```text
How fast?

How many tokens saved?

How much context?

Which model?

How often fail-open?

How good is retrieval?
```

Do not build dashboards until the metrics themselves are stable and useful.

---

## P2 — Documentation

### TASK-023 — Rewrite README Around Actual V1

The README should describe Coderun as:

> Coderun is a local AI runtime that improves coding agents through repository intelligence, context optimization, skills, model routing, and tool-output compression.

Avoid making workflow orchestration, durable approvals, enterprise auditing, or similar future functionality look like core v1 functionality.

---

# 19. Features Explicitly Not Recommended for V1

Do not add:

```text
Plugin Manager
Capability Registry
Generic workflow engine
Temporal
LangGraph
Distributed event bus
Vector database
Graph database
Multi-agent orchestration
Enterprise API
Web dashboard
Large numbers of additional model providers
Large numbers of additional language parsers
```

The project already has enough technology to prove the product.

---

# 20. Target V1 Architecture After Cleanup

```text
                         CODING AGENT
                              │
                              ▼
                       ┌────────────┐
                       │  Adapter   │
                       └─────┬──────┘
                             │
                             ▼
                   ┌──────────────────┐
                   │ Coderun Daemon   │
                   │                  │
                   │ Repository       │
                   │ Intelligence     │
                   │       ↓          │
                   │ Knowledge        │
                   │       ↓          │
                   │ Skills           │
                   │       ↓          │
                   │ Context Engine   │
                   │       ↓          │
                   │ Model Router     │
                   └────────┬─────────┘
                            │
                       Context Pack
                            │
                            ▼
                         LiteLLM
                            │
                     ┌──────┼──────┐
                     ▼      ▼      ▼
                   Claude   GPT   Gemini


         During agent execution:

                    Agent
                      │
                      ▼
                 shell/test/git
                      │
                      ▼
                    RTK
                      │
                      ▼
                compressed output
                      │
                      ▼
                    Agent
```

---

# 21. Final Assessment

## Keep

- Rust
- local daemon
- UDS + MessagePack
- SQLite
- tree-sitter
- ripgrep
- ast-grep
- Tantivy
- FlashRank
- Engram
- deterministic skills
- BuildContext
- heuristic router
- LiteLLM
- RTK
- fail-open design
- CLI
- repository watcher
- evaluation infrastructure

These components are aligned with the intended product.

## Simplify

- number of crates
- event infrastructure
- language scope
- LSP usage
- knowledge retrieval complexity
- adapter count

## Remove from V1

- DBOS
- workflows
- approvals
- workflow audits
- workflow HMAC
- durable workflow infrastructure
- event replay persistence

## Add

The most important additions are evaluation rather than architecture:

1. Golden retrieval dataset.
2. Baseline vs Coderun benchmark.
3. Context quality metrics.
4. Routing quality metrics.
5. RTK compression metrics.
6. End-to-end coding-agent tests.

---

# 22. Recommended Next Milestone

Do not throw away the current code.

The project is already sufficiently implemented to move from architecture development to product validation.

The next milestone should be:

```text
CURRENT CODE
     │
     ▼
SCOPE CLEANUP
     │
     ├── remove DBOS/workflow from v1
     ├── simplify events
     └── simplify dependencies
     │
     ▼
PRODUCT BENCHMARK
     │
     ├── baseline
     ├── retrieval evaluation
     ├── routing evaluation
     └── token savings
     │
     ▼
E2E CODING AGENT
     │
     ▼
PROVE:
     │
     ├── better context
     ├── fewer tokens
     ├── appropriate model
     └── no agent breakage
     │
     ▼
        V1
```

## The Most Important Conclusion

The biggest risk is not that Coderun is incomplete.

The biggest risk is that it becomes a sophisticated infrastructure project without proving that it improves a real coding agent.

The next milestone should therefore prove the **five-minute demo**:

> Install Coderun → connect OpenCode → ask it to implement or fix something in a real repository → show exactly what context Coderun selected → show which model it chose → show tokens saved → show the task succeeding.

Once that works convincingly, the elasticity, plugin, capability, and broader platform architecture can be added based on real requirements rather than speculation.
