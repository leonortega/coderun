# DBOS Workflows (v0.6.0 — required)

DBOS Transact is the **required** durable orchestrator since v0.6.0 (`docs/V0_6_0_PLAN.md:1`). Runtime uses SQLite + Litestream single-node durability (`sqlite://~/.coderun/dbos.db` + `sqlite://~/.coderun/dbos_system.db`, `DBOS_LITESTREAM_REPLICA_URL` for replica). `IWorkflowEngine` is `async_trait` native (no `block_on_in_thread` hack). See `workflow/dbos/dbos-config.yaml` (prod = SQLite, dev alias `dbos-config-dev.yaml`).

## When to enable

```toml
[workflow]
enabled = true              # true default since v0.6.0 (was false in v0.4-0.5)
engine = "dbos"
dbos_endpoint = "http://localhost:3001"
auto_governance = false            # true → tier=capable tasks auto-start workflow
require_approval_tiers = ["capable"]
```

Or env: `CODERUN_WORKFLOW_ENABLED=true CODERUN_DBOS_ENDPOINT=http://localhost:3001` (v0.6.0 `enabled` defaults `true`; set `false` only for tests with `#[cfg(test)] NoopWorkflowEngine`).

## Architecture

Rust daemon (`DBOSWorkflowEngine` `crates/coderun-workflow/src/dbos.rs`) async `#[async_trait] IWorkflowEngine` POSTs to Node sidecar `workflow/dbos/src/main.ts` (`POST /workflow/start`) via `tokio::time::timeout(5s)`. Sidecar runs `DBOS.workflow(governedWorkflow)` native `dbos-transact` (`DBOS.communicator(BuildContext)` → `DBOS.waitForSignal("approved",86400)` if `require_approval` → `DBOS.transaction(INSERT INTO audits)`). SQLite `~/.coderun/dbos.db` WAL + Litestream replica (`DBOS_LITESTREAM_REPLICA_URL=s3://…` or `file://~/.coderun/replica`). No `block_on_in_thread` — async directly on Tokio runtime.

Hot path (`BuildContext` <50ms) never touches DBOS — `RwLock` read path is unchanged.

## Approval UX

```bash
coderun workflow start "refactor auth" --require-approval   # → wf_abc status=awaiting_approval
coderun workflow status wf_abc
coderun workflow approve wf_abc                             # POST /workflow/{id}/approve → completed
coderun workflow list                                       # local DB `workflows` table fallback
```

Slack webhook: configure `workflow/dbos/src/main.ts` to call Slack before `waitForSignal`.

## Audit & recovery

`crates/coderun-storage/src/migrations/005_audits.sql` → `audits` (workflow_id, correlation_id, actor, task, ctx_pack_hash) + `workflows`. Kill sidecar mid-`sleep` → Litestream WAL replay on restart → `get_status` returns same `workflow_id` (idempotency via `DBOS.workflowID()`).

All payloads pass `redact_secrets()` before `audits.payload`.

## HMAC

Set `workflow.dbos_shared_secret` / `CODERUN_DBOS_SECRET`; daemon verifies `X-Coderun-Signature: hex(HMAC-SHA256(secret, body))` via `hmac` crate `Hmac<Sha256>` `coderun-core/src/secrets.rs:verify_hmac` (was `sha256(secret+body)` before v0.6.0; see `docs/V0_6_0_PLAN.md:1.1`). Single impl shared with `daemon/src/ratelimit.rs`. For local sidecar the installer default `your-secret` is sufficient — no real token needed for local development.

## When you DO need a token or key

You will only need authentication credentials if you decide to connect your local app to external cloud services:

- **DBOS Cloud CLI:** If you run `dbos-cloud login` to deploy your application to their managed hosting, it will generate a login token stored in your local `.dbos/` directory.
- **DBOS Conductor:** If you want to connect your local application to the DBOS web console for tracking and visualizing workflows, you must generate a `DBOS_CONDUCTOR_KEY` from their console dashboard.

Otherwise no token/key is required — keep the local default.

## Monitoring

`GET /metrics` exposes `coderun_requests_total`, `coderun_build_context_duration_seconds`, `coderun_fail_open_total`. Grafana `docs/dashboards/coderun.json` + `deploy/prometheus/alerts.yml`.
