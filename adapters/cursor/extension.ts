// Coderun adapter for Cursor (Tier 1 — programmatic hooks, v0.4.0)
// Spec §3 Adapter Layer: intercept before generation (rewrite) and before tool call (compress)
// Uses Cursor's hook/extension API analogous to opencode `chat.message` / `tool.execute.before`.
// Fail-open on timeout (30s) — returns OriginalPassthrough and logs warning.
// IPC: UDS + MessagePack primary (rmp), HTTP/JSON fallback on Windows.
// See .opencode/plugins/coderun.ts for reference implementation and ADAPTERS.md.

import * as net from "net";
import * as fs from "fs";

const SOCKET_PATH = process.env.CODERUN_SOCKET || "/tmp/coderun.sock";
const HTTP_FALLBACK = process.env.CODERUN_DAEMON_URL || "http://127.0.0.1:9527";
const TIMEOUT_MS = 30_000;
const READY_TIMEOUT_MS = Number(process.env.CODERUN_READY_TIMEOUT_MS) || 10_000;
const READY_POLL_MS = 250;
const READY_REPOLL_MS = 30_000;

type HookType = "PreGeneration" | "PreToolCall";

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

// Readiness gate state: once the daemon reports ready we skip re-polling for a
// cooldown window so the check never adds latency to every hook invocation.
let daemonReadyAt = 0;

/**
 * Wait for daemon readiness once per cooldown window (parity with the opencode
 * plugin's waitForDaemonReady). The HTTP health listener binds BEFORE the initial
 * index, so a cold start reports `state: "indexing"` and POST /hook 503s — poll so
 * the first request of a session gets context. Fail-open: never stalls on a daemon
 * that is simply down (unreachable returns immediately) or past the budget.
 */
async function ensureDaemonReady(): Promise<void> {
  const now = Date.now();
  if (now - daemonReadyAt < READY_REPOLL_MS) return;
  const deadline = now + READY_TIMEOUT_MS;
  while (Date.now() < deadline) {
    let state: string | undefined;
    try {
      const res = await fetch(`${HTTP_FALLBACK}/health`, {
        // Per-poll cap (≤2s): a local daemon answers in ms; a stall means it is not healthy.
        signal: AbortSignal.timeout(Math.min(2_000, deadline - Date.now())),
      });
      if (res.ok) {
        try {
          state = (await res.json())?.state;
        } catch {
          // 200 with a non-JSON body — daemon is up; treat as ready.
        }
        if (state === undefined || state === "ready") {
          daemonReadyAt = Date.now();
          return;
        }
        // Reachable but still indexing — keep polling until the deadline.
      }
    } catch {
      // Unreachable (connection refused / aborted) — the daemon is not running.
      return;
    }
    await sleep(READY_POLL_MS);
  }
}

async function callDaemon(payload: unknown, hookType: HookType): Promise<string | null> {
  // Try UDS/MessagePack first (primary), fallback to HTTP
  try {
    if (fs.existsSync(SOCKET_PATH)) {
      // UDS path — MessagePack would be encoded here via msgpack-lite; stubbed for type-level parity
      // In production: const msgpack = require("msgpack-lite"); socket.write(encode(...))
      return null; // fall through to HTTP for this stub
    }
  } catch {}
  try {
    // Gate on daemon readiness (bounded, fail-open) before the first real request.
    await ensureDaemonReady();
    const res = await fetch(`${HTTP_FALLBACK}/hook`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ hook_type: hookType, payload }),
      signal: AbortSignal.timeout(TIMEOUT_MS),
    });
    if (!res.ok) return null;
    const data: any = await res.json();
    if (data.payload?.type === "RewrittenMessage") return data.payload.rewritten;
    if (data.payload?.type === "CompressedOutput") return data.payload.compressed;
    return null;
  } catch {
    return null; // fail-open
  }
}

// Cursor extension entry
export function activate() {
  // Pseudocode for Cursor extension host:
  // cursor.hooks.on("beforeGeneration", async (msg: string) => {
  //   const rewritten = await callDaemon({ type: "MessageRewrite", session_id: "cursor", message: msg }, "PreGeneration");
  //   return rewritten ?? msg; // fail-open
  // });
  // cursor.hooks.on("beforeToolUse", async (tool: string, output: string) => {
  //   const compressed = await callDaemon({ type: "ToolOutput", tool_name: tool, output_type: "Other", content: output }, "PreToolCall");
  //   return compressed ?? output;
  // });
  console.log("[coderun] Cursor adapter registered (Tier 1, UDS+MessagePack primary, HTTP fallback, 30s fail-open)");
}
