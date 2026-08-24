# Coderun — AI Runtime

An AI Runtime that enhances coding agents with contextual intelligence. Coderun runs as a local daemon, intercepting agent requests via HTTP, enriching them with repository context, knowledge, skills, and model routing decisions.

## Features

- **Context Engine** — Assembles contextual information from your codebase for better AI responses
- **Repository Intelligence** — Incremental indexing with symbol extraction and text search (regex-based)
- **Knowledge Hub** — Stores project conventions, patterns, and domain knowledge (SQLite)
- **Skill Engine** — Tag-based skill matching from community-format files
- **Model Router** — Heuristic complexity scoring for tier-based model selection
- **Execution Optimizer** — Tool-output compression to reduce token consumption
- **Event Bus** — Async observability for debugging and metrics
- **Fail-Open Design** — Always returns a response, never blocks the agent

## Quick Start

```bash
# 1. Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Build coderun
cargo build --release

# 3. Initialize your project
./target/release/coderun init

# 4. Index your repository
./target/release/coderun index

# 5. Start the daemon
./target/release/coderun serve
```

## Prerequisites

- **Rust 1.75+** — Install via [rustup](https://rustup.rs/)
- **SQLite** — Bundled via `rusqlite` (no system dependency needed)
- **Node.js** — For OpenCode plugin (optional)

## Build

```bash
# Build all crates
cargo build

# Build in release mode (recommended)
cargo build --release

# Build specific crate
cargo build -p coderun-core
```

## Test

```bash
# Run all tests (108 tests)
cargo test

# Run tests for a specific crate
cargo test -p coderun-core

# Run with output
cargo test -- --nocapture
```

## Lint

```bash
# Run clippy for additional lint checks
cargo clippy

# Check for security vulnerabilities
cargo audit
```

## Project Structure

```
coderun/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── coderun-core/             # Shared types, errors, config (22 tests)
│   ├── coderun-daemon/           # Daemon binary — HTTP server (10 tests)
│   ├── coderun-cli/              # CLI binary — all subcommands (3 tests)
│   ├── coderun-repo-intel/       # Repository Intelligence — indexing, search (10 tests)
│   ├── coderun-knowledge/        # Knowledge Hub — storage, retrieval (9 tests)
│   ├── coderun-skills/           # Skill Engine — parsing, matching (8 tests)
│   ├── coderun-context/          # Context Engine — pipeline assembly (7 tests)
│   ├── coderun-router/           # Model Router — complexity scoring (8 tests)
│   ├── coderun-optimizer/        # Execution Optimizer — compression (11 tests)
│   ├── coderun-events/           # Event Bus — observability (6 tests)
│   └── coderun-storage/          # Local Storage — SQLite (12 tests)
├── .coderun/
│   ├── config.toml               # Default configuration
│   └── skills/                   # Skill definitions
├── .opencode/plugins/            # OpenCode plugin (TypeScript)
├── .claude/hooks/                # Claude Code hooks (shell scripts)
└── docs/                         # Architecture and specification docs
```

## CLI Commands

### `coderun init`

Initialize coderun for the current repository.

```bash
coderun init
```

Creates:
- `.coderun/` directory
- `.coderun/config.toml` with default configuration
- `.coderun/skills/` directory for skill definitions
- SQLite database at `~/.coderun/data.db`

### `coderun index`

Index the repository for search and context building.

```bash
coderun index
```

Output:
```
✓ Indexing complete!

  Files indexed:    142
  Symbols extracted: 89
  Files skipped:    23
  Duration:         1234ms
```

### `coderun serve`

Start the daemon server (HTTP on port 9527).

```bash
# Start with default config
coderun serve

# The daemon will:
# 1. Load configuration
# 2. Initialize logging
# 3. Open database
# 4. Start background indexing
# 5. Start HTTP server on port 9527
# 6. Wait for shutdown signal (Ctrl+C)
```

### `coderun preview <prompt>`

Preview what BuildContext would produce for a prompt.

```bash
coderun preview "implement a new API endpoint"
```

Shows:
- Skills that would match
- Knowledge entries that would be included
- Code files that would be included
- Model routing decision

### `coderun status`

Show daemon status and metrics.

```bash
coderun status
```

Output:
```
Coderun Status
═══════════════════════════════════════

Database:
  Path:          /home/user/.coderun/data.db
  Files indexed: 142
  Symbols:       89

Token Usage:
  Total input tokens:  12500
  Total output tokens: 3200
  Total requests:      15

Skills: 3 files
```

### `coderun skills list`

List all loaded skills.

```bash
coderun skills list
```

Output:
```
Loaded skills (3):

  Rust Expert (tags: rust, cargo, async, ownership)
  Python Expert (tags: python, django, fastapi)
  API Design (tags: api, rest, graphql, endpoints)
```

### `coderun skills validate`

Validate all skill definitions.

```bash
coderun skills validate
```

Output:
```
✓ All 3 skill files are valid
```

### `coderun config show`

Display the effective configuration.

```bash
coderun config show
```

### `coderun config validate`

Validate the configuration file.

```bash
coderun config validate
```

### `coderun doctor`

Health check for all dependencies.

```bash
coderun doctor
```

Output:
```
Coderun Doctor
═══════════════════════════════════════

SQLite:          ✓ OK
Config:          ✓ OK
Skills directory: ✓ OK (.coderun/skills)

✓ All critical checks passed
```

## Configuration

Configuration is loaded in order of priority (highest wins):

1. **Environment variables**: `CODERUN_*`
2. **Project config**: `.coderun/config.toml`
3. **User config**: `~/.config/coderun/config.toml`
4. **Defaults**: Built-in defaults

### Configuration Sections

| Section | Purpose |
|---------|---------|
| `[daemon]` | Socket path, concurrency, timeout |
| `[database]` | SQLite path, connection pool |
| `[knowledge]` | Memory settings |
| `[skills]` | Skills directory, auto-discovery |
| `[context]` | Token budget, file limits |
| `[model]` | Default tier, routing toggle |
| `[routing]` | Weights, thresholds, model mappings |
| `[logging]` | Level, file path, retention |

### Environment Variables

| Variable | Overrides | Default |
|----------|-----------|---------|
| `CODERUN_DAEMON_SOCKET` | daemon.socket_path | /tmp/coderun.sock |
| `CODERUN_DATABASE_PATH` | database.path | ~/.coderun/data.db |
| `CODERUN_LOG_LEVEL` | logging.level | info |
| `CODERUN_MODEL_DEFAULT` | model.default_tier | balanced |
| `CODERUN_CONTEXT_MAX_TOKENS` | context.max_tokens | 12000 |

## Skills

Skills are community-format files that provide instructions for specific tasks.

### Skill Format (Markdown)

```markdown
# Add Rate Limiting

## Tags
rate limit, throttle, request limit, API limit, middleware, security

## Instructions
1. Check existing middleware patterns in the project
2. Create a rate limiter module following project conventions
3. Apply to target routes
4. Add configuration for rate limits
5. Add tests for rate limiting behavior

## Examples
- Add global rate limiting: src/middleware/rate_limit.rs, src/config.rs

## Constraints
- Do not modify authentication logic
- Use the project's existing error handling pattern
- Make rate limits configurable via environment variables
```

### Supported Formats

- **Markdown** (`.md`) — Primary format
- **TOML** (`.toml`) — Alternative
- **YAML** (`.yaml`, `.yml`) — Alternative

### Creating Skills

1. Create a file in `.coderun/skills/`
2. Add name, tags, instructions, examples, and constraints
3. Run `coderun skills validate` to check syntax
4. Run `coderun skills list` to verify loading

## Agent Integration

### OpenCode

1. Start the daemon: `coderun serve`
2. Copy the plugin: `cp .opencode/plugins/coderun.ts .opencode/plugins/`
3. Restart OpenCode

### Claude Code

1. Start the daemon: `coderun serve`
2. Hooks are configured in `.claude/settings.json`
3. Make hooks executable: `chmod +x .claude/hooks/*.sh`
4. Restart Claude Code

## Architecture

### Component Overview

```
┌─────────────────────────────────────────────────────────────┐
│                      Coding Agent                           │
│  (opencode, Claude Code, etc.)                              │
└─────────────────────────┬───────────────────────────────────┘
                          │ HTTP (JSON)
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                    Adapter Layer                            │
│  • Request validation                                       │
│  • Correlation ID generation                                │
│  • Fail-open behavior                                       │
└─────────────────────────┬───────────────────────────────────┘
                          │
          ┌───────────────┼───────────────┐
          ▼               ▼               ▼
┌─────────────┐  ┌─────────────┐  ┌─────────────┐
│   Context   │  │  Execution  │  │    Event    │
│   Engine    │  │  Optimizer  │  │     Bus     │
└──────┬──────┘  └─────────────┘  └─────────────┘
       │
       ├──► Repository Intelligence (indexing, search)
       ├──► Knowledge Hub (conventions, patterns)
       ├──► Skill Engine (tag-based matching)
       └──► Model Router (complexity scoring)
```

### Request Flow

1. Agent sends request via HTTP
2. Adapter Layer validates and generates correlation ID
3. Context Engine assembles context pack:
   - Searches code via Repository Intelligence
   - Retrieves knowledge via Knowledge Hub
   - Matches skills via Skill Engine
   - Orders: skills → docs → code (cache-stable)
   - Enforces token budget
4. Model Router selects appropriate model tier
5. Response returned to agent (or OriginalPassthrough on error)

## IPC Protocol

### Request Format (JSON)

```json
{
  "hook_type": "PreGeneration",
  "payload": {
    "type": "MessageRewrite",
    "session_id": "test",
    "message": "fix a typo in README"
  }
}
```

### Response Format (JSON)

```json
{
  "correlation_id": "req_abc123",
  "hook_type": "PreGeneration",
  "payload": {
    "type": "RewrittenMessage",
    "original": "fix a typo in README",
    "rewritten": "fix a typo in README\n\n---\n\nContext:\n..."
  },
  "latency_ms": 100,
  "error": null
}
```

### Fail-Open Behavior

On any error or timeout, the daemon returns `OriginalPassthrough` with the original message unchanged. The agent always gets a response.

| Condition | Response | Reason |
|-----------|----------|--------|
| Timeout (>30s) | OriginalPassthrough | "timeout" |
| Context build error | OriginalPassthrough | "error" |
| Any internal error | OriginalPassthrough | "fail-open" |

## Implementation Status (v0.2.0)

| Component | Status | Implementation |
|-----------|--------|----------------|
| Config System | ✅ Complete | TOML loading, env overrides, validation |
| Core Types | ✅ Complete | Error enums, IPC types, serde |
| Event Bus | ✅ Complete | broadcast channel, buffer, correlation |
| Storage | ✅ Complete | SQLite + WAL + tantivy BM25 |
| Repository Intelligence | ✅ Complete | tree-sitter AST + ripgrep search |
| Skill Engine | ✅ Complete | MD/TOML/YAML parsing, tag matching |
| Knowledge Hub | ✅ Complete | SQLite + engram + FlashRank reranking |
| Model Router | ✅ Complete | Heuristic scoring + LiteLLM routing |
| Execution Optimizer | ✅ Complete | File/search/shell compressors |
| Context Engine | ✅ Complete | Pipeline, cache ordering, token budget |
| Adapter Layer | ✅ Complete | HTTP server, JSON, fail-open |
| Daemon Lifecycle | ✅ Complete | Startup, shutdown, signal handling |
| CLI Commands | ✅ Complete | All 10 subcommands |
| Agent Adapters | ✅ Complete | OpenCode + Claude Code |
| Evaluation | ✅ Complete | Promptfoo framework |

### v0.2.0 External Tool Integration

| Tool | Integration | Status |
|------|-------------|--------|
| tree-sitter | AST parsing for Rust, Python, JS, TS | ✅ Integrated |
| ripgrep | Fast text search with .gitignore support | ✅ Integrated |
| tantivy | BM25 full-text indexing and search | ✅ Integrated |
| engram | Cross-session memory via HTTP | ✅ Integrated |
| FlashRank | Reranking with TF-IDF fallback | ✅ Integrated |
| LiteLLM | Multi-provider model routing | ✅ Integrated |
| MkDocs | Documentation site | ⏳ Planned for v0.3.0 |

## Development

### Adding a New Crate

1. Create `crates/coderun-<name>/Cargo.toml`
2. Add to workspace `Cargo.toml` members
3. Add shared dependencies to `[workspace.dependencies]`
4. Create `src/lib.rs` with module code
5. Add tests in `#[cfg(test)] mod tests`

### Running Specific Tests

```bash
# Test single crate
cargo test -p coderun-core

# Test specific function
cargo test test_config_load

# Test with output
cargo test -- --nocapture
```

### Code Quality

```bash
# Check for warnings
cargo build 2>&1 | grep warning

# Run clippy
cargo clippy

# Format code
cargo fmt

# Check formatting
cargo fmt -- --check
```

## Roadmap

See [docs/ROADMAP.md](docs/ROADMAP.md) for v0.2.0 and beyond.

## License

MIT
