/**
 * Knocode AI Runtime Plugin for OpenCode
 *
 * Intercepts OpenCode hooks to enrich prompts and compress tool outputs
 * via the Knocode daemon (HTTP POST /hook).
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
/** How long the first request waits for the daemon to finish indexing before fail-open. */
export const DEFAULT_READY_TIMEOUT_MS = 10_000;
export const DEFAULT_READY_POLL_MS = 250;

export function getDaemonUrl(): string {
  return process.env.KNOCODE_DAEMON_URL || DEFAULT_DAEMON_URL;
}

export function getTimeoutMs(): number {
  const raw = process.env.KNOCODE_TIMEOUT_MS;
  if (raw) {
    const n = Number(raw);
    if (Number.isFinite(n) && n > 0) return n;
  }
  return DEFAULT_TIMEOUT_MS;
}

export function getReadyTimeoutMs(): number {
  const raw = process.env.KNOCODE_READY_TIMEOUT_MS;
  if (raw) {
    const n = Number(raw);
    if (Number.isFinite(n) && n > 0) return n;
  }
  return DEFAULT_READY_TIMEOUT_MS;
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface KnocodeRequest {
  hook_type: "PreGeneration" | "PreToolCall";
  payload: {
    type: "MessageRewrite" | "ToolOutput";
    session_id?: string;
    message?: string;
    tool_name?: string;
    output_type?: string;
    content?: string;
    context?: string;
    /** Absolute workspace root of the agent's project — daemon scopes retrieval to it (TASK-036) */
    repository_path?: string;
  };
  repository_id?: string;
  timestamp?: string;
}

export interface KnocodeResponse {
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
 * Wait until the daemon reports ready via `GET /health` (parity with the UDS Probe).
 *
 * The HTTP health/metrics listener binds BEFORE the initial index, so during a cold
 * start `/health` answers `{"state": "indexing"}` and `POST /hook` returns 503
 * (`daemon_indexing`) — polling here means the first real request gets context
 * instead of an instant passthrough.
 *
 * Returns:
 *  - `true`  once `/health` reports `state: "ready"` (a 200 without a parseable
 *            state is treated as ready — a live daemon is better than a strict one)
 *  - `false` when the daemon is UNREACHABLE (connection refused — not running),
 *            so a missing daemon never stalls a hook for the full budget
 *  - `false` when the budget (`timeoutMs`) expires while the daemon keeps indexing
 *
 * Fail-open: callers proceed with the POST regardless and rely on passthrough.
 */
export async function waitForDaemonReady(
  opts: { url?: string; timeoutMs?: number; pollMs?: number; fetchImpl?: typeof fetch } = {},
): Promise<boolean> {
  const url = opts.url ?? getDaemonUrl();
  const timeoutMs = opts.timeoutMs ?? getReadyTimeoutMs();
  const pollMs = opts.pollMs ?? DEFAULT_READY_POLL_MS;
  const fetchFn = opts.fetchImpl ?? fetch;
  const deadline = Date.now() + timeoutMs;

  while (Date.now() < deadline) {
    const remaining = deadline - Date.now();
    try {
      const res = await fetchFn(`${url}/health`, {
        // Per-poll cap (≤2s): a local daemon answers in ms; a stall means it is not healthy.
        signal: AbortSignal.timeout(Math.min(2_000, Math.max(1, remaining))),
      });
      if (res.ok) {
        let state: string | undefined;
        try {
          const body: any = await res.json();
          state = body?.state;
        } catch {
          // 200 with a non-JSON body — daemon is up; treat as ready.
        }
        if (state === undefined || state === "ready") return true;
        // Reachable but still indexing — keep polling until the deadline.
      }
      // Reachable with an error status — keep polling until the deadline.
    } catch {
      // Unreachable (connection refused / aborted) — the daemon is not running.
      return false;
    }
    await new Promise((r) => setTimeout(r, pollMs));
  }
  return false;
}

/**
 * Call the Knocode daemon HTTP `/hook` endpoint.
 * Returns null on any failure (fail-open).
 *
 * KEPT as the backward-compatibility fallback: older daemons that predate the MCP
 * route (`POST /mcp`) are still served through this legacy wire format.
 */
export async function callKnocodeDaemon(
  request: KnocodeRequest,
  opts?: { url?: string; timeoutMs?: number; fetchImpl?: typeof fetch },
): Promise<KnocodeResponse | null> {
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
      console.error(`[knocode] Daemon returned ${response.status}`);
      return null;
    }

    return (await response.json()) as KnocodeResponse;
  } catch (error) {
    console.error(`[knocode] Daemon unreachable: ${error}`);
    return null;
  }
}

// ---------------------------------------------------------------------------
// MCP client — JSON-RPC 2.0 over POST /mcp
// ---------------------------------------------------------------------------
// The daemon hosts an MCP surface on its HTTP listener (`POST /mcp`) so plugins can
// drive Knocode with typed tools (`tools/call`) instead of shaping prompts/answers
// into the legacy MessageRewrite/ToolOutput wire payloads — no conversions on either
// side. This client is stateless (the daemon's MCP subset needs no session) and
// fail-open: any failure → passthrough, exactly like the old /hook behavior.

export type McpCallOutcome =
  | { kind: "ok"; result: any }
  | { kind: "error"; code: number; message: string }
  // The daemon predates the /mcp route (HTTP 404/405) — the caller falls back to /hook.
  | { kind: "unsupported"; status: number }
  | { kind: "failure"; reason: string };

let mcpRequestId = 0;

/**
 * Send one JSON-RPC request to the daemon's `POST /mcp` endpoint.
 */
export async function mcpCall(
  method: string,
  params: any,
  opts?: { url?: string; timeoutMs?: number; fetchImpl?: typeof fetch },
): Promise<McpCallOutcome> {
  const url = opts?.url ?? getDaemonUrl();
  const timeoutMs = opts?.timeoutMs ?? getTimeoutMs();
  const fetchFn = opts?.fetchImpl ?? fetch;
  const id = ++mcpRequestId;

  try {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), timeoutMs);

    const res = await fetchFn(`${url}/mcp`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id, method, params }),
      signal: controller.signal,
    });

    clearTimeout(timeout);

    if (res.status === 404 || res.status === 405) {
      return { kind: "unsupported", status: res.status };
    }
    if (!res.ok) {
      return { kind: "failure", reason: `HTTP ${res.status}` };
    }

    let body: any;
    try {
      body = await res.json();
    } catch {
      return { kind: "failure", reason: "non-JSON /mcp response" };
    }
    // JSON-RPC application error (e.g. -32001 daemon_indexing) — NOT "unsupported":
    // the daemon speaks MCP, it just can't serve this call right now.
    if (body?.error) {
      return { kind: "error", code: body.error.code, message: body.error.message };
    }
    if (body?.result === undefined) {
      return { kind: "failure", reason: "malformed JSON-RPC response" };
    }
    return { kind: "ok", result: body.result };
  } catch (error) {
    console.error(`[knocode] Daemon unreachable: ${error}`);
    return { kind: "failure", reason: String(error) };
  }
}

/**
 * Pre-model context enrichment: drive `knocode_context` via MCP. Returns the enriched
 * text to substitute for the user message, or `null` for an untouched passthrough
 * (no context hits, indexing in progress, daemon down, or a legacy daemon whose
 * /mcp route is missing — the latter falls back to the /hook rewrite path).
 */
export async function requestContextEnrichment(
  message: string,
  repositoryPath: string,
  sessionId: string = "unknown",
  opts?: { url?: string; timeoutMs?: number; fetchImpl?: typeof fetch },
): Promise<string | null> {
  const out = await mcpCall(
    "tools/call",
    {
      name: "knocode_context",
      arguments: { prompt: message, repository_path: repositoryPath },
    },
    opts,
  );

  switch (out.kind) {
    case "ok": {
      const text: string | undefined = out.result?.content?.[0]?.text;
      const passthrough = out.result?.structuredContent?.passthrough === true;
      const isError = out.result?.isError === true;
      if (!text || passthrough || isError) return null;
      return text;
    }
    case "unsupported": {
      // Legacy daemon — enrich via the /hook MessageRewrite contract.
      const request: KnocodeRequest = {
        hook_type: "PreGeneration",
        payload: {
          type: "MessageRewrite",
          session_id: sessionId,
          message,
          repository_path: repositoryPath,
        },
        repository_id: hashRepositoryId(repositoryPath),
        timestamp: new Date().toISOString(),
      };
      const response = await callKnocodeDaemon(request, opts);
      if (response?.payload.type === "RewrittenMessage" && response.payload.rewritten) {
        return response.payload.rewritten;
      }
      return null;
    }
    default:
      // error (e.g. -32001 while indexing) or failure — untouched passthrough.
      return null;
  }
}

/**
 * Tool-output compression: drive `knocode_compress` via MCP. Returns the compressed
 * text (+ token counts for the savings log) or `null` for passthrough. Legacy daemons
 * (no /mcp route) fall back to the /hook ToolOutput contract.
 */
export async function requestOutputCompression(
  content: string,
  toolName: string,
  repositoryPath: string,
  opts?: { url?: string; timeoutMs?: number; fetchImpl?: typeof fetch },
): Promise<{ compressed: string; originalTokens?: number; compressedTokens?: number } | null> {
  const out = await mcpCall(
    "tools/call",
    {
      name: "knocode_compress",
      arguments: {
        content,
        tool_name: toolName,
        output_type: getOutputType(toolName),
      },
    },
    opts,
  );

  switch (out.kind) {
    case "ok": {
      const text: string | undefined = out.result?.content?.[0]?.text;
      if (!text || out.result?.isError === true) return null;
      const s = out.result?.structuredContent;
      return {
        compressed: text,
        originalTokens: s?.original_tokens,
        compressedTokens: s?.compressed_tokens,
      };
    }
    case "unsupported": {
      // Legacy daemon — compress via the /hook ToolOutput contract.
      const request: KnocodeRequest = {
        hook_type: "PreToolCall",
        payload: {
          type: "ToolOutput",
          tool_name: toolName,
          output_type: getOutputType(toolName),
          content,
          repository_path: repositoryPath,
        },
        repository_id: hashRepositoryId(repositoryPath),
        timestamp: new Date().toISOString(),
      };
      const response = await callKnocodeDaemon(request, opts);
      if (response?.payload.type === "CompressedOutput" && response.payload.compressed) {
        return {
          compressed: response.payload.compressed,
          originalTokens: response.payload.original_tokens,
          compressedTokens: response.payload.compressed_tokens,
        };
      }
      return null;
    }
    default:
      return null;
  }
}

function hashRepositoryId(path: string): string {
  // Informational trace id (TASK-021). NOTE: the daemon derives its authoritative
  // repository_id from payload.repository_path via a shared SHA-256 formula (TASK-036);
  // this JS hash is NOT used for retrieval scoping.
  let hash = 0;
  for (let i = 0; i < path.length; i++) {
    hash = (hash * 31 + path.charCodeAt(i)) >>> 0;
  }
  return hash.toString(16).padStart(8, "0");
}

export const KnocodePlugin: Plugin = async ({ project, client, $, directory, worktree }: any) => {
  // TASK-036/F-7: the agent's active workspace root — sent with EVERY hook payload so ONE
  // shared daemon serves multiple opencode windows on different repos simultaneously.
  const repositoryPath: string = worktree || directory || project?.worktree || process.cwd();

  // --- MCP Initialize ----------------------------------------------------
  // Formal MCP handshake: initialize + notifications/initialized. This verifies
  // the daemon speaks MCP, retrieves protocol version and capabilities, and
  // signals the plugin is ready. Fail-open: if daemon is unreachable or legacy,
  // we proceed without initialization (hooks will use /hook fallback).
  let mcpInitialized = false;
  let daemonVersion: string | undefined;

  async function initializeMcp(): Promise<void> {
    const out = await mcpCall(
      "initialize",
      {
        protocolVersion: "2024-11-05",
        capabilities: {},
        clientInfo: { name: "opencode-knocode", version: "0.9.11" },
      },
    );

    if (out.kind === "ok") {
      mcpInitialized = true;
      daemonVersion = out.result?.serverInfo?.version;
      console.log(`[knocode] MCP initialized (daemon v${daemonVersion || "unknown"})`);

      // Send initialized notification
      await mcpCall("notifications/initialized", {});
    } else if (out.kind === "unsupported") {
      console.log("[knocode] Daemon does not support MCP — using /hook fallback");
    } else {
      console.log(`[knocode] MCP init failed: ${out.kind === "error" ? out.message : out.reason}`);
    }
  }

  // Try to initialize MCP on startup (non-blocking, fail-open)
  initializeMcp().catch(() => {});

  // Optional structured log when client is available
  try {
    await client?.app?.log?.({
      body: {
        service: "opencode-knocode",
        level: "info",
        message: "Plugin initialized",
        extra: { directory, worktree, repositoryPath, daemonUrl: getDaemonUrl() },
      },
    });
  } catch {
    // ignore logging failures
  }
  console.log("[knocode] Plugin initialized");

  // --- Readiness gate -----------------------------------------------------
  // The HTTP health/metrics listener binds BEFORE the initial index, so the daemon
  // answers `/health` with `state: "indexing"` (and 503s `/hook`) during a cold
  // start. We wait for readiness once, then skip re-polling for a cooldown window so
  // the check never adds latency to every message. Results are cached unconditionally:
  // once the daemon is actually ready a skipped gate doesn't matter (the POST succeeds
  // on its own), and a mid-index daemon only costs the wait budget once per window.
  const READY_REPOLL_MS = 30_000;
  let readinessCheckedAt = 0;

  async function ensureDaemonReady(): Promise<void> {
    const now = Date.now();
    if (now - readinessCheckedAt < READY_REPOLL_MS) return;
    const waitStartedAt = Date.now();
    const ready = await waitForDaemonReady();
    readinessCheckedAt = Date.now();
    if (ready && Date.now() - waitStartedAt > 1_000) {
      console.log(`[knocode] Daemon became ready after ${Date.now() - waitStartedAt}ms`);
    }
  }

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

    // First real request of a session may hit a daemon mid-index (cold start or
    // auto-reindex); wait (bounded, fail-open) so this message gets context.
    await ensureDaemonReady();

    // Pre-model enrichment over the daemon MCP surface (typed tool call, no prompt
    // conversions). Legacy daemons without /mcp fall back to the /hook rewrite path
    // inside requestContextEnrichment — behavior is identical either way.
    const rewritten = await requestContextEnrichment(text, repositoryPath, input.session_id || "unknown");
    if (rewritten != null) {
      // Mutate in place — opencode reads input.message after hook
      if (typeof msg.content === "string") {
        msg.content = rewritten;
      } else {
        // Replace text parts with single enriched part
        msg.content = [{ type: "text", text: rewritten }];
      }
      console.log("[knocode] Enriched message");
    }
    // Passthrough (no_context_hits / indexing / unreachable): leave the user's prompt
    // byte-identical — no metadata-only rewrite.
  }

  return {
    // ── Hooks — automatic enrichment/compression ─────────────────────────
    // The agent never explicitly calls knocode — everything happens transparently.
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

      // Compression over the daemon MCP surface; legacy /hook fallback inside.
      const result = await requestOutputCompression(content, toolName, repositoryPath);
      if (result && result.compressed !== content) {
        output.result = result.compressed;
        const savings =
          result.originalTokens && result.compressedTokens
            ? `${Math.round((1 - result.compressedTokens / result.originalTokens) * 100)}%`
            : "unknown";
        console.log(`[knocode] Compressed ${toolName} output (${savings} savings)`);
      }
    },
  };
};

// Default export for opencode auto-discovery (both named and default work)
export default KnocodePlugin;
