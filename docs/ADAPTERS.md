# Agent Adapters

This document describes how to integrate Coderun with coding agents.

## Supported Agents

| Agent | Tier | Status | Adapter Type | IPC |
|-------|------|--------|--------------|-----|
| OpenCode | 1 | ✅ Supported | Plugin (TypeScript) | UDS+MessagePack primary, HTTP fallback |
| Claude Code | 1 | ✅ Supported | Hooks (Shell scripts) | UDS+MessagePack primary, HTTP fallback |
| Cursor | 1 | ✅ Supported (v0.3.0) | Extension (`adapters/cursor/extension.ts`) | UDS+MessagePack primary, HTTP fallback |
| Gemini CLI | 1 | ✅ Supported (v0.3.0) | Hooks (`adapters/gemini/hooks.sh`) | UDS+MessagePack primary, HTTP fallback |
| Continue | 1 | ⏳ Planned (v0.4.0) | Extension | — |
| Codex / Windsurf / Cline / Kilo / Antigravity / Kimi | 2 | ⚠️ Best-effort (`adapters/tier2/README.md`) | Convention file (no hook guarantee) | HTTP only |

## OpenCode Integration

### Installation

1. Start the Coderun daemon:
   ```bash
   coderun serve
   ```

2. Copy the plugin to your OpenCode plugins directory:
   ```bash
   # Project-level (recommended)
   cp .opencode/plugins/coderun.ts .opencode/plugins/
   
   # Or global
   cp .opencode/plugins/coderun.ts ~/.config/opencode/plugins/
   ```

3. Restart OpenCode

### How It Works

The OpenCode plugin intercepts two events:

| Event | Action |
|-------|--------|
| `message.updated` | Enriches user messages with context from Coderun |
| `tool.execute.before` | Compresses tool outputs to reduce token usage |

### Configuration

Set environment variables to configure the plugin:

```bash
# Override daemon URL (default: http://127.0.0.1:9527)
export CODERUN_DAEMON_URL="http://127.0.0.1:9527"
```

### Plugin Code

See `.opencode/plugins/coderun.ts` for the full implementation.

## Claude Code Integration

### Installation

1. Start the Coderun daemon:
   ```bash
   coderun serve
   ```

2. The hooks are already configured in `.claude/settings.json`

3. Make the hook scripts executable:
   ```bash
   chmod +x .claude/hooks/coderun-pregeneration.sh
   chmod +x .claude/hooks/coderun-pretool.sh
   ```

4. Restart Claude Code

### How It Works

Claude Code hooks run as shell commands at specific lifecycle points:

| Hook | Fires | Action |
|------|-------|--------|
| `UserPromptSubmit` | Before Claude reads prompt | Enriches message with context |
| `PreToolUse` | Before tool execution | Logs compression stats |

### Configuration

The hooks are configured in `.claude/settings.json`:

```json
{
  "hooks": {
    "UserPromptSubmit": [
      {
        "matcher": "",
        "hooks": [
          {
            "type": "command",
            "command": "$CLAUDE_PROJECT_DIR/.claude/hooks/coderun-pregeneration.sh",
            "timeout": 30
          }
        ]
      }
    ],
    "PreToolUse": [
      {
        "matcher": "Read|Write|Edit|Bash|Grep",
        "hooks": [
          {
            "type": "command",
            "command": "$CLAUDE_PROJECT_DIR/.claude/hooks/coderun-pretool.sh",
            "timeout": 10
          }
        ]
      }
    ]
  }
}
```

### Environment Variables

```bash
# Override daemon URL (default: http://127.0.0.1:9527)
export CODERUN_DAEMON_URL="http://127.0.0.1:9527"

# Session ID for tracking (optional)
export SESSION_ID="my-session"
```

### Hook Scripts

See `.claude/hooks/` for the shell script implementations.

## IPC Protocol

Both adapters communicate with the Coderun daemon via HTTP (TCP on Windows, UDS on Unix).

### Request Format

```json
{
  "correlation_id": "req_abc123",
  "hook_type": "PreGeneration" | "PreToolCall",
  "payload": {
    "type": "MessageRewrite" | "ToolOutput",
    "session_id": "optional",
    "message": "user message",
    "tool_name": "optional",
    "output_type": "optional",
    "content": "optional"
  }
}
```

### Response Format

```json
{
  "correlation_id": "req_abc123",
  "hook_type": "PreGeneration",
  "payload": {
    "type": "RewrittenMessage" | "CompressedOutput" | "OriginalPassthrough",
    "original": "original content",
    "rewritten": "enriched content",
    "compressed": "compressed content",
    "reason": "why passthrough",
    "context_pack": {},
    "routing_decision": {},
    "original_tokens": 1000,
    "compressed_tokens": 500
  },
  "latency_ms": 150,
  "error": null
}
```

### Fail-Open Behavior

If the Coderun daemon is unreachable or returns an error, the adapters fail open:
- The original message/output is used unchanged
- The agent continues without interruption
- An error is logged for debugging

## Troubleshooting

### Daemon Not Running

If you see errors about the daemon being unreachable:

```bash
# Check if daemon is running
coderun status

# Start the daemon
coderun serve
```

### Hook Not Firing

For Claude Code:
1. Check `.claude/settings.json` exists and is valid JSON
2. Make hook scripts executable: `chmod +x .claude/hooks/*.sh`
3. Check hook script logs in Claude Code output

For OpenCode:
1. Check plugin is in the correct directory
2. Check OpenCode logs for plugin loading errors
3. Verify the plugin file has correct TypeScript syntax

### Slow Responses

If hooks are slow:
1. Check daemon logs for slow queries
2. Reduce the timeout in settings
3. Check network latency to daemon

## Adding Support for Other Agents

To add support for a new agent:

1. Research the agent's hook/extension API
2. Create adapter files in `adapters/<agent-name>/`
3. Implement the hook handlers that call Coderun daemon
4. Document the installation steps in this file
5. Add integration tests
