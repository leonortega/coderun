# Tier 2 Adapters — Best-Effort (Spec §3 Adapter Layer)

Tier 2 agents (Codex, Windsurf, Cline, Kilo Code, Antigravity, Kimi) only expose **convention-based integration**
(a rules file the agent may or may not follow). Coderun supports them only as **clearly-labeled best-effort**, never
with the same guarantee as Tier 1 (opencode, Claude Code, Cursor, Gemini CLI, Copilot, OpenClaw, Pi, Factory Droid).

> **Disclaimer:** Tier 2 adapters cannot use native programmatic hooks (`chat.message` / `UserPromptSubmit` / `PreToolUse`).
> They rely on a rules file (e.g., `AGENT.md`, `.cursorrules`, `.windsurfrules`) that instructs the agent to call
> the Coderun daemon before generation. The agent may skip the call; fail-open still holds but there is no interception guarantee.

## Supported Tier 2 (best-effort)

| Agent | Convention file | Coderun instruction |
|-------|----------------|---------------------|
| Codex | `AGENT.md` or `.codex/instructions.md` | Add: `Before answering, call http://127.0.0.1:9527/hook with your prompt for context` |
| Windsurf | `.windsurfrules` | Same |
| Cline / Kilo | `.clinerules` | Same |
| Antigravity | `ANTIGRAVITY.md` | Same |
| Kimi | `AGENT.md` | Same |

## Template

Copy `adapters/tier2/AGENT.md.template` into the repository root and customize the daemon URL:

```markdown
# Coderun Context
Before generating a response, POST your prompt to `http://127.0.0.1:9527/hook` with
`{ "hook_type": "PreGeneration", "payload": { "type": "MessageRewrite", "session_id": "tier2", "message": "<your prompt>" } }`
and use the returned `rewritten` field (which contains the Context Pack YAML ordered skills→docs→code) as additional context.
If the daemon is unreachable or times out (>30s), proceed with the original prompt (fail-open).
```

## Limitations

- No `tool.execute.before` compression guarantee — agent may not call `PreToolCall`.
- No latency SLA — convention file is advisory.
- No event bus integration — observability limited to promptfoo eval.
