#!/usr/bin/env node
/**
 * Knocode agent-hook handler for GitHub Copilot (VS Code).
 *
 * Reads the hook event JSON from stdin, calls the local Knocode daemon over MCP
 * (POST /mcp on http://127.0.0.1:9527), and writes a single JSON object to stdout
 * — the exact shape VS Code expects. It never prints anything else to stdout.
 *
 * Events (passed as argv[2], also read from hook_event_name):
 *   session-start   -> inject repository context via knocode_context  (additionalContext)
 *   pre-tool-use    -> enrich context for read/search tools via knocode_context
 *   post-tool-use   -> compress large tool outputs via knocode_compress and inject the digest
 *
 * The hooks that run this script can ONLY inject extra context or block — VS Code does
 * not expose a prompt-rewrite or tool-output-replacement hook. So this is the faithful
 * analog of the opencode plugin's `chat.message` (context) and `tool.execute.before`
 * (compression-as-context) behaviors, given the Copilot hook surface.
 *
 * Fail-open: any daemon error, timeout, indexing-in-progress (-32001), or missing tool
 * returns `{}` (no-op) and exits 0 — configured hooks never stall or break the agent.
 *
 * Env:
 *   KNOCODE_DAEMON_URL              daemon base URL            (default http://127.0.0.1:9527)
 *   KNOCODE_TIMEOUT_MS              per MCP call timeout (ms)  (default 15000)
 *   KNOCODE_READY_TIMEOUT_MS        session-start readiness    (default 5000, 0 disables)
 *   KNOCODE_HOOK_COMPRESS_MIN_CHARS min tool_response length to compress (default 2000)
 *
 * Requires Node.js >= 18 (global fetch + AbortSignal.timeout).
 */

import * as readline from "node:readline";

const DAEMON_URL = process.env.KNOCODE_DAEMON_URL || "http://127.0.0.1:9527";
const TIMEOUT_MS = num("KNOCODE_TIMEOUT_MS", 15000);
const READY_TIMEOUT_MS = num("KNOCODE_READY_TIMEOUT_MS", 5000);
const COMPRESS_MIN_CHARS = num("KNOCODE_HOOK_COMPRESS_MIN_CHARS", 2000);
const READY_POLL_MS = 250;

function num(env, def) {
  const n = Number(process.env[env]);
  return Number.isFinite(n) && n > 0 ? n : def;
}

/** Logs to stderr only — stdout is reserved for the hook JSON response. */
function log(...args) {
  try {
    process.stderr.write(`[knocode-hook] ${args.join(" ")}\n`);
  } catch { /* ignore */ }
}

// ---------------------------------------------------------------------------
// Daemon client (MCP)
// ---------------------------------------------------------------------------

let mcpRequestId = 0;

/**
 * Send one JSON-RPC request to the daemon's POST /mcp endpoint.
 * Returned shape lets callers fail open without throwing.
 */
async function mcpCall(method, params) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), TIMEOUT_MS);
  try {
    const res = await fetch(`${DAEMON_URL}/mcp`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: ++mcpRequestId, method, params }),
      signal: controller.signal,
    });
    clearTimeout(timer);
    if (res.status === 404 || res.status === 405) return { kind: "unsupported" };
    if (!res.ok) return { kind: "failure", reason: `HTTP ${res.status}` };
    const body = await res.json();
    if (body?.error) return { kind: "error", code: body.error.code, message: body.error.message };
    if (body?.result === undefined) return { kind: "failure", reason: "malformed JSON-RPC response" };
    return { kind: "ok", result: body.result };
  } catch (err) {
    clearTimeout(timer);
    return { kind: "failure", reason: String(err) };
  }
}

/** Wait (bounded, fail-open) until the daemon reports ready via GET /health. */
async function daemonReady(timeoutMs) {
  if (!timeoutMs) return true; // readiness disabled
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const remaining = deadline - Date.now();
    const controller = new AbortController();
    // Manual timeout (NOT AbortSignal.timeout): its native timer can crash libuv
    // on Windows when combined with process.exit() right after a response.
    const timer = setTimeout(() => controller.abort(), Math.min(2000, Math.max(1, remaining)));
    try {
      const res = await fetch(`${DAEMON_URL}/health`, { signal: controller.signal });
      clearTimeout(timer);
      if (res.ok) {
        let state;
        try { state = (await res.json())?.state; } catch { /* non-JSON body => ready */ }
        if (state === undefined || state === "ready") return true;
      }
    } catch {
      clearTimeout(timer);
      return false; // unreachable
    }
    await new Promise((r) => setTimeout(r, READY_POLL_MS));
  }
  return false;
}

// ---------------------------------------------------------------------------
// Tool mapping (mirrors packages/opencode-knocode/src/index.ts getOutputType)
// ---------------------------------------------------------------------------

const READ_LIKE = new Set(["read", "read_file", "readfile", "show_file", "open_file", "view", "cat"]);
const SEARCH_LIKE = new Set(["grep", "search", "grep_search", "search_pattern", "file_search", "glob", "list_dir"]);
const SHELL_LIKE = new Set(["bash", "shell", "exec", "run_in_terminal", "run_terminal", "terminal", "execute_command", "run_command", "worktree_run_command", "zsh"]);

function outputType(toolName) {
  const t = (toolName || "").toLowerCase();
  if (READ_LIKE.has(t)) return "FileRead";
  if (SEARCH_LIKE.has(t)) return "SearchResult";
  if (SHELL_LIKE.has(t)) return "ShellOutput";
  return "Other";
}

// ---------------------------------------------------------------------------
// Per-event handlers — each returns a hook output object or {} (no-op)
// ---------------------------------------------------------------------------

async function handleSessionStart(input) {
  if (READY_TIMEOUT_MS && !(await daemonReady(READY_TIMEOUT_MS))) {
    log("daemon not ready; skipping session context");
    return {};
  }
  const repositoryPath = input?.cwd || process.cwd();
  const probe =
    "Give a concise overview of this repository: project purpose, structure, key " +
    "modules, and conventions. Use this to seed the session context.";
  const out = await mcpCall("tools/call", {
    name: "knocode_context",
    arguments: { prompt: probe, repository_path: repositoryPath },
  });
  if (out.kind !== "ok") return {};
  const text = resultText(out.result);
  if (!text) return {};
  return {
    hookSpecificOutput: {
      hookEventName: "SessionStart",
      additionalContext: `[knocode] repository context:\n${text}`,
    },
  };
}

async function handlePreToolUse(input) {
  const toolName = input?.tool_name;
  if (!toolName || !isContextWorthwhile(toolName)) return {};
  const repositoryPath = input?.cwd || process.cwd();
  const vals = Object.values(input?.tool_input ?? {})
    .filter((v) => typeof v === "string")
    .slice(0, 5)
    .join("\n");
  const prompt = `The agent is about to run tool \`${toolName}\`${vals ? ` targeting:\n${vals.slice(0, 400)}` : ""}. Provide relevant repository context to interpret its result.`;
  const out = await mcpCall("tools/call", {
    name: "knocode_context",
    arguments: { prompt, repository_path: repositoryPath },
  });
  if (out.kind !== "ok") return {};
  const text = resultText(out.result);
  if (!text) return {};
  return {
    hookSpecificOutput: {
      hookEventName: "PreToolUse",
      additionalContext: `[knocode] repository context for ${toolName}:\n${text}`,
    },
  };
}

async function handlePostToolUse(input) {
  const toolName = input?.tool_name;
  const content = input?.tool_response;
  if (!toolName || typeof content !== "string" || content.length < COMPRESS_MIN_CHARS) return {};
  const out = await mcpCall("tools/call", {
    name: "knocode_compress",
    arguments: { content, tool_name: toolName, output_type: outputType(toolName) },
  });
  if (out.kind !== "ok") return {};
  const text = resultText(out.result);
  if (!text || text === content) return {};
  const s = out.result?.structuredContent;
  const tokens =
    s && typeof s.original_tokens === "number" && typeof s.compressed_tokens === "number"
      ? ` (${s.original_tokens} -> ${s.compressed_tokens} tokens)`
      : "";
  return {
    hookSpecificOutput: {
      hookEventName: "PostToolUse",
      additionalContext: `[knocode] compressed the ${toolName} output${tokens}:\n${text}`,
    },
  };
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

async function run(event, input) {
  try {
    switch (event) {
      case "session-start":
      case "sessionstart":
        return await handleSessionStart(input);
      case "pre-tool-use":
      case "pretooluse":
        return await handlePreToolUse(input);
      case "post-tool-use":
      case "posttooluse":
        return await handlePostToolUse(input);
      default:
        log(`unknown event: ${event}`);
        return {};
    }
  } catch (err) {
    log(`hook error: ${err?.message || err}`);
    return {};
  }
}

function main() {
  const event = (process.argv[2] || "").toLowerCase();
  // The hook input may be a single JSON line or pretty-printed across lines; buffer
  // until it parses, then respond once and exit without waiting for stdin to close.
  const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
  let buffer = "";
  rl.on("line", (line) => {
    buffer += line + "\n";
    if (!buffer.trim()) return;
    let input = null;
    try {
      input = JSON.parse(buffer.trim());
    } catch {
      return; // not complete yet — wait for more lines
    }
    rl.close();
    run(event, input ?? {}).then((out) => {
      // Flush stdout, then let the process exit naturally (set exitCode + close stdin).
      // Abrupt `process.exit()` right after a fetch can trigger a libuv fail-fast on
      // Windows (uv async handle closing) — natural teardown avoids it.
      process.stdout.write(JSON.stringify(out) + "\n", () => {
        process.exitCode = 0;
        process.stdin.destroy();
      });
      // Hard safety net so a stubborn stdin never hangs a hook past its budget.
      setTimeout(() => process.exit(0), 3000).unref();
    });
  });
}

main();
function isContextWorthwhile(toolName) {
  const t = (toolName || "").toLowerCase();
  return READ_LIKE.has(t) || SEARCH_LIKE.has(t);
}

/** Extract natural language result text; returns null on error/passthrough/empty. */
function resultText(result) {
  if (!result || result.isError === true) return null;
  const text = result?.content?.[0]?.text;
  if (!text || typeof text !== "string") return null;
  if (result?.structuredContent?.passthrough === true) return null; // zero context hits
  return text;
}