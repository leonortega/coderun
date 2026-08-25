# opencode-coderun

Coderun AI Runtime plugin for [OpenCode](https://opencode.ai).

Enriches prompts with repository context and compresses tool outputs via the Coderun daemon (`coderun serve` on `http://127.0.0.1:9527`).

## Install

```bash
npm install opencode-coderun
# or publish and add to opencode config:
```

`opencode.json` / `opencode.jsonc`:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "plugin": ["opencode-coderun"]
}
```

OpenCode installs npm plugins automatically via `bun` to `~/.cache/opencode/node_modules/` on startup. Alternatively keep a local copy in `.opencode/plugins/`.

## Configuration

| Env var | Default | Description |
|---------|---------|-------------|
| `CODERUN_DAEMON_URL` | `http://127.0.0.1:9527` | Daemon base URL |
| `CODERUN_TIMEOUT_MS` | `30000` | HTTP timeout |

## Scripts

| Script | Description |
|--------|-------------|
| `npm run build` | Compile TypeScript to `dist/` (compiler) |
| `npm run typecheck` | Type-check without emit |
| `npm run test` | Run unit tests (vitest) |
| `npm run test:watch` | Watch mode |
| `npm run dev` | Watch compiler |

## Development

```bash
npm install
npm run build
npm test
```

## How it works

- `chat.message` + `message.updated` (compat shim) -> `POST /hook` `{hook_type:"PreGeneration",payload:{type:"MessageRewrite",message}}` -> rewrites `input.message.content`
- `tool.execute.before` -> `POST /hook` `{hook_type:"PreToolCall",payload:{type:"ToolOutput"}}` -> replaces `output.result` with compressed version

Fail-open: daemon unreachable or non-2xx returns no-op.

## License

MIT
