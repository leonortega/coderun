# DBOS Workflows (v0.4.0)

DBOS Transact is the **optional** durable orchestrator. Runtime never requires it (`IWorkflowEngine` is `Noop` by default). Enable only when you need approvals, audit, or multi-step governance.

## When to enable

```toml
[workflow]
enabled = true
engine = "dbos"
dbos_endpoint = "http://localhost:3001"
auto_governance = false            # true → tier=capable tasks auto-start workflow
require_approval_tiers = ["capable"]
```

Or env: `CODERUN_WORKFLOW_ENABLED=true CODERUN_DBOS_ENDPOINT=http://localhost:3001`.

## Architecture

Rust daemon (`DBOSWorkflowEngine` `crates/coderun-workflow/src/dbos.rs`) POSTs to Node sidecar `workflow/dbos/src/main.ts` (`POST /workflow/start`). Sidecar runs `DBOS.workflow(governedWorkflow)` which: `BuildContext` → `awaitApproval` (if `require_approval`) → `DBOS.transaction(INSERT INTO audits)`. SQLite `~/.coderun/dbos.db` WAL + Litestream replica.

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

Set `workflow.dbos_shared_secret` / `CODERUN_DBOS_SECRET`; daemon verifies `X-Coderun-Signature: hex(sha256(secret+body))` `daemon/src/ratelimit.rs:58`.

## Monitoring

`GET /metrics` exposes `coderun_requests_total`, `coderun_build_context_duration_seconds`, `coderun_fail_open_total`. Grafana `docs/dashboards/coderun.json` + `deploy/prometheus/alerts.yml`.
