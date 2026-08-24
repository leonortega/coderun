// Real DBOS workflow — activate when `dbos-transact` is installed.
// import { DBOS } from "dbos-transact";
//
// export const governedWorkflow = DBOS.workflow(async (task: { message: string; session_id: string }, opts: { requireApproval: boolean }) => {
//   // Step 1: BuildContext via Rust daemon (HTTP bridge to UDS)
//   const ctx = await DBOS.communicator(async () => {
//     const body = JSON.stringify({ hook_type: "PreGeneration", payload: { type: "MessageRewrite", session_id: task.session_id, message: task.message } });
//     const resp = await fetch("http://127.0.0.1:9527/hook", { method: "POST", headers: { "Content-Type": "application/json" }, body });
//     return await resp.json();
//   });
//
//   // Step 2: Approval gate (durable sleep + signal)
//   if (opts.requireApproval) {
//     await DBOS.sleep(0); // yield to allow signal
//     await DBOS.waitForSignal("approved", 24 * 3600); // 24h timeout
//   }
//
//   // Step 3: Audit (durable transaction)
//   await DBOS.transaction(async (tx: any) => {
//     await tx.execute("INSERT INTO audits (workflow_id, task, ctx_pack_hash) VALUES (?,?,?)", [DBOS.workflowID(), task.message, "hash_stub"]);
//   });
//
//   return ctx;
// });
//
// export const approveSignal = DBOS.signalHandler(async (workflowId: string) => {
//   await DBOS.sendSignal(workflowId, "approved", {});
// });

export const placeholder = "install dbos-transact to enable durable governedWorkflow";
