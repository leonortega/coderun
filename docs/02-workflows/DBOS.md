# DBOS Workflows (v0.6.0 — required)

DBOS Transact is the durable orchestrator (was required since v0.6.0, isolated to `future/workflow/` as of v0.7.5). Runtime uses SQLite + Litestream single-node durability (`sqlite://~/.knocode/dbos.db` + `sqlite://~/.knocode/dbos_system.db`, `DBOS_LITESTREAM_REPLICA_URL` for replica). `IWorkflowEngine` is `async_trait` native. See `future/workflow/dbos/dbos-config.yaml` (prod = SQLite, dev alias `dbos-config-dev.yaml`).

## When to enable

```toml
[workflow]
enabled = true              # true default since v0.6.0 (was false in v0.4-0.5)
engine = "dbos"
dbos_endpoint = "http://localhost:3001"
auto_governance = false            # true → tier=capable tasks auto-start workflow
require_approval_tiers = ["capable"]
```

Or env: `KNOCODE_WORKFLOW_ENABLED=true KNOCODE_DBOS_ENDPOINT=http://localhost:3001` (v0.6.0 `enabled` defaults `true`; set `false` only for tests with `#[cfg(test)] NoopWorkflowEngine`).

## Architecture

Rust daemon (`DBOSWorkflowEngine` `crates/knocode-workflow/src/dbos.rs`) async `#[async_trait] IWorkflowEngine` POSTs to Node sidecar `workflow/dbos/src/main.ts` (`POST /workflow/start`) via `tokio::time::timeout(5s)`. Sidecar runs `DBOS.workflow(governedWorkflow)` native `dbos-transact` (`DBOS.communicator(BuildContext)` → `DBOS.waitForSignal("approved",86400)` if `require_approval` → `DBOS.transaction(INSERT INTO audits)`). SQLite `~/.knocode/dbos.db` WAL + Litestream replica (`DBOS_LITESTREAM_REPLICA_URL=s3://…` or `file://~/.knocode/replica`). No `block_on_in_thread` — async directly on Tokio runtime.

Hot path (`BuildContext` <50ms) never touches DBOS — `RwLock` read path is unchanged.

## Approval UX

```bash
knocode workflow start "refactor auth" --require-approval   # → wf_abc status=awaiting_approval
knocode workflow status wf_abc
knocode workflow approve wf_abc                             # POST /workflow/{id}/approve → completed
knocode workflow list                                       # local DB `workflows` table fallback
```

Slack webhook: configure `workflow/dbos/src/main.ts` to call Slack before `waitForSignal`.

## Audit & recovery

`crates/knocode-storage/src/migrations/005_audits.sql` → `audits` (workflow_id, correlation_id, actor, task, ctx_pack_hash) + `workflows`. Kill sidecar mid-`sleep` → Litestream WAL replay on restart → `get_status` returns same `workflow_id` (idempotency via `DBOS.workflowID()`).

All payloads pass `redact_secrets()` before `audits.payload`.

## HMAC

Set `workflow.dbos_shared_secret` / `KNOCODE_DBOS_SECRET`; daemon verifies `X-Knocode-Signature: hex(HMAC-SHA256(secret, body))` via `hmac` crate `Hmac<Sha256>` `knocode-core/src/secrets.rs:verify_hmac`. Single impl shared with `daemon/src/ratelimit.rs`. For local sidecar the installer default `your-secret` is sufficient — no real token needed for local development.

## When you DO need a token or key

You will only need authentication credentials if you decide to connect your local app to external cloud services:

- **DBOS Cloud CLI:** If you run `dbos-cloud login` to deploy your application to their managed hosting, it will generate a login token stored in your local `.dbos/` directory.
- **DBOS Conductor:** If you want to connect your local application to the DBOS web console for tracking and visualizing workflows, you must generate a `DBOS_CONDUCTOR_KEY` from their console dashboard.

Otherwise no token/key is required — keep the local default.

## Monitoring

`GET /metrics` exposes `knocode_requests_total`, `knocode_build_context_duration_seconds`, `knocode_fail_open_total`. Grafana `docs/dashboards/knocode.json` + `deploy/prometheus/alerts.yml`.
