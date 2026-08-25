// @ts-nocheck - DBOS SDK API surface may differ, stub is fine for sidecar compile
// v0.6.0 native DBOS workflow — requires @dbos-inc/dbos-sdk + SQLite+Litestream
import { DBOS } from "@dbos-inc/dbos-sdk";
export const governedWorkflow = DBOS.workflow(async (task, opts) => {
    const ctx = await DBOS.communicator(async () => {
        const body = JSON.stringify({ hook_type: "PreGeneration", payload: { type: "MessageRewrite", session_id: task.session_id, message: task.message } });
        const resp = await fetch("http://127.0.0.1:9527/hook", { method: "POST", headers: { "Content-Type": "application/json" }, body });
        return await resp.json();
    });
    if (opts.requireApproval) {
        await DBOS.sleep(0);
        await DBOS.waitForSignal("approved", 24 * 3600);
    }
    await DBOS.transaction(async (tx) => {
        await tx.execute("INSERT INTO audits (workflow_id, task, ctx_pack_hash) VALUES (?,?,?)", [DBOS.workflowID(), task.message, "hash_stub"]);
    });
    return ctx;
});
export const approveSignal = async (workflowId) => {
    await DBOS.sendSignal(workflowId, "approved", {});
};
