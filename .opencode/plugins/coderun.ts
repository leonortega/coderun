/**
 * Coderun AI Runtime Plugin for OpenCode
 * 
 * This plugin integrates OpenCode with the Coderun daemon via UDS/TCP.
 * It intercepts pre-generation and pre-tool-call events to enrich
 * context and compress tool outputs.
 * 
 * Usage:
 *   1. Start the Coderun daemon: `coderun serve`
 *   2. Place this file in `.opencode/plugins/`
 *   3. Restart OpenCode
 * 
 * Configuration:
 *   Set CODERUN_DAEMON_URL environment variable to override the default.
 *   Default: http://127.0.0.1:9527 (TCP) or /tmp/coderun.sock (UDS)
 */

import type { Plugin } from "@opencode-ai/plugin"

const CODERUN_DAEMON_URL = process.env.CODERUN_DAEMON_URL || "http://127.0.0.1:9527"
const REQUEST_TIMEOUT_MS = 30000

interface CoderunRequest {
  correlation_id: string
  hook_type: "PreGeneration" | "PreToolCall"
  payload: {
    type: "MessageRewrite" | "ToolOutput"
    session_id?: string
    message?: string
    tool_name?: string
    output_type?: string
    content?: string
    context?: string
  }
}

interface CoderunResponse {
  correlation_id: string
  hook_type: string
  payload: {
    type: string
    original?: string
    rewritten?: string
    compressed?: string
    reason?: string
    context_pack?: unknown
    routing_decision?: unknown
    original_tokens?: number
    compressed_tokens?: number
  }
  latency_ms: number
  error?: string
}

function generateCorrelationId(): string {
  return `req_${crypto.randomUUID()}`
}

async function callCoderunDaemon(request: CoderunRequest): Promise<CoderunResponse | null> {
  try {
    const controller = new AbortController()
    const timeout = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS)

    const response = await fetch(`${CODERUN_DAEMON_URL}/hook`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
      signal: controller.signal,
    })

    clearTimeout(timeout)

    if (!response.ok) {
      console.error(`[coderun] Daemon returned ${response.status}`)
      return null
    }

    return await response.json() as CoderunResponse
  } catch (error) {
    // Fail-open: if daemon is unreachable, continue without enrichment
    console.error(`[coderun] Daemon unreachable: ${error}`)
    return null
  }
}

export const CoderunPlugin: Plugin = async ({ project, client, $, directory, worktree }) => {
  console.log("[coderun] Plugin initialized")

  return {
    // Pre-generation hook: enrich messages with context
    "message.updated": async (input, output) => {
      // Only intercept user messages
      if (input.message?.role !== "user") return

      const message = input.message?.content || ""
      if (!message || typeof message !== "string") return

      const request: CoderunRequest = {
        correlation_id: generateCorrelationId(),
        hook_type: "PreGeneration",
        payload: {
          type: "MessageRewrite",
          session_id: input.session_id || "unknown",
          message: message,
        },
      }

      const response = await callCoderunDaemon(request)
      if (response?.payload.type === "RewrittenMessage" && response.payload.rewritten) {
        // Replace the message content with the enriched version
        if (input.message) {
          input.message.content = response.payload.rewritten
        }
        console.log(`[coderun] Enriched message (${response.latency_ms}ms)`)
      }
    },

    // Pre-tool hook: compress tool outputs
    "tool.execute.before": async (input, output) => {
      const toolName = input.tool
      if (!toolName) return

      // Get the tool output content if available
      const content = output?.result
      if (!content || typeof content !== "string") return

      const request: CoderunRequest = {
        correlation_id: generateCorrelationId(),
        hook_type: "PreToolCall",
        payload: {
          type: "ToolOutput",
          tool_name: toolName,
          output_type: getOutputType(toolName),
          content: content,
        },
      }

      const response = await callCoderunDaemon(request)
      if (response?.payload.type === "CompressedOutput" && response.payload.compressed) {
        // Replace with compressed content
        if (output) {
          output.result = response.payload.compressed
        }
        const savings = response.payload.original_tokens && response.payload.compressed_tokens
          ? `${Math.round((1 - response.payload.compressed_tokens / response.payload.original_tokens) * 100)}%`
          : "unknown"
        console.log(`[coderun] Compressed ${toolName} output (${savings} savings)`)
      }
    },
  }
}

function getOutputType(toolName: string): string {
  switch (toolName.toLowerCase()) {
    case "read":
    case "readfile":
      return "FileRead"
    case "grep":
    case "search":
      return "SearchResult"
    case "bash":
    case "shell":
    case "exec":
      return "ShellOutput"
    default:
      return "Other"
  }
}
