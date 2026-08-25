# v0.6.0 — DBOS Required + Spec Compliance Plan

> **Purpose:** Promote DBOS from optional sidecar to required runtime durability (SQLite + Litestream, native `dbos-transact`), collapse duplicate implementations identified in `coderun.md` vs code audit, wire `extended-languages` feature, and adopt OpenSpec hook compat. No new orchestration product — DBOS is now first-class.
>
> **Locked decisions (user-approved):**
> 1. DBOS uses **SQLite + Litestream** (single-node `sqlite://~/.coderun/dbos.db` + `sqlite://~/.coderun/dbos_system.db`), per `coderun.md` §5 and DBOS requirements — not Postgres.
> 2. DBOS is **native async** (`async_trait` `IWorkflowEngine`, no `block_on_in_thread` hack, real `dbos-transact` `governedWorkflow` with communicator/transaction/sleep/signal).
> 3. Hook names use **OpenSpec recommendation**: spec-authoritative `chat.message` primary + `message.updated` compat shim with deprecation warn + metric.
> 4. Languages use **Option B**: `extended-languages` feature flag (`go,java,c,cpp` behind flag, default build = 4 langs `rust,ts,js,python`).
> 5. Tier2 remains **README-only** (no `AGENT.md` snippet).
>
> **Baseline:** `v0.5.0` `Cargo.toml:18` `0.5.0` `166 tests` — 15/16 tools first-class, DBOS optional (`enabled:false`). `v0.6.0` closes audit gaps: skill/router HMAC/path duplicates, UDS primary gaps, incremental watcher wiring, and promotes DBOS to required.

---

## 0. Executive Summary

| Area | v0.5.0 status | Gap class | v0.6.0 target |
|---|---|---|---|
| **DBOS durability** | Optional `enabled:false` `WorkflowConfig`, Express mock `workflow/dbos/src/main.ts` `Map workflows`, `block_on_in_thread` `dbos.rs:46`, `sha256(secret+body)` not HMAC, `005_audits.sql` in runtime DB | **P0 — required promotion** | Native `dbos-transact` `governedWorkflow` over SQLite+Litestream, `async_trait IWorkflowEngine`, real `Hmac<Sha256>`, `dbos-config.yaml` SQLite required, `enabled:true` default, `doctor` fails if DBOS down |
| **Hook names** | `.opencode/plugins/coderun.ts` `message.updated` only (drift from `chat.message` spec) | **P0 — spec drift** | Dual registration `chat.message` primary + `message.updated` compat shim, `tracing::warn` + `hook_compat_total` |
| **Languages** | `IndexConfig` advertises 8 but `parser.rs:get_language` implements 4 (`go,java,c,cpp` fallback regex) | **P0 — advertised vs real** | `extended-languages` feature `tree-sitter-go/java/c/cpp`, `parser.rs` `#[cfg(feature)]` arms, `validate()` warns if `go/java` requested without feature |
| **Duplicate skill scorer** | `coderun-skills::SkillEngine::compute_match_score` vs `coderun-knowledge::simple_tag_match` (divergent 0.3 thresholds) | **P0 — two impls** | Single scorer `SkillEngine`; `KnowledgeHub::match_skills` delegates, `simple_tag_match` deleted |
| **Duplicate tier→model** | `ModelRouter::tier_to_model` vs `LiteLLMClient::select_model` | **P0** | Single `tier_to_model`; latter delegates |
| **Duplicate HMAC** | `workflow/dbos.rs:verify_hmac` + `daemon/ratelimit.rs:verify_hmac` both `sha256(secret+body)` | **P0** | Single `coderun-core::secrets::verify_hmac` with `hmac` crate `Hmac<Sha256>` |
| **Duplicate UDS** | `lifecycle.rs:handle_uds_conn` 70 LOC inline vs `adapter.rs:handle_connection` | **P0** | Single listener `adapter.rs`; `lifecycle.rs` deletes inline UDS loop, single `Database::open` |
| **Duplicate paths/tokens** | `dirs()/dirs_home()` ×4, `count_tokens` ×2, `KnowledgeEntry` ×3 | **P1** | Single `coderun-core::dirs::home()` + `tokens::count` (`LazyLock<Regex>` for secrets) |
| **Repo Intel wiring** | `watcher.rs:try_notify_git2_watcher()->None` poll 5s fallback; `search_structural` fallback primary | **P1** | `notify+git2` incremental diff first-class (feature default), `sg-core` primary already gated |
| **Rerank/model** | `rerank.rs` TF-IDF fallback primary, `ort` optional off; `tantivy _language_filter` unused | **P1** | Keep TF-IDF fallback on `Err` only; `ort` download via `install.sh` stays optional, language filter enforced post-search |
| **Adapter UDS** | `.claude/hooks/*.sh` + `.opencode` HTTP primary, UDS stubs | **P1** | UDS MessagePack primary, HTTP fallback; `python3 socket.AF_UNIX + rmp` in hooks |
| **Context pipeline** | Per-section dedup not global, `session_fingerprints` unbounded, `Arc<Mutex>` serializes | **P1** | Global dedup `HashSet`, `LruCache 1000` or TTL, `Arc<RwLock>` (already), `fs::write` reversible cache spawned |
| **Packaging** | `install.sh` no DBOS/ort payload | **P2** | `install.sh` probes `sqlite3` + Litestream `DBOS_LITESTREAM_REPLICA_URL`, downloads `flashrank.onnx` int8 idempotently |

**Spec compliance:** 15/16 → 16/16 (LSP stays optional). DBOS no longer separate product.

---

## 1. P0 — DBOS Required (SQLite Native)

### 1.1 Contract change

**Current:** `crates/coderun-core/src/traits.rs:33 IWorkflowEngine` sync, `NoopWorkflowEngine` returns `Err` “separate product”, `config.rs:WorkflowConfig enabled:false engine:noop`.

**Plan:**
1. `crates/coderun-core/src/traits.rs:33` → `#[async_trait] pub trait IWorkflowEngine { async fn start_workflow ...; async fn get_status ...; async fn is_available ... }`. Requires `async-trait = "0.1"` workspace dep.
2. `crates/coderun-core/src/config.rs:258 WorkflowConfig` default `enabled:true engine:"dbos"` (was `false/noop`), `validate()` rejects `enabled && engine != "dbos"` and warns if `dbos_shared_secret` missing when enabled.
3. `crates/coderun-core/src/secrets.rs` add `pub fn verify_hmac(secret, body, sig) -> bool` using `hmac = "0.12"` `Hmac::<Sha256>::new_from_slice`; delete ad-hoc `sha256(secret+body)` hex.

**Acceptance:** `Config::default().validate()` passes with `enabled:true`; `verify_hmac` uses `hmac` crate, not `Sha256::new().update(secret)`.

### 1.2 `DBOSWorkflowEngine` native async

**Current:** `crates/coderun-workflow/src/dbos.rs:46 block_on_in_thread` hack, sync `start_workflow` returns local `wf_ uuid` on `Err` (fail-open).

**Plan:**
1. `crates/coderun-workflow/Cargo.toml` add `async-trait` + `hmac`; `crates/coderun-workflow/src/dbos.rs` delete `block_on_in_thread`, implement `#[async_trait] impl IWorkflowEngine` with async `reqwest` + `tokio::time::timeout(5s)` directly.
2. `hmac_header` → `Hmac::<Sha256>` + `hex`; `verify_hmac` re-export from core.
3. Fail semantics flip: when `workflow.enabled=true` and `is_available().await == false`, `start_workflow` returns `Err` (required durability) instead of local `wf_` id. `NoopWorkflowEngine` remains for `#[cfg(test)]` only.
4. Wire `require_approval` from `WorkflowConfig.require_approval_tiers` + `TaskRequest` into `POST body {require_approval}` (currently `false` hardcoded).

**Acceptance:** `cargo test -p coderun-workflow -- test_workflow_start_fail_open_when_dbos_down` updated to expect `Err` when enabled; no `std::thread::spawn` in `dbos.rs`.

### 1.3 Sidecar native `dbos-transact` over SQLite

**Current:** `workflow/dbos/src/main.ts` Express mock `Map workflows`, `package.json` `express` only, `dbos-config.yaml` `sqlite://~/.coderun/dbos.db` correct but no `dbos-transact` dep, `governed.ts` placeholder commented.

**Plan:**
1. `workflow/dbos/package.json:3` add `"dbos-transact": "^1.2"` (or `dbos` per DBOS requirements), keep `express` for HTTP ingress. Bump `version: 0.4.0 → 0.6.0`.
2. `workflow/dbos/src/main.ts` replace mock with:

   ```ts
   import { DBOS } from "dbos-transact";
   export const governedWorkflow = DBOS.workflow(async (task, opts) => {
     const ctx = await DBOS.communicator(fetchBuildContext); // HTTP bridge to daemon UDS
     if (opts.requireApproval) await DBOS.waitForSignal("approved", 86400);
     await DBOS.transaction(auditInsert);
     return ctx;
   });
   DBOS.launch();
   // express routes POST /workflow/start → DBOS.startWorkflow(governedWorkflow)(task, opts)
   ```

3. Keep `workflow/dbos/dbos-config.yaml` SQLite: `database_url: sqlite://~/.coderun/dbos.db`, `system_database_url: sqlite://~/.coderun/dbos_system.db`, `app_db_migrations: ./migrations`. Add `workflow/dbos/dbos-config-dev.yaml` alias. Document `DBOS_LITESTREAM_REPLICA_URL=s3://bucket/coderun` or `file://~/.coderun/replica` for durability (from `coderun.md` §5).
4. `scripts/install.ps1|.sh` probe `sqlite3 --version` and warn if `DBOS_LITESTREAM_REPLICA_URL` unset when `workflow.enabled`.

**Acceptance:** `npm install && npm run build` in `workflow/dbos` succeeds with `dbos-transact`; ` governedWorkflow` import present not commented.

---

## 2. P0 — Spec-Drift Fixes

### 2.1 Hook compat (OpenSpec recommendation)

**Current:** `.opencode/plugins/coderun.ts` registers `message.updated` only; spec `coderun.md` says `chat.message`.

**Plan:**
1. `.opencode/plugins/coderun.ts` register both `chat.message` (primary) and `message.updated` (compat shim) mapping to same `handle_pre_generation`. Compat path does `tracing::warn!("deprecated hook message.updated, use chat.message")` and `metrics::hook_compat_total.inc()`.
2. `crates/coderun-daemon/src/http_server.rs:map hook_type` exhaustive match for both strings, Prometheus `hook_compat_total{hook="message.updated"}` counter. Delete single-string assumption.
3. Keep until `opencode` 2.0 removes legacy; remove shim in `v0.7.0` with `CHANGELOG` deprecation note.

**Acceptance:** Install with `message.updated` still works; `coderun doctor` probe checks `opencode` plugin emits `WARN` on legacy hook.

### 2.2 Languages feature B

**Current:** `crates/coderun-core/src/config.rs:IndexConfig languages=[rust,ts,js,python,go,java,c,cpp]` advertises 8 but `coderun-repo-intel/src/parser.rs:get_language` only 4.

**Plan:**
1. `crates/coderun-repo-intel/Cargo.toml` add:

   ```toml
   tree-sitter-go = { version="0.23", optional=true }
   tree-sitter-java = { version="0.23", optional=true }
   tree-sitter-c = { version="0.23", optional=true }
   tree-sitter-cpp = { version="0.23", optional=true }
   [features]
   extended-languages = ["tree-sitter-go","tree-sitter-java","tree-sitter-c","tree-sitter-cpp"]
   git-watcher = ["dep:notify","dep:git2"]
   ```

2. `crates/coderun-repo-intel/src/parser.rs:get_language` gate new arms `#[cfg(feature="extended-languages")] "go" => tree_sitter_go::LANGUAGE_GO` etc.; fallback `None` → caller warns `WARN go parser requires --features extended-languages` and uses regex.
3. `crates/coderun-core/src/config.rs:validate()` warn (not error) if `languages` contains `go/java/c/cpp` without feature, suggest `cargo build -p coderun-repo-intel --features extended-languages`.
4. Document in `docs/01-architecture/ARCHITECTURE.md:Tech Stack` `tree-sitter` row as `4 default (+4 behind extended-languages)`.

**Acceptance:** Default `cargo build` supports 4; `cargo test -p coderun-repo-intel --features extended-languages -- test_go_symbols` finds `func main`.

### 2.3 Duplicate collapse

| Duplication | Deletion |
|---|---|
| `KnowledgeHub::simple_tag_match` vs `SkillEngine::compute_match_score` | Delete `simple_tag_match` `knowledge/src/lib.rs`, keep `SkillEngine`; `KnowledgeHub::match_skills` delegates |
| `tier_to_model` vs `select_model` | Keep `ModelRouter::tier_to_model`, make `LiteLLMClient::select_model` delegate |
| `verify_hmac` ×2 | Keep `secrets::verify_hmac` (hmac crate), delete both ad-hoc impls |
| `lifecycle.rs:handle_uds_conn` vs `adapter.rs:handle_connection` | Keep `adapter.rs`, delete inline UDS loop in `lifecycle.rs` |
| `Database::open` ×3 in `lifecycle.rs:initialize` | Single `Database::open` shared `Arc<Database>` |
| `dirs()/dirs_home()` ×4, `count_tokens` ×2, `KnowledgeEntry` ×3 | Single `coderun-core::dirs::home()` + `tokens::count` (`LazyLock<Regex>` for `redact_secrets` per-call `Regex::new` removed) |

**Acceptance:** Grep `verify_hmac|simple_tag_match|handle_uds_conn|Regex::new` finds single impl with `LazyLock`.

---

## 3. P1 — High-Priority Wiring (no code yet, docs only this phase)

- **Watcher:** `notify+git2` incremental `diff_tree_to_workdir` first-class (feature default for daemon builds). Polling 5s kept only inside `Err` branch with `warn!`.
- **Rerank:** Keep `ort` optional off by default; `install.sh` downloads `rank-T5-flan int8 ONNX` to `~/.coderun/models/flashrank.onnx` idempotently; TF-IDF fallback only on `Err`.
- **Adapter UDS:** Hooks (` .opencode`, `.claude`, `cursor`, `gemini`) UDS MessagePack primary `rmp-serde` 4-byte BE len + 10MB guard, HTTP fallback, 30s fail-open `OriginalPassthrough`.
- **Context pipeline:** Global dedup `HashSet` across `skills→docs→code`, `LruCache 1000` for session fingerprints, `RwLock` already, reversible cache `fs::write` via `spawn_blocking` (not hot path).
- **Storage:** `005_audits.sql` renamed `005_workflows.sql` conceptually (keep file id 005), add FK `workflows.correlation_id → events.correlation_id` when code phase.

---

## 4. File-Level Change Map (docs + code, docs first per instruction)

| File | Docs-phase action | Code-phase action (deferred) |
|---|---|---|
| `Cargo.toml:18` | — | `0.5.0 → 0.6.0`, add `async-trait,hint` deps |
| `crates/coderun-core/Cargo.toml` | — | add `async-trait,hmac` |
| `crates/coderun-core/src/traits.rs:33` | Update docstring in `ARCHITECTURE.md` to `async` | `#[async_trait] IWorkflowEngine` |
| `crates/coderun-core/src/secrets.rs` | — | `LazyLock<Regex>`, `hmac` `verify_hmac` |
| `crates/coderun-core/src/config.rs:258` | Update `docs/02-workflows/DBOS.md` to `enabled:true` | `enabled:true` default, `validate()` new checks |
| `crates/coderun-workflow/src/dbos.rs:46` | — | Delete `block_on_in_thread`, async + hmac |
| `workflow/dbos/package.json:3` | — | `dbos-transact` dep, `0.4.0→0.6.0` |
| `workflow/dbos/src/main.ts` | docs/ARCHITECTURE DBOS row already | Replace mock with `DBOS.workflow` |
| `workflow/dbos/dbos-config.yaml:3` | Add dev alias comment | keep SQLite, document Litestream replica |
| `.opencode/plugins/coderun.ts` | — | Dual hook registration |
| `crates/coderun-repo-intel/Cargo.toml` | — | `extended-languages` feature |
| `crates/coderun-repo-intel/src/parser.rs` | Update ARCHITECTURE tech table | Gate `go,java,c,cpp` |
| `crates/coderun-daemon/src/lifecycle.rs:32` | — | Single DB open, delete inline UDS |
| `docs/*` | This plan + ROADMAP/SCOPE/ARCHITECTURE/DBOS.md edits | — |
| `CHANGELOG.md` | Append `0.6.0` planned section | Fill on release |

---

## 5. Acceptance Checklist (release gate, unchanged thresholds)

- [ ] `cargo test` ≥166 tests green, `clippy` 0 warnings, `audit` 0 vulns
- [ ] `cargo test --features extended-languages -p coderun-repo-intel` go/java green
- [ ] `cargo bench` `BuildContext` p95 <50ms, `RTK` <10ms, `FlashRank ort` <50ms RAM <50MB int8
- [ ] `coderun doctor` all critical ✓, `DBOS` required probe `✓` (SQLite WAL+Litestream), `hook_compat_total` warns on legacy
- [ ] Grep `fallback-only`: heuristic/`TF-IDF`/`db.search_memory`/`compress_file_read`/polling as primary — none outside `Err/warn`
- [ ] `coderun preview "add auth"` hits engram 2s + tantivy→rerank + structural + graph edges (when feature on)
- [ ] `workflow start --require-approval` → `awaiting_approval` → `approve` → `completed` + `audits` row, kill mid-`sleep` → WAL replay same `workflow_id`
- [ ] No `Temporal` hits, `cargo search temporal` 0
- [ ] Dual hook: `chat.message` and `message.updated` both succeed, deprecation metric increments on legacy

---

## 6. Explicitly NOT v0.6.0

- Postgres for DBOS (deferred; SQLite + Litestream stays per your choice)
- Tier2 `AGENT.md` snippet (README-only)
- Vector/semantic recall, graph RAG beyond `graph.rs` adjacency
- `Temporal`, multi-tenancy, web dashboard, distributed

---

## 7. References

- User-locked decisions: SQLite DBOS, native async, OpenSpec hook compat, extended-languages B
- Audit: `coderun.md` vs code §1 scope creep (`005_audits`, `block_on_in_thread`, `sha256 secret+body`), `crates/coderun-knowledge` vs `coderun-skills` scorer drift
- Baseline `docs/V0_5_0_PLAN.md:9-27` first-class table, `docs/V0_4_0_PLAN.md:1.1` DBOS over Temporal rationale
- Tech stack `COMPONENTS.md:762`, `PRINCIPLES.md:148`
