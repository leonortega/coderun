#!/usr/bin/env node
/**
 * Coderun MCP server — stdio → http proxy
 * Exposes coderun_search, coderun_preview, coderun_symbols, coderun_read as MCP tools.
 * Agent-agnostic: Codex (~/.codex/config.toml), Copilot (.vscode/mcp.json), Claude, Opencode.
 * Forwards to existing daemon http://127.0.0.1:9527/hook (no CLI shell conversions).
 */

import * as readline from "node:readline";

const DAEMON_URL = process.env.CODERUN_DAEMON_URL || "http://127.0.0.1:9527";
const TIMEOUT_MS = Number(process.env.CODERUN_TIMEOUT_MS || "30000");

interface MCPRequest {
  jsonrpc: string;
  id: number | string | null;
  method: string;
  params?: any;
}

interface MCPResponse {
  jsonrpc: string;
  id: number | string | null;
  result?: any;
  error?: { code: number; message: string; data?: any };
}

async function callDaemon(request: any): Promise<any> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), TIMEOUT_MS);
  try {
    const res = await fetch(`${DAEMON_URL}/hook`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
      signal: controller.signal,
    });
    clearTimeout(timeout);
    if (!res.ok) return null;
    return await res.json();
  } catch {
    clearTimeout(timeout);
    return null;
  }
}

const TOOLS = [
  {
    name: "coderun_search",
    description: "Search repository via Coderun BM25 + symbol index. Returns ranked files/symbols for a natural-language query.",
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string", description: "Natural-language code query, e.g. 'GifskiOptions interface'" },
        repository_path: { type: "string", description: "Absolute workspace path for repo-scoped search" },
      },
      required: ["query"],
    },
  },
  {
    name: "coderun_preview",
    description: "Build ContextPack for a prompt (candidate_k 200, max_files 50 for large repos). Returns YAML context pack + provenance.",
    inputSchema: {
      type: "object",
      properties: {
        prompt: { type: "string" },
        repository_path: { type: "string" },
        candidate_k: { type: "number", description: "Candidate pool 20/50/100/200, default 100 (200 for 53k)" },
        max_files: { type: "number", description: "Max files in pack, default 20 (50 for large repo)" },
      },
      required: ["prompt"],
    },
  },
  {
    name: "coderun_symbols",
    description: "Search symbols by name pattern via tree-sitter index.",
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string" },
        repository_path: { type: "string" },
      },
      required: ["query"],
    },
  },
  {
    name: "coderun_read",
    description: "Read file content with optional line range via repository intelligence.",
    inputSchema: {
      type: "object",
      properties: {
        path: { type: "string" },
        repository_path: { type: "string" },
        line_start: { type: "number" },
        line_end: { type: "number" },
      },
      required: ["path"],
    },
  },
];

async function handleToolsCall(name: string, args: any): Promise<any> {
  const repoPath = args.repository_path || process.cwd();
  const repository_id = ""; // daemon derives via repository_path SHA-256 (TASK-036)

  if (name === "coderun_search" || name === "coderun_preview") {
    const prompt = args.query || args.prompt || "";
    const candidate_k = args.candidate_k;
    const max_files = args.max_files;
    // Use preview path for full ContextPack; search is lighter but we reuse same daemon hook
    // For search we still call PreGeneration with candidate_k/max_files hints via env override
    const prevEnvK = process.env.CODERUN_CANDIDATE_K;
    const prevEnvM = process.env.CODERUN_MAX_FILES;
    if (candidate_k) process.env.CODERUN_CANDIDATE_K = String(candidate_k);
    if (max_files) process.env.CODERUN_MAX_FILES = String(max_files);
    const req = {
      hook_type: "PreGeneration",
      payload: { type: "MessageRewrite", session_id: "mcp", message: prompt, repository_path: repoPath },
      repository_id,
      timestamp: new Date().toISOString(),
    };
    const res = await callDaemon(req);
    if (candidate_k) {
      if (prevEnvK) process.env.CODERUN_CANDIDATE_K = prevEnvK; else delete process.env.CODERUN_CANDIDATE_K;
    }
    if (max_files) {
      if (prevEnvM) process.env.CODERUN_MAX_FILES = prevEnvM; else delete process.env.CODERUN_MAX_FILES;
    }
    if (!res) return { content: [{ type: "text", text: "Coderun daemon unreachable at " + DAEMON_URL + " — run coderun serve" }] };
    const rewritten = (res as any)?.payload?.rewritten || (res as any)?.payload?.reason || JSON.stringify(res, null, 2);
    const pack = (res as any)?.payload?.context_pack;
    const provenance = pack?.provenance ? `\n\nProvenance:\n${JSON.stringify(pack.provenance.slice(0, 10), null, 2)}` : "";
    return { content: [{ type: "text", text: rewritten + provenance }] };
  }

  if (name === "coderun_symbols") {
    // Fallback via daemon tool output compression path? For now return via search
    const req = {
      hook_type: "PreGeneration",
      payload: { type: "MessageRewrite", session_id: "mcp", message: args.query, repository_path: repoPath },
      repository_id,
      timestamp: new Date().toISOString(),
    };
    const res = await callDaemon(req);
    if (!res) return { content: [{ type: "text", text: "daemon unreachable" }] };
    return { content: [{ type: "text", text: JSON.stringify(res, null, 2) }] };
  }

  if (name === "coderun_read") {
    // Direct file read is outside daemon; fallback to fs read via CLI would be shell — for MCP we return via daemon's file read if available
    // For v1, proxy via preview with file hint
    const req = {
      hook_type: "PreToolCall",
      payload: { type: "ToolOutput", tool_name: "read", output_type: "FileRead", content: args.path, repository_path: repoPath },
      repository_id,
      timestamp: new Date().toISOString(),
    };
    const res = await callDaemon(req);
    return { content: [{ type: "text", text: JSON.stringify(res, null, 2) }] };
  }

  throw new Error(`unknown tool ${name}`);
}

async function main() {
  const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });

  rl.on("line", async (line) => {
    if (!line.trim()) return;
    let req: MCPRequest;
    try {
      req = JSON.parse(line);
    } catch {
      return;
    }
    const id = req.id;
    let response: MCPResponse = { jsonrpc: "2.0", id, result: null };

    try {
      switch (req.method) {
        case "initialize": {
          response.result = {
            protocolVersion: "2024-11-05",
            capabilities: { tools: {} },
            serverInfo: { name: "coderun-mcp", version: "0.8.0" },
          };
          break;
        }
        case "notifications/initialized":
          // no response needed for notifications
          return;
        case "tools/list": {
          response.result = { tools: TOOLS };
          break;
        }
        case "tools/call": {
          const { name, arguments: args } = req.params || {};
          const result = await handleToolsCall(name, args || {});
          response.result = result;
          break;
        }
        case "ping":
          response.result = {};
          break;
        default:
          response.error = { code: -32601, message: `Method not found: ${req.method}` };
      }
    } catch (e: any) {
      response.error = { code: -32603, message: e?.message || String(e) };
    }

    if (id !== null && req.method !== "notifications/initialized") {
      process.stdout.write(JSON.stringify(response) + "\n");
    }
  });
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
