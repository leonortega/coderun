-- Migration 005: Audits + workflows for DBOS orchestration (v0.4.0 §1.3)
-- Structured audit log for every request + durable workflow state
CREATE TABLE IF NOT EXISTS audits (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workflow_id TEXT,
    correlation_id TEXT,
    actor TEXT,
    task TEXT NOT NULL,
    ctx_pack_hash TEXT,
    payload TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_audits_workflow ON audits(workflow_id);
CREATE INDEX IF NOT EXISTS idx_audits_correlation ON audits(correlation_id);
CREATE INDEX IF NOT EXISTS idx_audits_created ON audits(created_at);

CREATE TABLE IF NOT EXISTS workflows (
    workflow_id TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    task TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_workflows_status ON workflows(status);
