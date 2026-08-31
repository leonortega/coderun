# LLM Routing Removal — Architectural Decision Record

**Date:** August 2026
**Status:** Accepted
**Decision:** Remove LiteLLM / Model Router (`crates/coderun-router`) from v1 runtime path. BuildContext is deterministic retrieval only — no LLM call decides ranking or model tier. MCP is retained.

---

## Context

### What existed

`crates/coderun-router` provided two layers:

- `ModelRouter::select_model` — heuristic `structural 0.30 / semantic 0.40 / scope 0.30` → `fast <0.3 / balanced / capable >0.7` (`crates/coderun-router/src/lib.rs:78-172`, `crates/coderun-core/src/config.rs:83-92`).
- `LiteLLMGateway` / `LiteLLMClient` — `reqwest` HTTP client `POST /v1/chat/completions`, `GET /v1/models`, `GET /health` with `Authorization: Bearer` and `fallback_chain` `capable→[capable,balanced,fast]` (`crates/coderun-router/src/litellm.rs:87-211`, `crates/coderun-router/src/lib.rs:270-307`).

`ContextEngine::build_context` returned `(ContextPack, RoutingDecision)` and called `select_model` after retrieval (`crates/coderun-context/src/lib.rs:449-452,617-623,768-792`). Config surface was `[routing]`, `[models]`, `[litellm]` + env `CODERUN_LITELLM_URL` (`crates/coderun-core/src/config.rs:22,83-108,232-239,364-366`).

LiteLLM itself was never a Rust crate — only Python `pip install "litellm[proxy]"` in `scripts/install.ps1:122` / `install.sh:48`.

The question was: **does `query → model tier` routing belong in the repository-context engine, when the coding agent already owns the LLM?**

### What is NOT being removed

Per explicit scope decision for this ADR, **MCP is retained**:

- `codebase-memory-mcp` was already removed in `docs/01-architecture/ENGRAM_CBM_REMOVAL.md` (zero R@5 gain). That decision stands independently.
- The MCP surface that remains (if any) is out of scope for this ADR — this change is strictly `LiteLLM / Model Router → removed`.

---

## Why removed

### 1. Not core to the validated capability

The strongest validated capability is `repository → BuildContext` (Tantivy BM25 + symbols + paths + Git). Benchmarks measure `Recall@5` / `MRR` / latency — not tier accuracy. No `BuildContext` consumer requires a `RoutingDecision` to function. V1 minimal stack `docs/00-project/V1_MINIMAL_STACK_PLAN.md:2.6` already marks this as `Defer`.

### 2. The coding agent already has the LLM

`CODING AGENT → Coderun → BuildContext → CODING AGENT` — Coderun answers *"what should the agent see before it starts working?"*. Model selection is the agent's responsibility. Duplicating it inside Coderun adds a second routing layer without evidence it improves task success.

### 3. Deterministic retrieval is the V1 goal

Your DefinitelyTyped finding applies here too: the next improvement is *what Coderun considers relevant* (field weights, markdown as first-class, token-budgeted selection), not *which model* to call. The feedback proposes:

```
Query → Deterministic retrieval → Deterministic ranking → Context Pack
```

No LLM required on the hot path. A heuristic `how to / install / run tests → boost docs` is deterministic and sufficient for V1; an LLM classifier would add latency/cost without proven gain.

### 4. Cost without benefit

| Aspect | Cost |
|---|---|
| Extra crate `coderun-router` (`reqwest` + `axum` only for LiteLLM) | Build time, dependency surface, `cargo test` scope |
| HTTP path `reqwest::Client` + retry + `Authorization` header | Runtime failure modes, `LITELLM_URL` / `CODERUN_LITELLM_URL` env, `doctor` probe |
| `RoutingDecision` / `RoutingScores` / `IModelGateway` / `LiteLlmConfig` / `ModelsConfig` / `RoutingConfig` | Config/IPC/trait surface across `coderun-core`, `coderun-context`, adapters |
| Eval `eval/providers/model-routing.js` + `eval/datasets/model-routing.yaml` | Separate evaluation axis not tied to retrieval quality |

No benchmark shows `heuristic routing → better BuildContext → higher task success`. The latency/cost is pure overhead for V1.

### 5. Operational simplicity (Local-First)

`docs/00-project/PRINCIPLES.md:56` Local-First favors in-process `Tantivy + SQLite + Git + Tree-sitter`. LiteLLM requires an external proxy process (`http://localhost:4000`), API keys, and network handling — contradicts minimal runtime.

---

## Decision

```text
LiteLLM / Model Router  ──► removed from v1 runtime
MCP                     ──► retained (out of scope for this ADR)
BuildContext            ──► ContextPack only (no RoutingDecision tuple)
```

`BuildContext` becomes:

```
Query → Tantivy/BM25 + symbols + paths + Markdown → Deterministic Ranker → Context Selector → Token Budget → BuildContext
```

No `select_model`, no `LiteLLMClient`, no `RoutingDecision`.

---

## Removed from

### Rust (must delete)

- **Entire crate** `crates/coderun-router/` — `Cargo.toml:15,18` (`axum`, `reqwest`), `src/lib.rs:1-575`, `src/litellm.rs:1-348` (~900 lines)
- Workspace member `Cargo.toml:11,83` `coderun-router = { path = "crates/coderun-router" }`
- `crates/coderun-core/src/config.rs:22,83-108,209-239,320-332,364-366,439-461` — `RoutingConfig`, `ModelsConfig`, `LiteLlmConfig`, their `Default`/`merge`/`apply_env_overrides`/`validate`, `CODERUN_LITELLM_URL` override
- `crates/coderun-core/src/ipc.rs:74,296-312` — `RoutingDecision`, `RoutingScores`, `RewrittenMessageData.routing_decision`
- `crates/coderun-core/src/traits.rs:16-33,9-11` — `IModelGateway` trait + `IContextBuilder::build_context` tuple return (`(ContextPack, RoutingDecision)` → `ContextPack`)
- `crates/coderun-context/src/lib.rs:9,88-111,617-623,768-792,852-860` — `model_router` field, `ModelRouter` import, `select_model` fn, `build_context` tuple return, `IContextBuilder` impl
- `crates/coderun-context/Cargo.toml:13` — `coderun-router` dependency

### Config / Env

- `.coderun/config.toml:38-62` — `[litellm]` + `[routing]` + `[model].routing_enabled` + `fast_model`/`balanced_model`/`capable_model` keys (retain `[context]`/`[index]`/`[cache]`)
- Env `CODERUN_LITELLM_URL` (`config.rs:364`, `README.md:339`, `docs/01-architecture/RUNTIME.md:233`) — deleted; `OPENAI_API_KEY` already absent (zero hits)

### Scripts (Python, not Cargo)

- `scripts/install.ps1:122-123` `pip install "litellm[proxy]"` optional branch
- `scripts/install.sh:48-49` `pip3 install "litellm[proxy]"`
- `scripts/uninstall.ps1:429-430` `litellm` in pip removal list
- `scripts/uninstall.sh:211` `litellm` loop entry

### Docs / Eval (trim, not necessarily delete history)

- `docs/00-project/V1_MINIMAL_STACK_PLAN.md:36,91-95,168-171` — mark `LiteLLM / Model Router` as **Removed** (was `Defer`)
- `docs/01-architecture/REQUEST_LIFECYCLE.md:437,485-517,543-554,596-615` — delete Stage 6 Model Routing
- `docs/01-architecture/DATA_FLOW.md:351,366-390,520` — delete Flow 7 Model Routing
- `docs/01-architecture/COMPONENTS.md:687-786` — delete Model Router section
- `docs/01-architecture/ARCHITECTURE.md:41,77,101,184,211` — delete `Model Router → LiteLLM` edges
- `docs/01-architecture/RUNTIME.md:183,186,202,233,285,347-353` — delete `routing_enabled`, `[routing]`, `[litellm]`, `RoutingDecision` examples
- `eval/config-model-routing.yaml`, `eval/providers/model-routing.js`, `eval/datasets/model-routing.yaml` entry, `eval/promptfooconfig.yaml:4-10` routing provider, `eval/run-evaluation.sh:43,49` routing eval
- `packages/opencode-coderun/test/e2e.test.ts:5-11` `Router → LiteLLM` assertion

### Kept (not routing-specific)

- `tiktoken-rs = "0.12"` in `Cargo.toml:65`, `crates/coderun-context/Cargo.toml:20`, `crates/coderun-optimizer/Cargo.toml:14`, `crates/coderun-bench/Cargo.toml:34`, `crates/coderun-cli/Cargo.toml:27` — **kept** (token budgeting for `ContextPack`, local `cl100k_base`, not via model API)
- `axum`/`reqwest` in `crates/coderun-daemon` (`axum 0.8`), `crates/coderun-workflow` (`reqwest 0.12`, `axum 0.7`), `crates/coderun-cli` (`reqwest 0.13` for health/workflow) — **kept** (not LiteLLM-specific)
- `tantivy-tokenizer-api` — **kept** (BM25 indexing)
- All MCP surface — **kept** per scope

---

## Consequences

1. **Simpler BuildContext.** `ContextEngine::build_context(TaskRequest) -> Result<ContextPack, CoderunError>` — no `RoutingDecision` tuple, no `model_router` field, no `IModelGateway` trait. Single responsibility: retrieval → ranking → selection → budgeting.
2. **Smaller dependency graph.** One fewer workspace crate, two fewer `reqwest`/`axum` usages in router, ~900 lines removed, `cargo test` scope reduced.
3. **Smaller config.** Three structs + one env var removed from `config.rs`; `.coderun/config.toml` shrinks; `doctor` no longer probes `LITELLM_URL`.
4. **Faster & more reliable.** No `reqwest::Client` construction per health/model check, no `complete_with_fallback` retry loop, no timeout branch on the hot path.
5. **Clearer ownership.** Agent owns model selection; Coderun owns repository context. V1 retrieval improvements (markdown as first-class, field boosts, token-budgeted `5-15` files) get the freed complexity budget.
6. **Reversible.** If future eval shows `routing → higher task success` on `eval/datasets/eshop_tasks.yaml`, re-propose with an ADR referencing this one — same gate as `FLASHRANK_REMOVAL.md`/`ENGRAM_CBM_REMOVAL.md`.

## References

- `crates/coderun-router/src/lib.rs:78-172` — removed heuristic
- `crates/coderun-router/src/litellm.rs:87-211` — removed HTTP client
- `crates/coderun-context/src/lib.rs:768-792` — removed `select_model` (heuristic only, no HTTP per `V1_MINIMAL_STACK_PLAN.md:2.6`)
- `crates/coderun-core/src/config.rs:221-230` — removed `ModelsConfig { fast:"gpt-4o-mini", balanced:"gpt-4o", capable:"o1" }`
- `docs/00-project/V1_MINIMAL_STACK_PLAN.md:91-95` — prior `Defer` rationale (now `Removed`)
- Feedback: `how to add a new type package and run dtslint tests` → 6,116-token case motivating deterministic doc-aware ranking
