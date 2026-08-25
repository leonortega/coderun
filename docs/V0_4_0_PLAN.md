# v0.4.0 — Production Hardening + DBOS Orchestration Plan

> **Purpose:** Close every remaining gap between `v0.3.0` (90%+ spec-compliant) and production-grade `v1`. v0.3.0 proved correctness; v0.4.0 proves operability: durable workflows, observability, security, distribution, and multi-agent. External orchestrator is **DBOS Transact** — decision locked (not Temporal).
>
> **Baseline:** `v0.3.0` per `docs/V0_3_0_PLAN.md:9` — UDS+MessagePack, tiktoken, cache-aware pack, repo-intel completion, Knowledge Hub unified, LiteLLM fallback, RTK, event persistence, MkDocs, promptfoo gates all green. `ROADMAP.md:105-133` listed v0.4.0 as "Planned" with Temporal as optional — this doc refines it and locks DBOS.
>
> **Non-goals:** v0.4.0 still does NOT add vector/semantic recall, graph-based retrieval beyond `graph.rs` edges, multi-tenancy, or model fine-tuning (`SCOPE.md:44-51`). Those stay deferred.

---

## 0. Executive Summary

| Dimension | Spec / ROADMAP requires | v0.3.0 ships | Gap class | v0.4.0 target |
|---|---|---|---|---|
| **External orchestration** (`SCOPE.md:96`, `COMPONENTS.md:10`, `traits.rs:33-42`) | Optional durable workflows: approvals, audit, multi-step governance when actually needed | `IWorkflowEngine` trait + `NoopWorkflowEngine` `traits.rs:45-58` — always `is_available()=false`, `start_workflow()` errors | **P0 — product decision locked** | DBOS Transact (single-node + SQLite + Litestream) as durable sidecar; `DBOSWorkflowEngine` implements `IWorkflowEngine`; daemon calls it only when `config.workflow.enabled=true` |
| **Monitoring & Observability** (`ROADMAP.md:111-115`) | Prometheus metrics, distributed tracing, health dashboards | `tracing` spans + `EventBus` `events/src/lib.rs` (1000-ring → `004_events.sql`) but no `/metrics`, no OTel export | **P1** | `/metrics` + OTel trace export; Grafana dashboard JSON |
| **Security hardening** (`ROADMAP.md:118-122`, `V0_3_0_PLAN.md:3.5`) | Input validation, rate limiting, audit logging | Partial: `http_server.rs:129-138` length check + `secrets.rs:redact_secrets()` — no token-bucket, no structured audit log | **P1** | Token-bucket per `session_id`, structured audit log (`audits` table `005_audits.sql`), request signing for DBOS→daemon |
| **Distribution** (`ROADMAP.md:124-128`) | Homebrew, Docker, Windows installer | `cargo build --release` only; `release.toml` present but no formula/Dockerfile/MSI | **P1** | `brew tap`, `Dockerfile` (multi-stage `rust:1.75-slim` + `distroless`), `cargo-wix` MSI |
| **Multi-agent** (`SCOPE.md:11`, `V0_3_0_PLAN.md:4`) | Tier 1: Cursor, Gemini CLI, Copilot, OpenClaw, Pi, Factory Droid — Tier 2 best-effort labeled | Only `opencode` + `Claude Code` `ADAPTERS.md:7-8`; `adapters/tier2/README.md` exists | **P1-P2** | Cursor + Gemini CLI promoted to Tier 1 (prove `IContextBuilder` portability); Copilot + Factory Droid scaffolds behind feature flag; Tier 2 table updated |
| **Concurrency & isolation** (`ROADMAP.md:129`, `SCOPE.md:36`) | Concurrent agent sessions, session isolation, memory | Single `Mutex<ContextEngine>` `daemon/src/adapter.rs:44`; `session_fingerprints` in-memory only — no per-session memory scope | **P1** | `Arc<RwLock<>>` + per-session `KnowledgeHub` namespace scoping; soak test 20 concurrent sessions |
| **Packaging hardening** (`V0_3_0_PLAN.md:3.2`) carry-over | `init --wizard`, full `doctor` (7 probes), `migrate --from` | `doctor` 3/7 checks are `⚠` `cli/src/main.rs:342-412`; no wizard/migration | **P2** | Close carry-over if v0.3.0 deferred |
| **Benchmarks & perf** (`ROADMAP.md:152`, `V0_3_0_PLAN.md:3.5`) | `BuildContext` p95 <50ms, indexing ≥300 files/s, RTK <10ms, `cargo bench` in CI | `benches/` stub, no `criterion` in `Cargo.toml:1` | **P2** | `criterion` benches + CI `cargo bench -- --save-baseline` |

**v0.4.0 compliance target:** 100% of `SCOPE.md:8-22` In-Scope + production SLOs (p95, availability, audit completeness). Spec score: 90% → 99% (remaining 1% is intentional deferral: vector search).

---

## 1. P0 — DBOS Transact Orchestration (decision: DBOS, not Temporal)

### 1.1 Why DBOS over Temporal

| Criterion | Temporal | DBOS Transact | Verdict for coderun |
|---|---|---|---|
| **Stack alignment** | Go server + PostgreSQL/Cassandra required; SDK is external service | Single-process TypeScript/Python with **SQLite + Litestream** durable store — same `rusqlite`+WAL `storage/src/lib.rs:25` we already ship | DBOS wins: zero new infra, Litestream replicates `~/.coderun/data.db` we already replicate |
| **Local-first daemon** | Assumes cluster; single-node+SQLite+Litestream is possible but second-class (requires `temporalite`) | Designed for single-node SQLite — exactly `SCOPE.md:39` distributed-infra out-of-scope, matches `daemon` long-lived local process | DBOS wins |
| **Durability guarantees** | Durable timers, activity retry, exactly-once via history | Durable **transactions + workflows + queues** via Postgres/SQLite WAL — `DBOS.workflow()` + `DBOS.transaction()` survive restarts; exactly-once via `workflow_id` idempotency | Tie — both durable; DBOS API is plain functions, easier to embed |
| **Operational cost** | Heavy: separate Temporal cluster, UI, worker fleet | Sidecar `dbos` npm package or `dbos-cloud` — for v0.4.0 we run **embedded DBOS sidecar** `crates/coderun-workflow/src/dbos.rs` that shells to `npx dbos start` or calls HTTP; no cluster | DBOS wins |
| **Governance primitives needed** | Signals, queries, approval activities, continue-as-new | `DBOS.workflow`, `DBOS.communicator`, `DBOS.queue`, `DBOS.cron` — approval gate = `await DBOS.sleep()` + external signal endpoint; audit = `DBOS.transaction()` row in `workflows`/`audits` tables | Both sufficient; DBOS simpler for our "only if/when approvals, audit, or multi-step governance is actually needed" (`table` row 16) |
| **Rust interop** | Rust SDK exists but server still Go | No native Rust SDK — we use **HTTP bridge**: Rust daemon (`IWorkflowEngine::start_workflow`) `POST /workflow/start` to Node sidecar; sidecar `POST /uds` back to Rust for `BuildContext` steps | Acceptable trade-off; HTTP boundary is already `adapter.rs` |
| **Licensing / vendor risk** | MIT (server) + Cloud SaaS | MIT (Transact) + open source; younger project but matches our `tiktoken-rs`/`tantivy` local-first choices | DBOS wins on philosophy |

**Decision locked:** `IWorkflowEngine` `traits.rs:33` gets `DBOSWorkflowEngine` (not `TemporalEngine`). Temporal remains documented as "evaluated, not chosen" in `ARCHITECTURE.md:230-241`. Rationale recorded here so future ADR need not re-litigate.

Spec §5 principle: runtime never requires orchestrator; it is consumed via `IContextBuilder` API. DBOS sidecar is **optional**, started only if `config.workflow.enabled=true` and `config.workflow.engine="dbos"`.

### 1.2 Architecture

```
┌──────────────┐  UDS/MessagePack   ┌──────────────────┐  HTTP :3001  ┌─────────────────┐
│ Coding Agent │ ────────────────► │  coderun-daemon   │ ───────────► │  dbos-sidecar    │
│              │ ◄──────────────── │  (Rust)           │ ◄─────────── │  (Node/TS)       │
└──────────────┘                  │  adapter.rs       │             │  workflows/*.ts  │
                                  │  ContextEngine    │             │  DBOS.workflow() │
                                  │  IWorkflowEngine ─┼────────────►│  SQLite WAL      │
                                  └──────────────────┘             │  + Litestream    │
                                                                   └─────────────────┘
```

* Hot path (`BuildContext` <50ms) **never** touches DBOS. Only when agent or policy requests a governed run (`coderun workflow start --task "..." --require-approval` or `config.workflow.auto_governance=true` for `tier=capable` tasks) does `adapter.rs` call `workflow_engine.start_workflow()`.
* DBOS workflow steps: `1. BuildContext(task)` (HTTP to Rust daemon UDS→HTTP bridge) → `2. Await approval` (if configured, `DBOS.sleep` + `POST /workflow/{id}/approve` signal) → `3. Execute` (agent does code edit — runtime still does not own editing per `SCOPE.md:28`) → `4. Audit log` (`DBOS.transaction()` insert to `audits` table) → `5. Emit WorkflowCompleted` on EventBus.
* Single-node SQLite at `~/.coderun/dbos.db` (separate from `~/.coderun/data.db` to isolate WAL). Litestream replicates both to `~/.coderun/replica/` or S3 if `DBOS_LITESTREAM_REPLICA_URL` set.

### 1.3 Plan

1. **Crate** `crates/coderun-workflow/` (**new**):
   - `Cargo.toml` — `reqwest`, `serde`, `tokio`, `coderun-core`, `coderun-storage`.
   - `src/lib.rs` — `DBOSWorkflowEngine: IWorkflowEngine` — `start_workflow()` POSTs `{task, config, workflow_id=req_{uuid}}` to `http://localhost:3001/workflow/start` with `tokio::time::timeout(5s)`; `get_status()` GETs `/workflow/{id}`; `is_available()` probes `/health`.
   - `src/dbos_sidecar.rs` — `spawn_sidecar()` — if `which dbos` found, `Command::new("npx").arg("dbos").arg("start")`; else log `WARN` and stay `Noop`. Health probe retries 3×100ms.
   - `src/types.rs` — `WorkflowRequest { task, require_approval, audit_level }`, `WorkflowStatus { id, state: Pending|AwaitingApproval|Running|Completed|Failed, audit_url }`.

2. **Node sidecar** `workflow/dbos/` (**new**, not Rust — TypeScript):
   - `package.json` — `dbos-transact@latest`, `express`.
   - `src/main.ts` — `DBOS.init({ dbUrl: "sqlite://~/.coderun/dbos.db" })`, `DBOS.launch()`.
   - `src/workflows/governed.ts`:
     ```ts
     export const governedWorkflow = DBOS.workflow(async (task: TaskRequest, opts: {requireApproval:boolean}) => {
       const ctx = await DBOS.communicator(async () => fetch("http://127.0.0.1:9527/hook", {method:"POST", body: rmpEncode(task)}));
       if (opts.requireApproval) { await DBOS.sleep(0); await DBOS.waitForSignal("approved", 24*3600); }
       await DBOS.transaction(async (tx) => tx.execute("INSERT INTO audits (workflow_id, task, ctx_pack_hash) VALUES (?,?,?)", [DBOS.workflowID(), task.message, hash(ctx)]));
       return ctx;
     });
     export const approveSignal = DBOS.signalHandler(async (id:string) => DBOS.sendSignal(id, "approved", {}));
     ```
   - `dbos-config.yaml` — `database_url: sqlite://~/.coderun/dbos.db`, `app_db_migrations: ./migrations`.

3. **Migrations** `crates/coderun-storage/src/migrations/005_audits.sql` (**new**):
   ```sql
   CREATE TABLE IF NOT EXISTS audits (id INTEGER PRIMARY KEY, workflow_id TEXT NOT NULL, task TEXT NOT NULL, ctx_pack_hash TEXT, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, actor TEXT);
   CREATE TABLE IF NOT EXISTS workflows (workflow_id TEXT PRIMARY KEY, status TEXT NOT NULL, task TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
   CREATE INDEX IF NOT EXISTS idx_audits_workflow ON audits(workflow_id);
   ```

4. **Config** `crates/coderun-core/src/config.rs:20-24` — add `WorkflowConfig { enabled: bool, engine: String /* "dbos"|"noop" */, dbos_endpoint: String /* http://localhost:3001 */, auto_governance: bool, require_approval_tiers: Vec<String> }` default `enabled:false`.

5. **Wiring** `crates/coderun-daemon/src/lifecycle.rs` — on `serve`, if `config.workflow.enabled` spawn sidecar, construct `DBOSWorkflowEngine` and pass as `Box<dyn IWorkflowEngine>` to `AdapterLayer`. `adapter.rs:281-371` — add `POST /workflow/start` and `POST /workflow/{id}/approve` handlers that delegate to engine; still fail-open (`OriginalPassthrough` analog: return `202 Accepted` with `workflow_id` even if DBOS down, log `WARN`).

6. **CLI** `crates/coderun-cli/src/main.rs` — `coderun workflow start <prompt> [--require-approval]` (calls engine), `coderun workflow status <id>`, `coderun workflow approve <id>`, `coderun workflow list`. `coderun doctor` adds probe `DBOS reachable` `⚠` if `enabled` but `/health` fails with hint `npx dbos start` or `coderun workflow --help`.

7. **Secrets & isolation** — DBOS HTTP payloads pass through `redact_secrets()` `core/src/secrets.rs` before outbound; per-workflow SQLite transaction uses `rusqlite` WAL already enabled `storage/src/lib.rs:25`.

**Acceptance:**
- `cargo test -p coderun-core -- test_noop_workflow_unavailable` still passes; new `test_dbos_engine_available_when_sidecar_up` (wiremock `http://localhost:3001/health` → 200) and `test_workflow_start_reports_id`.
- `coderun workflow start "refactor auth" --require-approval` → prints `workflow_id=wf_... status=AwaitingApproval`; `coderun workflow approve wf_...` → `Completed` and row appears in `SELECT * FROM audits WHERE workflow_id=?`.
- Kill sidecar mid-workflow → DBOS recovers on restart (Litestream WAL replay) — tested by `test_dbos_recovery_after_restart` (spawn, start, kill, restart, `get_status` returns same `workflow_id`).
- Hot path `BuildContext` p95 still <50ms when `workflow.enabled=false` (no DBOS in call graph — grep `DBOSWorkflowEngine` absence in `context/src/lib.rs`).

---

## 2. P1 — Production Hardening

### 2.1 Monitoring & Observability (ROADMAP.md:111-115)

**Current:** `tracing` INFO spans + `EventBus` broadcast `events/src/lib.rs` (in-memory 1000 + `004_events.sql` spill). No export.

**Plan:**
1. **Prometheus** `crates/coderun-daemon/src/metrics.rs` (**new**) — `prometheus = "0.13"` crate, `Registry` + `CounterVec` (`requests_total{hook_type,tier}`), `Histogram` (`build_context_duration_seconds` buckets 0.01,0.05,0.1,0.5,1,5,30), `Gauge` (`index_files`, `tantivy_docs`). Expose `GET /metrics` on `http_server.rs:95` (same port 9527) or separate `metrics_port` `config.daemon.metrics_port=9090` if `workflow.enabled`.
2. **OTel tracing** — add `tracing-opentelemetry = "0.24"` + `opentelemetry-otlp = "0.24"` behind `otel` feature. `daemon/src/main.rs` init `tracer_provider` if `OTEL_EXPORTER_OTLP_ENDPOINT` set; otherwise no-op. Spans: `BuildContext`, `search_code`, `retrieve_knowledge`, `rerank`, `workflow.start`.
3. **Health dashboard** `docs/dashboards/coderun.json` (**new**) — Grafana JSON importing Prometheus: request rate, p50/p95 latency, error rate (fail-open %), token budget burn, DBOS workflow backlog. `mkdocs.yml:24` nav add `Dashboards: docs/dashboards/README.md`.
4. **Alert rules** `deploy/prometheus/alerts.yml` — `BuildContextP95 > 0.05` firing, `FailOpenRate > 0.05` warning.

**Acceptance:** `curl localhost:9090/metrics` contains `coderun_build_context_duration_seconds_bucket`; `cargo test -p coderun-daemon -- test_metrics_endpoint` asserts 200; Grafana imports JSON without error.

### 2.2 Security Hardening (ROADMAP.md:118-122, SCOPE.md:42)

**Current:** `http_server.rs:129` length checks (100KB/1MB), `secrets.rs:redact_secrets()` regex before outbound, but no rate limiting, no audit log.

**Plan:**
1. **Token-bucket** `crates/coderun-daemon/src/ratelimit.rs` (**new**) — `governor = "0.6"` or hand-rolled `HashMap<session_id, Bucket { tokens, last_refill }>` at `AdapterLayer` entry (before `handle_connection`). Default `10 req/s` per `session_id`, burst 20, configurable `config.daemon.rate_limit_per_session`. On exceed: `429` with `OriginalPassthrough` and `error:"rate_limited"` (still fail-open for agent).
2. **Structured audit log** — every `AgentRequest` + `AgentResponse` + `WorkflowStatus` transition written to `audits` table `005_audits.sql` with `actor=session_id`, `redacted_payload` (via `redact_secrets`), `latency_ms`. Off hot path: `tokio::spawn` → `Database::insert_audit()` (reuse `storage/src/lib.rs` slow-query log). Retention `config.logging.retention_days` already exists.
3. **Request signing DBOS↔daemon** — `config.workflow.dbos_shared_secret` (env `CODERUN_DBOS_SECRET`), HMAC-SHA256 header `X-Coderun-Signature` on `POST /workflow/*`; verified in `adapter.rs` before dispatch. Log `WARN` on mismatch, return 401 but still fail-open for agent-originated requests.
4. **Input sanitization hardening** — extend `validate_input_len` to reject `..` + `/` path traversal with `400` (already WARN) and control characters `\x00-\x1f` except `\n\t`.

**Acceptance:** `test_rate_limit_bucket_refills`, `test_audit_row_written`, `test_secrets_redacted_before_dbos` (`api_key: sk-...` → `[REDACTED]` in `audits.payload`), `test_hmac_rejects_tampered`.

### 2.3 Distribution (ROADMAP.md:124-128)

1. **Homebrew** `Formula/coderun.rb` (**new**) + `cargo dist` or `homebrew-core` tap `github.com/leonortega/homebrew-coderun` — `brew install coderun` builds `coderun` + `coderun-daemon` binaries, installs `com.coderun.daemon.plist` for `launchd`.
2. **Docker** `Dockerfile` (**new**) — multi-stage: `FROM rust:1.75-slim AS builder` → `cargo build --release` → `FROM gcr.io/distroless/cc-debian12` copy binaries + `mkdocs` site. `docker run -v $PWD:/repo -p 9527:9527 coderun serve --http` for CI. `deploy/docker-compose.yml` adds `dbos-sidecar` service (`node:20-slim` running `workflow/dbos`).
3. **Windows** — `cargo wix` MSI `wix/main.wxs` (**new**) installs to `Program Files\coderun`, registers `coderun serve` as Windows Service via `cargo install cargo-wix`. UDS on Windows uses `tokio::net::windows::named_pipe` `adapter.rs:82-124` TCP fallback remains documented.

**Acceptance:** `brew audit --strict Formula/coderun.rb` passes; `docker build . && docker run --rm coderun --help` prints version; `cargo wix --no-build` produces `.msi` in `target/wix/`.

### 2.4 Multi-Agent & Concurrency (ROADMAP.md:127-133, SCOPE.md:11)

**Current:** Tier 1 = `opencode` + `Claude Code` `ADAPTERS.md:7-8`; `Mutex<ContextEngine>` serializes all sessions.

**Plan:**
1. **Promote to Tier 1:** `adapters/cursor/extension.ts` (**new**) — Cursor `UserPromptSubmit` hook → UDS MessagePack (same `adapter.rs` protocol); `adapters/gemini/hooks/pre-generation.sh` (**new**) — Gemini CLI `preGeneration` hook. Add rows to `ADAPTERS.md:6-8` with `✅ Supported (v0.4.0)` and `chat.message`-equivalent mapping `COMPONENTS.md:92-105`. Prove portability: `IContextBuilder` unchanged.
2. **Tier 2 scaffolds (behind `tier2` feature):** `adapters/copilot/`, `adapters/factory-droid/`, `adapters/openclaw/` — each reuses `adapter.rs` protocol; `adapters/tier2/README.md` updated with disclaimer "best-effort, no 30s guarantee".
3. **Session isolation:** Replace `Arc<Mutex<ContextEngine>>` `adapter.rs:44` with `Arc<RwLock<ContextEngine>>` for concurrent reads; `session_fingerprints` already `Arc<Mutex<HashMap>>` `context/src/lib.rs:46` — add `session_memory_namespace: HashMap<session_id, String>` to `KnowledgeHub` `knowledge/src/lib.rs:38` so `memory_search` scopes to `namespace=session_id` when `config.knowledge.per_session_memory=true`. `TokenUsage` already per-correlation.
4. **Soak test** `tests/soak_concurrent.rs` (**new**) — 20 tokio tasks × 100 `BuildContext` with distinct `session_id`, assert no deadlock, p95 still <50ms, no cross-session dedup leakage (hash from sess A not seen in sess B).

**Acceptance:** `cargo test --test soak_concurrent` passes; `ADAPTERS.md` table has 6 Tier 1 rows; `test_session_isolation_no_leak` asserts `dedup_content("sess1","x")` does not affect `"sess2"`.

---

## 3. P2 — Packaging, Benchmarks, Docs

### 3.1 Carry-over from v0.3.0 if deferred

If `V0_3_0_PLAN.md:3.2` items slipped, close in v0.4.0 week 1: `coderun init --wizard`, expanded `doctor` 7 probes (SQLite, tree-sitter 4 langs, tantivy, engram, FlashRank `ort` model, LiteLLM, RTK), `coderun migrate --from claude|continue|cursor`, `mkdocs gh-deploy`.

### 3.2 Benchmarks (ROADMAP.md:152 vs 160)

`Cargo.toml` add `criterion = "0.5"` dev-dep + `benches/context_bench.rs`, `benches/index_bench.rs`, `benches/rtk_bench.rs` (**new**):
- `BuildContext` p95 <50ms on `crates/coderun-context` 10KB fixture
- Indexing ≥300 files/s on `tests/fixtures/repo-300`
- RTK <10ms per `compress_output`
CI `.github/workflows/bench.yml` runs `cargo bench -- --save-baseline v0.4.0` and fails if regression >10%.

### 3.3 Docs

`docs/02-workflows/DBOS.md` (**new**) — governs when to use `workflow.enabled`, approval UX (`coderun workflow approve` vs Slack webhook), audit retention, Litestream replica config. `docs/01-architecture/ARCHITECTURE.md:209-241` updated to show `IWorkflowEngine → DBOSWorkflowEngine` + sidecar.

---

## 4. P3 — Deferred / Out-of-Scope for v0.4.0

Per `SCOPE.md:44-51` and `V0_3_0_PLAN.md:8` — must NOT be smuggled into v0.4.0:

- Vector/semantic memory (embedding store, cosine recall) — deferred, lexical `tantivy`+`FlashRank` stays.
- Multi-tenancy, SSO, RBAC, web dashboard — deferred to v1.1.
- Model fine-tuning, data labeling — out of scope.
- Collaborative editing, multi-repository single daemon — deferred (v2).
- Native per-language static analyzers as gates — external tool owns.

---

## 5. Work Breakdown & Dependencies

```
Phase 0 (week 1) — Foundations (no dependencies)
  ☐ WorkflowConfig + 005_audits.sql migration
  ☐ IWorkflowEngine → DBOSWorkflowEngine trait wiring (Noop still default)
  ☐ metrics.rs scaffolding + /metrics endpoint
  ☐ Close v0.3.0 carry-over (wizard/doctor/migrate) if needed

Phase 1 (week 1-2) — DBOS P0 (depends Phase 0)
  ☐ Node sidecar workflow/dbos/{package.json, main.ts, workflows/governed.ts, dbos-config.yaml}
  ☐ crates/coderun-workflow crate + HTTP bridge + sidecar spawn
  ☐ AdapterLayer /workflow/* handlers + HMAC verification
  ☐ Litestream config for ~/.coderun/dbos.db + ~/.coderun/data.db
  ☐ CLI workflow start/status/approve/list

Phase 2 (week 2-3) — Observability + Security (depends Phase 0-1)
  ☐ Prometheus histogram + OTel feature + Grafana dashboard JSON
  ☐ ratelimit.rs token-bucket + audit log spill + HMAC hardening
  ☐ soak_concurrent test + RwLock migration

Phase 3 (week 3-4) — Distribution + Multi-agent (parallel)
  ☐ Cursor + Gemini CLI Tier 1 adapters (prove portability)
  ☐ Copilot/Factory Droid Tier 2 scaffolds + ADAPTERS.md update
  ☐ Dockerfile (multi-stage) + docker-compose + Formula/coderun.rb + cargo-wix MSI

Phase 4 (week 4-5) — Benchmarks + hardening (parallel)
  ☐ criterion benches + bench.yml CI gate
  ☐ Dashboard + alerts.yml + DBOS.md docs
  ☐ CHANGELOG, ROADMAP bump 0.4.0, clippy/audit clean, 180+ tests

Critical path: Phase 0 → Phase 1 → Phase 2 → Phase 4 (5 weeks). Phase 3 parallelizable.
```

---

## 6. File-Level Change Map

| File | Action |
|---|---|
| `Cargo.toml` | add `prometheus`, `governor`, `criterion` dev-dep; workspace member `coderun-workflow` |
| `crates/coderun-core/src/config.rs:20-24` | add `WorkflowConfig {enabled, engine, dbos_endpoint, auto_governance, require_approval_tiers, dbos_shared_secret}` |
| `crates/coderun-core/src/traits.rs:33-42` | keep trait; add `DBOSWorkflowEngine` struct + impl behind `workflow` feature |
| `crates/coderun-storage/src/migrations/005_audits.sql` | **new** audits + workflows tables |
| `crates/coderun-storage/src/lib.rs:54-62` | apply 005; add `insert_audit()`, `list_audits()` |
| `crates/coderun-workflow/Cargo.toml` | **new** crate |
| `crates/coderun-workflow/src/lib.rs` | **new** `DBOSWorkflowEngine: IWorkflowEngine` |
| `crates/coderun-workflow/src/dbos_sidecar.rs` | **new** spawn + health probe |
| `crates/coderun-workflow/src/types.rs` | **new** WorkflowRequest/Status |
| `workflow/dbos/package.json` | **new** Node sidecar |
| `workflow/dbos/src/main.ts` | **new** DBOS.init + launch |
| `workflow/dbos/src/workflows/governed.ts` | **new** durable workflow + signal |
| `workflow/dbos/dbos-config.yaml` | **new** sqlite URL |
| `crates/coderun-daemon/src/metrics.rs` | **new** Prometheus registry + /metrics |
| `crates/coderun-daemon/src/ratelimit.rs` | **new** token-bucket |
| `crates/coderun-daemon/src/adapter.rs:44,281-371` | `Mutex→RwLock`, add `/workflow/*` handlers, HMAC check |
| `crates/coderun-daemon/src/http_server.rs:95` | add `/metrics` route |
| `crates/coderun-daemon/src/lifecycle.rs` | spawn sidecar if enabled |
| `crates/coderun-cli/src/main.rs` | add `workflow start/status/approve/list`, expand `doctor` with DBOS probe |
| `crates/coderun-knowledge/src/lib.rs:38` | add `per_session_memory` scoping |
| `crates/coderun-context/src/lib.rs:46` | keep fingerprints; add session isolation test |
| `adapters/cursor/extension.ts` | **new** Tier 1 |
| `adapters/gemini/hooks/pre-generation.sh` | **new** Tier 1 |
| `adapters/tier2/README.md` | update disclaimer |
| `docs/ADAPTERS.md:6-8` | add 2 Tier 1 + 3 Tier 2 rows |
| `docs/01-architecture/ARCHITECTURE.md:230` | note DBOS choice, deprecate Temporal mention |
| `docs/02-workflows/DBOS.md` | **new** |
| `docs/dashboards/coderun.json` | **new** Grafana |
| `deploy/prometheus/alerts.yml` | **new** |
| `Dockerfile` | **new** multi-stage |
| `deploy/docker-compose.yml` | **new** |
| `Formula/coderun.rb` | **new** |
| `wix/main.wxs` | **new** |
| `benches/*.rs` | **new** criterion |
| `.github/workflows/bench.yml` | **new** |
| `mkdocs.yml:24` | add Dashboards + Workflows nav |
| `CHANGELOG.md`, `ROADMAP.md` | 0.4.0 entries |

---

## 7. Acceptance Checklist (release gate)

- [ ] `cargo test` ≥180 tests (150 in v0.3.0 + ~30 new for workflow/metrics/ratelimit/multi-agent) all green, `cargo clippy` 0 warnings, `cargo audit` 0 vulns
- [ ] `cargo bench` — `BuildContext` p95 <50ms (`ROADMAP.md:159`), indexing ≥300 files/s, RTK <10ms; CI fails on >10% regression
- [ ] `coderun doctor` all critical ✓, DBOS probe `⚠ DBOS sidecar not running (hint: npx dbos start)` only when `workflow.enabled=true`
- [ ] DBOS e2e: `coderun workflow start "add audit gate" --require-approval` → `AwaitingApproval`; `approve` → `Completed` + row in `audits`; kill sidecar mid-`sleep` → restart → `get_status` recovers same `workflow_id` (Litestream WAL)
- [ ] No DBOS in hot path when `enabled=false` — grep `DBOSWorkflowEngine` absent from `BuildContext` flamegraph; p95 unchanged vs v0.3.0
- [ ] `curl :9090/metrics` exposes `coderun_requests_total` + `coderun_build_context_duration_seconds`; Grafana dashboard imports
- [ ] Rate limit: 30 req/s burst from one `session_id` → 429 on 31st, `OriginalPassthrough` with `reason:"rate_limited"`; audit row written with `[REDACTED]` for `api_key: sk-...`
- [ ] Tier 1 adapters: Cursor + Gemini CLI `UserPromptSubmit` → UDS MessagePack rewrite <1s, fail-open provable (31s handler → `OriginalPassthrough` with `reason:"timeout"`)
- [ ] Soak: 20 concurrent sessions × 100 builds — no deadlock, no cross-session fingerprint leak
- [ ] Distribution: `brew install` (tap), `docker build && docker run` healthcheck, `cargo wix` MSI builds
- [ ] Docs: `mkdocs build` + `docs/02-workflows/DBOS.md` + dashboard guide; `coderun preview "add auth"` + `coderun replay <id>` + `coderun workflow status <id>` all exercised in `EVALUATION.md`

---

## 8. What Is Explicitly NOT v0.4.0

Per `SCOPE.md:44-51` + §5 — must not be smuggled into v0.4.0:

- Vector/semantic recall, Neo4j-style graph retrieval — deferred.
- Multi-tenancy, dashboards with auth, human RBAC, org-level governance — deferred.
- Any LLM-based classifier for routing/retrieval — deterministic only (principle).
- Model fine-tuning, data labeling — out of scope.
- DBOS as required runtime — remains optional sidecar; `NoopWorkflowEngine` is default.

---

## 9. References

- Roadmap refined: `docs/ROADMAP.md:105-133` (this doc replaces the 5-bullet sketch)
- Architecture portability: `docs/01-architecture/ARCHITECTURE.md:209-241`, `COMPONENTS.md:9-1168`, `SCOPE.md:96` (external orchestration)
- v0.3.0 baseline: `docs/V0_3_0_PLAN.md:9`, `CHANGELOG.md:120-135`, `crates/coderun-core/src/traits.rs:33-58`
- DBOS docs: https://docs.dbos.dev (Transact, workflows, queues, Litestream) — single-node SQLite pattern
- Metrics: `prometheus` crate, `tracing-opentelemetry` + OTLP, Grafana
- Distribution: `cargo-dist`, `cargo-wix`, `homebrew` Formula DSL
