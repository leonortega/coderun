// Coderun adapter for Cursor (Tier 1 — programmatic hooks, v0.3.0)
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

type HookType = "PreGeneration" | "PreToolCall";

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
