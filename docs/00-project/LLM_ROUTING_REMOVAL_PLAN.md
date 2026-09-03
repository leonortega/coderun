# LLM Routing Removal — Implementation Plan

> ⚠️ **Status: EXECUTED (v0.8.6).** This is the historical implementation plan for the
> v0.8.6 removal — LiteLLM / Model Router are gone (see
> `docs/01-architecture/LLM_ROUTING_REMOVAL.md`). Retained as an audit trail of what
> was deleted and why; do not treat it as a live plan.
> Companion to `docs/01-architecture/LLM_ROUTING_REMOVAL.md` (rationale + inventory).
> Scope: remove LiteLLM / Model Router. **MCP is retained** — no changes to MCP.

## 1. Objective

Make `BuildContext` deterministic retrieval only:

```
CODING AGENT → Knocode → BuildContext (ContextPack only) → CODING AGENT
```

Remove `RoutingDecision` tuple, `ModelRouter`, `LiteLLMClient`, and all routing config/env/script/docs that exist solely for model-tier selection.

## 2. Pre-conditions

- `docs/01-architecture/LLM_ROUTING_REMOVAL.md` accepted.
- **MCP maintained** — confirmed this session (`no remove MCP, maintain`). No MCP files touched in this plan.
- Track A (this PR) = LLM removal only. Track B (next PR) = doc-aware ranking per DefinitelyTyped feedback — both MCP-compatible; no new retrieval engine.

## 3. Phases — ordered, each verifiable independently

### Phase 0 — Baseline lock (no code, 0.5 day)

- Tag baseline: `git diff --stat` clean, `cargo test -p knocode-router -p knocode-context -p knocode-core` green.
- Record current public API: `IContextBuilder::build_context -> (ContextPack, RoutingDecision)` and `ContextPack` consumers (`packages/opencode-knocode/src/index.ts:152`, adapters).
- Snapshot `.knocode/config.toml` (`[litellm]`, `[routing]`, `[model]`) and env `KNOCODE_LITELLM_URL` usage.

**Verify:** `rg -g '!target' "RoutingDecision|LiteLLM|ModelRouter|KNOCODE_LITELLM"` lists only expected files (router + context + core + docs).

### Phase 1 — Cut the crate (1 day)

**Delete:**

- `crates/knocode-router/` entirely (`Cargo.toml`, `src/lib.rs`, `src/litellm.rs`).
- Workspace member `Cargo.toml:11` + `Cargo.toml:83` `knocode-router = { path = ... }`.

**Edit:**

- `crates/knocode-context/Cargo.toml:13` remove `knocode-router`.
- `crates/knocode-context/src/lib.rs:9` remove `use knocode_router::{...}`.
- `crates/knocode-context/src/lib.rs:88-111` remove `model_router: ModelRouter` field + `new()` init.
- `crates/knocode-context/src/lib.rs:449-452,617-623,768-792,852-860` change `build_context` signature to `-> Result<ContextPack, KnocodeError>` (or `String`), delete `select_model`, delete `routing_decision` construction, update `IContextBuilder` impl.
- `crates/knocode-core/src/traits.rs:9-33` change `IContextBuilder::build_context` to `ContextPack` only, delete `IModelGateway`.

**Verify:** `cargo test -p knocode-context -p knocode-core` green; `cargo check --workspace` no `knocode-router` errors.

### Phase 2 — Collapse core config/IPC (0.5 day)

**Edit `crates/knocode-core/src/config.rs`:**

- Remove structs `RoutingConfig:83-92`, `ModelsConfig:94-100`, `LiteLlmConfig:102-108`.
- Remove `Default` impls `209-239`, `merge 320-332` routing/models/litellm arms, `apply_env_overrides 364-366` `KNOCODE_LITELLM_URL`, validation `439-461`.
- Remove fields `Config { models, routing, litellm }` (`12,21,22`).
- Update tests `568-574` that assert `[litellm]`/`[models]`/`[routing]` toml.

**Edit `crates/knocode-core/src/ipc.rs`:**

- Remove `RoutingDecision:296-303`, `RoutingScores:305-312`, `RewrittenMessageData.routing_decision:74`.

**Edit `.knocode/config.toml`:**

- Delete `[litellm] 38-41`, `[routing] 50-62`, `routing_enabled` + `fast_model`/`balanced_model`/`capable_model` keys. Keep `[context]`/`[index]`/`[cache]`.

**Verify:** `cargo test -p knocode-core` green; `rg "KNOCODE_LITELLM_URL|LiteLlmConfig|RoutingConfig|ModelsConfig"` in `crates/` returns zero.

### Phase 3 — Scripts cleanup (0.25 day)

- `scripts/install.ps1:122-123` delete `pip install "litellm[proxy]"` branch (keep `mkdocs`/`other` handling).
- `scripts/install.sh:48-49` delete `pip3 install "litellm[proxy]"`.
- `scripts/uninstall.ps1:429-430` remove `litellm` from `@("litellm", ...)`.
- `scripts/uninstall.sh:211` remove `litellm` from `for pkg in ...`.

**Verify:** `rg litellm scripts/` returns only history/ADR mentions.

### Phase 4 — Docs & eval trim (0.5 day)

**Docs to edit (mark routing as Removed, point to ADR):**

- `docs/00-project/V1_MINIMAL_STACK_PLAN.md:36,91-95,168-171` — `LiteLLM / Model Router: ❌ Removed (see LLM_ROUTING_REMOVAL.md)` + update Phase 4.
- `docs/01-architecture/REQUEST_LIFECYCLE.md` — delete Stage 6, remove `RoutingDecision` examples.
- `docs/01-architecture/DATA_FLOW.md` — delete Flow 7.
- `docs/01-architecture/COMPONENTS.md:687-786` — delete Model Router section.
- `docs/01-architecture/ARCHITECTURE.md` + `RUNTIME.md` + `README.md:9-11,103,294,325,339` — delete routing rows, keep `tiktoken-rs` rows.
- `CHANGELOG.md` — no deletion (history); add entry for this removal.

**Eval to delete/trim:**

- Delete `eval/config-model-routing.yaml`, `eval/providers/model-routing.js` (or gate behind `legacy/`).
- Remove `eval/datasets/model-routing.yaml` provider + `eval/promptfooconfig.yaml:4-10` + `eval/run-evaluation.sh:43,49` routing eval.
- Update `packages/opencode-knocode/test/e2e.test.ts:5-11` to assert `ContextPack` only.

**Verify:** `rg -g '!target' "RoutingDecision|IModelGateway|LiteLLMClient|fallback_chain" docs/ eval/ packages/` only in ADR/history.

### Phase 5 — Adapters & downstream (0.5 day)

- `packages/opencode-knocode/src/index.ts:152` `repository_path` daemon scoping — remove `routing_decision` expectation from `BuildContext` response.
- `adapters/` examples — remove `routing_decision` JSON snippets.
- `crates/knocode-storage/src/migrations/003_graph.sql:14-15` `cost_usd` — keep column (harmless) or add `// unused since LLM removal` comment; do not migrate data.

**Verify:** `cargo test --workspace` + `pnpm -C packages/opencode-knocode test` (if present) green.

## 4. Not in scope (MCP retained) + Follow-up Track B (doc-aware ranking, MCP-compatible)

> The earlier draft “Proposed V1 — no LLM/Engram/MCP” is superseded. **MCP is retained** — no MCP deletion in this plan. Only LiteLLM/Model Router is removed.

### What stays

- **MCP** — retained, no deletion. `codebase-memory-mcp` was already removed independently (`docs/01-architecture/ENGRAM_CBM_REMOVAL.md`); the MCP server surface that remains is untouched.
- `tiktoken-rs` — kept (local token counting `Cargo.toml:65`).
- `axum`/`reqwest` in `daemon`/`workflow`/`cli` — kept.

### Follow-up Track B — Deterministic doc-aware ranking (separate PR after LLM removal, MCP-compatible)

This incorporates your DefinitelyTyped feedback (“context-source prioritization, not retrieval quality”) with **MCP maintained**:

1. **`crates/knocode-storage/src/tantivy_index.rs:80-125` — fix doc penalty:** `FileClass::Documentation 0.2 → 1.2` (or `1.5` for `README.md`/`CONTRIBUTING.md` via filename boost). Add `title` field extracted from first `#` in markdown for `title 2.5×` boost. Keeps MCP path unchanged.
2. **`crates/knocode-storage/src/tantivy_index.rs:700-720` — unified field weights (deterministic, no LLM):** `symbol_name 3.0 / title 2.5 / path 2.0 / documentation 2.0 / filename 1.5 / content 1.0`. No classifier needed for V1.
3. **`crates/knocode-context/src/lib.rs:252-376` — query-adaptive boost (no LLM, ~15 lines, behind `KNOCODE_DOC_BOOST` flag):** `how to|how do|how can|where do|install|run tests|configuration → Documentation *1.8`, vs `where is|find implementation|class handles → symbol_name *1.5`.
4. **`crates/knocode-context/src/lib.rs:463-470 + 690-765` — token-budgeted selection:** keep `candidate_k=200` → rank → select until `max_tokens=12000` or score drop `>0.4*top_score` or gap `>2×`, clamp `5-15` files typical. `max_files=50` becomes hard ceiling only (not target). Fixes `6,116-token` DefinitelyTyped case.
5. **BuildContext (MCP-compatible):** `Query → Tantivy/BM25 + symbols + paths + Markdown → Deterministic Ranker → Context Selector → Token Budget → BuildContext`. No `gpt-4o` routing on hot path. MCP tools continue to call `build_context` for `ContextPack`.

**Files for Track B:** `crates/knocode-storage/src/tantivy_index.rs`, `crates/knocode-repo-intel/src/lib.rs` (ensure `**/*.md` indexed — `is_indexable_text_file:912` already includes `md`, just fix boost), `crates/knocode-context/src/lib.rs`, `crates/knocode-core/src/config.rs` (if `KNOCODE_DOC_BOOST` flag).

**Not in Track B:** Vector DB / embeddings / reranker / LLM query expansion — deferred per `V1_MINIMAL_STACK_PLAN.md:2.8-2.10`.

## 5. Verification per phase

| Phase | Command | Expected |
|---|---|---|
| 0 | `cargo test -p knocode-router -p knocode-context -p knocode-core` | green baseline |
| 1 | `cargo check --workspace` | no `knocode-router` |
| 1-2 | `cargo test -p knocode-context -p knocode-core -p knocode-storage` | green |
| 2 | `rg -g '!target' "KNOCODE_LITELLM_URL|RoutingConfig"` `crates/` | 0 hits |
| 3 | `rg litellm scripts/` | only ADR |
| 4 | `rg "RoutingDecision|LiteLLM" docs/ eval/` | only ADR/history |
| 5 | `cargo test --workspace` + `pnpm test` | green |
| all | `cargo test --workspace` | full green before PR |

## 6. Rollback

- Revert single PR; crate is isolated — no data migration (SQLite `cost_usd` column retained). Config `.knocode/config.toml` can re-add `[litellm]`/`[routing]` if needed, but V1 will ship without it.

## 7. Effort & ordering

Total `~3 days` wall-clock (phases sequential for clean diff). Phase 1 is the critical path; phases 2-5 can be one commit after Phase 1 green.

Proceed to Phase 1 after approval — no code changes until explicit “go ahead”.
