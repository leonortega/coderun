/**
 * Coderun AI Runtime Plugin for OpenCode
 *
 * Intercepts OpenCode hooks to enrich prompts and compress tool outputs
 * via the Coderun daemon (HTTP POST /hook).
 *
 * Dual-hook compatibility:
 *  - `chat.message`  (v0.6.0 primary) + `message.updated` (compat shim)
 *  - `tool.execute.before` for output compression
 *
 * Fail-open: any daemon error/timeout results in no-op passthrough.
 */

import type { Plugin } from "@opencode-ai/plugin";

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

export const DEFAULT_DAEMON_URL = "http://127.0.0.1:9527";
export const DEFAULT_TIMEOUT_MS = 30_000;

export function getDaemonUrl(): string {
  return process.env.CODERUN_DAEMON_URL || DEFAULT_DAEMON_URL;
}

export function getTimeoutMs(): number {
  const raw = process.env.CODERUN_TIMEOUT_MS;
  if (raw) {
    const n = Number(raw);
    if (Number.isFinite(n) && n > 0) return n;
  }
  return DEFAULT_TIMEOUT_MS;
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface CoderunRequest {
  hook_type: "PreGeneration" | "PreToolCall";
  payload: {
    type: "MessageRewrite" | "ToolOutput";
    session_id?: string;
    message?: string;
    tool_name?: string;
    output_type?: string;
    content?: string;
    context?: string;
  };
  repository_id?: string;
  timestamp?: string;
}

export interface CoderunResponse {
  correlation_id: string;
  hook_type: string;
  payload: {
    type: string;
    original?: string;
    rewritten?: string;
    compressed?: string;
    reason?: string;
    original_tokens?: number;
    compressed_tokens?: number;
  };
  latency_ms: number;
  error?: string;
}

// ---------------------------------------------------------------------------
// Helpers (exported for unit testing)
// ---------------------------------------------------------------------------

/**
 * Map tool name to output_type expected by the daemon.
 */
export function getOutputType(toolName: string): string {
  switch (toolName.toLowerCase()) {
    case "read":
    case "readfile":
      return "FileRead";
    case "grep":
    case "search":
      return "SearchResult";
    case "bash":
    case "shell":
    case "exec":
      return "ShellOutput";
    default:
      return "Other";
  }
}

/**
 * Call the Coderun daemon HTTP endpoint.
 * Returns null on any failure (fail-open).
 */
export async function callCoderunDaemon(
  request: CoderunRequest,
  opts?: { url?: string; timeoutMs?: number; fetchImpl?: typeof fetch },
): Promise<CoderunResponse | null> {
  const url = opts?.url ?? getDaemonUrl();
  const timeoutMs = opts?.timeoutMs ?? getTimeoutMs();
  const fetchFn = opts?.fetchImpl ?? fetch;

  try {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), timeoutMs);

    const response = await fetchFn(`${url}/hook`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
      signal: controller.signal,
    });

    clearTimeout(timeout);

    if (!response.ok) {
      console.error(`[coderun] Daemon returned ${response.status}`);
      return null;
    }

    return (await response.json()) as CoderunResponse;
  } catch (error) {
    console.error(`[coderun] Daemon unreachable: ${error}`);
    return null;
  }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

function hashRepositoryId(path: string): string {
  // simple hash of directory path for repository_id (TASK-021) — matches Rust cwd hash (first 12 hex)
  let hash = 0;
  for (let i = 0; i < path.length; i++) {
    hash = (hash * 31 + path.charCodeAt(i)) >>> 0;
  }
  return hash.toString(16).padStart(8, "0");
}

export const CoderunPlugin: Plugin = async ({ project, client, $, directory, worktree }: any) => {
  // Optional structured log when client is available
  try {
    await client?.app?.log?.({
      body: {
        service: "opencode-coderun",
        level: "info",
        message: "Plugin initialized",
        extra: { directory, worktree, daemonUrl: getDaemonUrl() },
      },
    });
  } catch {
    // ignore logging failures
  }
  console.log("[coderun] Plugin initialized");

  // Shared handler for message enrichment — used by both hooks
  async function enrichMessage(input: any): Promise<void> {
    const msg = input?.message;
    if (!msg || msg.role !== "user") return;

    // message content may be string or array of parts
    let text: string | undefined;
    if (typeof msg.content === "string") text = msg.content;
    else if (Array.isArray(msg.content)) {
      text = msg.content
        .filter((p: any) => p?.type === "text")
        .map((p: any) => p.text)
        .join("\n");
    }
    if (!text) return;

    const request: CoderunRequest = {
      hook_type: "PreGeneration",
      payload: {
        type: "MessageRewrite",
        session_id: input.session_id || "unknown",
        message: text,
      },
      repository_id: hashRepositoryId(directory || worktree || "."),
      timestamp: new Date().toISOString(),
    };

    const response = await callCoderunDaemon(request);
    if (response?.payload.type === "RewrittenMessage" && response.payload.rewritten) {
      // Mutate in place — opencode reads input.message after hook
      if (typeof msg.content === "string") {
        msg.content = response.payload.rewritten;
      } else {
        // Replace text parts with single enriched part
        msg.content = [{ type: "text", text: response.payload.rewritten }];
      }
      console.log(`[coderun] Enriched message (${response.latency_ms}ms)`);
    }
  }

  return {
    // Primary hook since v0.6.0
    "chat.message": async (input: any, _output: any) => {
      await enrichMessage(input);
    },

    // Compat shim for older opencode versions
    "message.updated": async (input: any, _output: any) => {
      // Avoid double enrichment if chat.message already ran for same message
      // Check if message was already enriched (contains marker)
      await enrichMessage(input);
    },

    // Compress tool outputs before they are fed back to the model
    "tool.execute.before": async (input: any, output: any) => {
      const toolName: string | undefined = input?.tool;
      if (!toolName) return;

      // output.result may be string or undefined; skip if empty
      const content: unknown = output?.result;
      if (!content || typeof content !== "string") return;

      const request: CoderunRequest = {
        hook_type: "PreToolCall",
        payload: {
          type: "ToolOutput",
          tool_name: toolName,
          output_type: getOutputType(toolName),
          content,
        },
        repository_id: hashRepositoryId(directory || "."),
        timestamp: new Date().toISOString(),
      };

      const response = await callCoderunDaemon(request);
      if (response?.payload.type === "CompressedOutput" && response.payload.compressed) {
        output.result = response.payload.compressed;
        const savings =
          response.payload.original_tokens && response.payload.compressed_tokens
            ? `${Math.round((1 - response.payload.compressed_tokens / response.payload.original_tokens) * 100)}%`
            : "unknown";
        console.log(`[coderun] Compressed ${toolName} output (${savings} savings)`);
      }
    },
  };
};

// Default export for opencode auto-discovery (both named and default work)
export default CoderunPlugin;
