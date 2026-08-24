import express from "express";

// v0.4.0 DBOS sidecar — scaffold without hard DBOS dependency for local dev.
// When `dbos-transact` is installed, replace mock with: import { DBOS } from "dbos-transact";
// For now, implements HTTP contract expected by Rust `DBOSWorkflowEngine` (`crates/coderun-workflow/src/dbos.rs`).

const app = express();
app.use(express.json());

type WorkflowRecord = { workflow_id: string; status: string; task: string; created_at: string; updated_at: string };
const workflows = new Map<string, WorkflowRecord>();

app.get("/health", (_req, res) => res.json({ status: "ok", version: "0.4.0", engine: "dbos-mock" }));

app.post("/workflow/start", (req, res) => {
  const { workflow_id, task, session_id, require_approval } = req.body;
  const id = workflow_id || `wf_${Date.now()}`;
  const record: WorkflowRecord = {
    workflow_id: id,
    status: require_approval ? "awaiting_approval" : "running",
    task: task || "",
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  };
  workflows.set(id, record);
  // Simulate durable work: fetch BuildContext from Rust daemon via UDS→HTTP bridge (stub)
  // In real DBOS: await DBOS.communicator(() => fetch("http://127.0.0.1:9527/hook", ...))
  // and await DBOS.transaction((tx) => tx.execute("INSERT INTO audits ..."))
  console.log(`[dbos] start ${id} task=${task} session=${session_id} require_approval=${require_approval}`);
  res.json({ workflow_id: id, status: record.status });
});

app.get("/workflow/:id", (req, res) => {
  const rec = workflows.get(req.params.id);
  if (!rec) return res.status(404).json({ error: "not found" });
  res.json(rec);
});

app.post("/workflow/:id/approve", (req, res) => {
  const rec = workflows.get(req.params.id);
  if (!rec) return res.status(404).json({ error: "not found" });
  rec.status = "running";
  rec.updated_at = new Date().toISOString();
  // In real DBOS: DBOS.sendSignal(id, "approved", req.body)
  console.log(`[dbos] approve ${req.params.id}`);
  // Simulate completion after approval
  setTimeout(() => {
    rec.status = "completed";
    rec.updated_at = new Date().toISOString();
  }, 100);
  res.json(rec);
});

app.get("/workflow", (_req, res) => res.json(Array.from(workflows.values())));

const port = process.env.DBOS_PORT ? parseInt(process.env.DBOS_PORT, 10) : 3001;
app.listen(port, () => console.log(`[dbos-sidecar] listening on :${port} (mock — install dbos-transact for durable)`));
