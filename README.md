# Coderun — AI Runtime

An AI Runtime that enhances coding agents with contextual intelligence. Coderun runs as a local daemon, intercepting agent requests via UDS/MessagePack (HTTP fallback), enriching them with repository context, knowledge, skills, and model routing decisions. **v1** returns to the agreed local-runtime scope: DBOS/workflows move to `future/workflow` (opt-in via `--features workflow`), event persistence is in-memory only (`tracing`+`metrics`+`correlation_id` canonical), and default languages are `rust/typescript/javascript/python` (`go/java/c/cpp` behind `--features extended-languages`).

## Features

- **Context Engine** — Assembles contextual information from your codebase for better AI responses (`BuildContext` — `skills → docs → code` + `FROZEN PREFIX END` + dedup, requires 30s budget, fail-open)
- **Repository Intelligence** — Incremental indexing: tree-sitter AST (4 default, +4 behind `--features extended-languages`: `go,java,c,cpp`) + ripgrep + tantivy BM25 + `sg-core` ast-grep structural + dependency graph (`graph.rs`)
- **Knowledge Hub** — Unified surface: SQLite+tantivy BM25 + engram (HTTP 2s timeout, fail-open to `memory`). FlashRank removed from v1 per benchmark evaluation.
- **Skill Engine** — Deterministic tag-based skill matching from community formats (Claude/Cursor/Continue/agentskills.io) — canonical `Skill {priority,specificity}` + `max_skills_per_request=5` + conflict detection
- **Model Router** — Heuristic complexity scoring (structural/semantic/scope `0.3/0.4/0.3`) + LiteLLM gateway with `capable→balanced→fast` fallback + `cost_usd` (`[models]` separate from `[routing]`)
- **Execution Optimizer** — RTK adoption (`RtkAdapter` if binary present) + built-in compressors + tee-on-failure `~/.coderun/logs/tool-failures/` + `tiktoken-rs` honest savings reporting
- **Event Bus** — Async-only in-memory observability (`ContextBuilt`…`MemorySaved`) + `tracing`/`metrics`/`correlation_id` — no SQLite persistence in v1
- **Workflows** — Not in v1 (preserved in `future/workflow/` for opt-in only)
- **Metrics** — Prometheus exposition `GET /metrics` (`coderun_build_context_duration_seconds` histogram `0.01-30s`, `coderun_requests_total`, `coderun_fail_open_total`), Grafana `docs/dashboards/coderun.json`
- **Fail-Open Design** — Always returns a response on hot path, never blocks the agent (30s hard timeout → `OriginalPassthrough`)

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
# Run all tests (v1: ~190 tests; workflow in future/workflow --features workflow)
cargo test
cargo test --features extended-languages -p coderun-repo-intel  # +4 langs (go,java,c,cpp)

# Run tests for a specific crate
cargo test -p coderun-core       # secrets HMAC (LazyLock), config v1 defaults
cargo test -p coderun-repo-intel # parser 4 default + extended-languages gate
# future/workflow: cargo test -p coderun-workflow --manifest-path future/workflow/Cargo.toml

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
├── Cargo.toml                    # Workspace root (v1, 10+1 crates; workflow in future/workflow --features workflow)
├── crates/
│   ├── coderun-core/             # Shared types, errors, config (WorkflowConfig enabled:false, HMAC LazyLock) (~32 tests)
│   ├── coderun-daemon/           # Daemon — UDS/MessagePack + HTTP fallback + /metrics (workflow routes behind --features workflow)
│   ├── coderun-cli/              # CLI — init/index/serve/preview/replay/doctor (workflow behind --features workflow, replay legacy)
│   ├── coderun-repo-intel/       # Repository Intelligence — tree-sitter 4+4 extended-languages + ripgrep + tantivy + graph + watcher + lsp (21 tests)
│   ├── coderun-knowledge/        # Knowledge Hub — SQLite+tantivy→rerank→engram deterministic reads (16 tests, collapsed scorer)
│   ├── coderun-skills/           # Skill Engine — MD/TOML/YAML parsing, tag matching from_skills (8 tests)
│   ├── coderun-context/          # Context Engine — BuildContext (tiktoken, frozen-prefix, dedup, reversible) (11 tests)
│   ├── coderun-router/           # Model Router — heuristic + LiteLLM fallback chain + cost (11 tests)
│   ├── coderun-optimizer/        # Execution Optimizer — RTK adapter + compressors (11 tests)
│   ├── coderun-events/           # Event Bus — in-memory broadcast + tracing/correlation (SQLite 004_events legacy, future/workflow only)
│   ├── coderun-storage/          # Local Storage — SQLite WAL + tantivy + audits 005_audits (22 tests, 004/005 legacy for future/workflow)
├── future/workflow/              # durable workflows preserved out of hot path (DBOS Transact sidecar, governed.ts) — opt-in CODERUN_WORKFLOW_ENABLED=true
├── .coderun/
│   ├── config.toml               # Default configuration (v1: [workflow] enabled:false, languages 4-default)
│   └── skills/                   # Skill definitions
├── adapters/
│   ├── cursor/extension.ts       # Cursor Tier 1
│   ├── gemini/hooks.sh           # Gemini CLI Tier 1
│   └── tier2/README.md           # Tier 2 best-effort (README-only since v0.6.0)
├── .opencode/plugins/            # OpenCode plugin (TypeScript, dual-hook chat.message + message.updated shim)
├── .claude/hooks/                # Claude Code hooks (shell scripts)
├── benches/                      # criterion benches (BuildContext p95)
└── docs/                         # Architecture and specification docs (ROADMAP v0.6.0, V0_6_0_PLAN.md)
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

Start the daemon server (UDS primary on `/tmp/coderun.sock` + HTTP fallback on `127.0.0.1:9527`, `GET /metrics`).

```bash
# Start with default config
coderun serve
coderun serve --socket /tmp/coderun.sock --port 9527

# The daemon will (v1: no DBOS/workflow — future/workflow only):
# 1. Load configuration (v1: no [workflow] required)
# 2. Initialize logging + metrics (`/metrics` exposition — token savings, retrieval recall)
# 3. Open database (migrations 001-003, WAL) + tantivy (MmapDirectory)
# 4. Start background indexing + git watcher
# 5. Start UDS+MessagePack primary and HTTP fallback (no sidecar)
# 6. Wait for shutdown signal (Ctrl+C)
```

### `coderun preview <prompt>`

Preview what BuildContext would produce for a prompt (probes daemon via HTTP/UDS, falls back to local).

```bash
coderun preview "implement a new API endpoint"
coderun preview "fix auth" --session my-sess --no-cache
```

Shows:
- Skills that would match (incl. `FROZEN PREFIX END` boundary)
- Knowledge entries (BM25→rerank→engram deterministic read) that would be included
- Code files (ripgrep/tantivy/graph) that would be included
- Token budget `by_source` (`behavioral_skills 20% / docs 15% / code 55%`)
- Model routing decision + fallback chain `capable→balanced→fast`

### `coderun replay` — removed in v1

Event replay and SQLite persistence (`004_events.sql`) removed from hot path (TASK-002). v1 keeps `tracing` + `metrics` + `correlation_id` (in-memory `EventBus`), future/workflow preserves replay.

### `coderun workflow` — future only

Durable workflows are **NOT part of v1** (preserved in `future/workflow/`, opt-in `--features workflow`). v1 `coderun serve`/`doctor` work without DBOS (TASK-001).

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

Health check for all dependencies (8 probes, v0.6.0 `workflow.enabled:true` default).

```bash
coderun doctor
```

Output:
```
Coderun Doctor (v1 — 8 probes, DBOS future only)
═══════════════════════════════════════

SQLite:          ✓ OK (WAL, migrations 001-003)
Config:          ✓ OK (4 langs + extended-languages flag noted)
Skills directory: ✓ OK
Socket path:     ✓ OK (/tmp/coderun.sock)
Tree-sitter:     ✓ OK (4 default + 4 with --features extended-languages)
Tantivy:         ✓ OK (MmapDirectory)
Engram:          ✓ Configured (http://localhost:9090, fail-open local LIKE)
LiteLLM:         ✓ Configured (http://localhost:4000, fallback chain)
RTK:             ⚠ Not found — using built-in compressors
Tiktoken:        ✓ OK (cl100k_base LazyLock)
Secrets redact:  ✓ OK (HMAC via hmac crate)
Workflow/DBOS:   ✓ OK (DBOS at http://localhost:3001) or ⚠ Not reachable
Metrics:         ○ GET /metrics on daemon

✓ All critical checks passed (v0.6.0)
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
| `[daemon]` | Socket path, concurrency, timeout, `metrics_port`, `rate_limit_per_session` |
| `[database]` | SQLite path, connection pool |
| `[index]` | Tantivy path, languages |
| `[knowledge]` | Memory settings (`memory_enabled`, `memory_endpoint`) |
| `[skills]` | Skills directory, auto-discovery |
| `[context]` | Token budget, file limits, `cache_order` |
| `[model]` | Default tier, routing toggle |
| `[routing]` | Weights, thresholds, model mappings |
| `[litellm]` | Endpoint, timeout, retries |
| `[rtk]` | Enabled, max tokens, compression level |
| `[workflow]` | DBOS durable workflows (`enabled`, `engine=dbos|noop`, `dbos_endpoint`, `dbos_shared_secret`, `auto_governance`) |
| `[logging]` | Level, file path, retention |

### Environment Variables

| Variable | Overrides | Default |
|----------|-----------|---------|
| `CODERUN_DAEMON_SOCKET` | daemon.socket_path | /tmp/coderun.sock |
| `CODERUN_DATABASE_PATH` | database.path | ~/.coderun/data.db |
| `CODERUN_LOG_LEVEL` | logging.level | info |
| `CODERUN_MODEL_DEFAULT` | model.default_tier | balanced |
| `CODERUN_CONTEXT_MAX_TOKENS` | context.max_tokens | 12000 |
| `CODERUN_LITELLM_URL` | litellm.endpoint | http://localhost:4000 |
| `CODERUN_ENGRAM_ENDPOINT` | knowledge.memory_endpoint | http://localhost:9090 |
<!-- workflow env vars removed from v1 — see future/workflow/README.md (opt-in CODERUN_WORKFLOW_ENABLED=true) -->

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

### Cursor (v0.4.0 Tier 1)

1. Start the daemon: `coderun serve`
2. Install extension from `adapters/cursor/extension.ts`
3. Extension calls `POST /hook` (UDS/MessagePack primary, HTTP fallback, 30s fail-open)

### Gemini CLI (v0.4.0 Tier 1)

1. Start the daemon: `coderun serve`
2. Hooks at `adapters/gemini/hooks.sh` (`UserPromptSubmit` + `PreToolUse` analogs)
3. `chmod +x adapters/gemini/hooks.sh`

## Architecture

### Component Overview

```
┌─────────────────────────────────────────────────────────────┐
│                      Coding Agent                           │
│  (opencode, Claude Code, Cursor, Gemini CLI, etc.)          │
└─────────────────────────┬───────────────────────────────────┘
                           │ UDS/MessagePack primary, HTTP fallback
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                    Adapter Layer                            │
│  • Request validation + rate-limit (token-bucket per session)│
│  • Correlation ID + HMAC verification (DBOS)                │
│  • Fail-open (30s → OriginalPassthrough)                    │
│  • Prometheus /metrics                                      │
└─────────────────────────┬───────────────────────────────────┘
                           │
           ┌───────────────┼───────────────┐
           ▼               ▼               ▼
┌─────────────┐  ┌─────────────┐  ┌─────────────┐
│   Context   │  │  Execution  │  │    Event    │
│   Engine    │  │  Optimizer  │  │     Bus     │
│ (RwLock)    │  │ (RTK)       │  │ (→ SQLite)  │
└──────┬──────┘  └─────────────┘  └─────────────┘
        │
        ├──► Repository Intelligence (tree-sitter/ast-grep/ripgrep/tantivy/graph/watcher/lsp)
        ├──► Knowledge Hub (tantivy→engram deterministic reads)
        ├──► Skill Engine (tag-based, full-instruction injection)
        └──► Model Router (heuristic + LiteLLM fallback chain)
                           │
                    ┌──────┴──────┐
                    │ DBOS Transact│ (optional sidecar :3001, audits 005)
                    │  workflow/   │
                    └─────────────┘
```

### Request Flow

1. Agent sends request via UDS/MessagePack (HTTP/JSON fallback on Windows)
2. Adapter Layer validates, rate-limits (10/s burst 20 per `session_id`), generates correlation ID
3. Context Engine assembles context pack (`RwLock` read, concurrent sessions):
   - Searches code via Repository Intelligence (ripgrep/tantivy/graph, `tiktoken-rs` budgets)
   - Retrieves knowledge via Knowledge Hub (BM25 top20 → rerank adaptive K `5-20` → engram 2s fail-open)
   - Matches skills via Skill Engine (deterministic tag scoring, dedup via `session_fingerprints`)
   - Orders: `behavioral_skills` (20%) → `docs_context` (15%) → `code_context` (55%) + `FROZEN PREFIX END` boundary
   - Reversible truncation (`~/.coderun/cache/originals/{hash}`) + `get_original()`
4. Model Router selects tier (`capable→balanced→fast` fallback via LiteLLM, `cost_usd` tracked)
5. Response returned (or `OriginalPassthrough` on error/timeout), `Timer` records `coderun_build_context_duration_seconds`, audit row inserted off hot path
6. If `workflow.enabled` + `auto_governance` and tier in `require_approval_tiers` → `DBOSWorkflowEngine::start_workflow()` POSTs to sidecar `:3001` (HMAC), sidecar `DBOS.workflow` persists to `~/.coderun/dbos.db` WAL+Litestream

## IPC Protocol

### Primary: UDS + MessagePack (Unix), `named_pipe`/TCP fallback on Windows

`rmp-serde` encode of `AgentRequest` (`crates/coderun-core/src/ipc.rs`) → `4-byte BE len` → body. HTTP/JSON `POST /hook` remains as fallback (and `POST /metrics`, `POST /workflow/*`). See `daemon/src/adapter.rs:204` (length-prefix + MessagePack) and `http_server.rs:93` (`create_router`).

### Request Format (JSON fallback shown; MessagePack is canonical)

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

### Response Format (JSON fallback)

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

### Metrics

```
GET /metrics  →  Prometheus exposition (crate daemon/src/metrics.rs)
coderun_requests_total{key="PreGeneration_balanced"} 12
coderun_build_context_duration_seconds_bucket{le="0.05"} 42
coderun_fail_open_total 3
coderun_index_files 142
```

### DBOS Workflow (HTTP)

```
POST /workflow/start     {task, session_id, require_approval} → {workflow_id, status}
GET  /workflow/:id       → {workflow_id, status, task}
POST /workflow/:id/approve → {workflow_id, status: "completed"}
GET  /health             → {status:"ok", engine:"dbos-mock"|"dbos"}
```

### Fail-Open Behavior

On any error or timeout, the daemon returns `OriginalPassthrough` with the original message unchanged. The agent always gets a response.

| Condition | Response | Reason |
|-----------|----------|--------|
| Timeout (>30s) | OriginalPassthrough | "timeout" |
| Context build error | OriginalPassthrough | "error" |
| Any internal error | OriginalPassthrough | "fail-open" |

## Implementation Status (v0.6.0)

| Component | Status | Implementation |
|-----------|--------|----------------|
| Config System | ✅ Complete | TOML + env (`CODERUN_*` v1), `[workflow]` future only (opt-in `CODERUN_WORKFLOW_ENABLED=true`), `index.languages` 4 default + `extended-languages` warn, validation |
| Core Types | ✅ Complete | IPC (MessagePack), `IWorkflowEngine` **async** `async_trait`, HMAC `hmac` crate `LazyLock<Regex>` |
| Event Bus | ✅ Complete | `broadcast` + ring→SQLite `004_events.sql` for `replay` |
| Storage | ✅ Complete | SQLite WAL + tantivy BM25 + `005_audits.sql` (`audits`+`workflows`) + `cost_usd` |
| Repository Intelligence | ✅ Complete | tree-sitter **4 +4 extended-languages** + ripgrep + tantivy full-text + `graph.rs` edges + `watcher.rs` `notify+git2` + stub `lsp.rs` |
| Skill Engine | ✅ Complete | MD/TOML/YAML, tag matching `from_skills` canonical, conflict detection, full-instruction injection |
| Knowledge Hub | ✅ Complete | BM25 top20→rerank adaptive `5-20` (TF-IDF, `ort` int8) + engram 2s fail-open + collapsed scorer delegates to `SkillEngine` |
| Model Router | ✅ Complete | Heuristic scoring + LiteLLM gateway `capable→balanced→fast` fallback + fallback tests |
| Execution Optimizer | ✅ Complete | `RtkAdapter` (binary if present, `~10ms`) + tee-on-failure → `~/.coderun/logs/tool-failures/` + `tiktoken-rs` `LazyLock` |
| Context Engine | ✅ Complete | `BuildContext` + `RwLock` concurrency, cache-order `skills→docs→code` + `FROZEN PREFIX END` + dedup + reversible `get_original()` + `tiktoken-rs` budgets |
| Adapter Layer | ✅ Complete | UDS/MessagePack primary + HTTP fallback, 30s fail-open `OriginalPassthrough`, `/metrics`, `/workflow/*`, rate-limit per `session_id`, HMAC via `core::secrets` |
| Daemon Lifecycle | ✅ Complete | Startup + graceful shutdown + DBOS sidecar spawn when `workflow.enabled` (required) |
| CLI Commands | ✅ Complete | 12 subcommands (`init --wizard`, `index --watch`, `preview`/`replay`, `workflow start/status/approve/list` async `rt.block_on`, `doctor` 8 probes) |
| Agent Adapters | ✅ Complete | OpenCode **dual-hook** `chat.message` + `message.updated` shim (v0.6.0), Cursor, Gemini CLI Tier 1; Tier 2 README-only |
| DBOS Workflows | ✅ Complete | `coderun-workflow` native async `DBOSWorkflowEngine` + `dbos-transact` `governedWorkflow` SQLite+Litestream, HMAC `hmac` crate, fail-closed |
| Metrics | ✅ Complete | `daemon/src/metrics.rs` histogram p95 + `ratelimit.rs` delegate + Grafana `docs/dashboards/coderun.json` |
| Evaluation | ✅ Complete | Promptfoo framework + UDS custom provider |
| Distribution | ✅ Complete | `Dockerfile` (distroless), `Formula/coderun.rb` (brew tap), `cargo-wix` MSI scaffold |

### v0.6.0 External Tool Integration

| Tool | Integration | Status |
|------|-------------|--------|
| tree-sitter | AST parsing for Rust, Python, JS, TS (4 default) + `go,java,c,cpp` via `--features extended-languages` | ✅ Integrated `repo-intel/src/parser.rs` `LazyLock` gating |
| ripgrep | Fast text search (`grep-searcher`+`ignore`) | ✅ Integrated `repo-intel/src/lib.rs` |
| tantivy | BM25 in-process MmapDirectory + `tantivy_index.rs` wiring | ✅ Integrated (repo-intel `search_fulltext` + storage `005`) |
| ast-grep | Structural search `search_structural()` `sg-core` gated | ✅ Integrated `sg-core` first-class (`search_structural_fallback` only on Err) |
| engram | Cross-session memory HTTP `2s` timeout, fail-open local `LIKE` | ✅ Integrated `knowledge/src/engram.rs`+`try_engram_search` |
| FlashRank (`ort`) | Removed from v1 runtime per benchmark evaluation — see `docs/01-architecture/FLASHRANK_REMOVAL.md` | ❌ Removed (offline eval only) |
| LiteLLM | Gateway `select_model` + `fallback_chain()` `capable→balanced→fast` + `cost_usd` | ✅ Integrated `router/src/litellm.rs` |
| RTK | Tool-output compression `RtkAdapter::detect()` + tee-on-failure `~/.coderun/logs/tool-failures/` | ✅ Integrated `optimizer/src/rtk.rs` (binary optional, built-ins on Err) |
| tiktoken-rs | `cl100k_base` local token counting, `heuristic` fallback | ✅ Integrated `context/src/lib.rs` `optimizer/src/lib.rs` |
| DBOS Transact | Durable workflows **required** SQLite+Litestream native async `async_trait` `governedWorkflow` | ✅ Integrated `crates/coderun-workflow` + `workflow/dbos/` `dbos-transact` |
| hmac | `HMAC-SHA256` via `hmac` crate (single `secrets::verify_hmac`) | ✅ Integrated `coderun-core/src/secrets.rs` `LazyLock` |
| MkDocs | Doc site `mkdocs.yml` + `category="docs"` ingestion (best-effort) | ✅ Integrated (build → `gh-pages`) |
| Prometheus/Grafana | `/metrics` exposition + `docs/dashboards/coderun.json` + `deploy/prometheus/alerts.yml` | ✅ Integrated `daemon/src/metrics.rs` |

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

See [docs/ROADMAP.md](docs/ROADMAP.md) for v0.6.0 (DBOS required + spec compliance) and beyond. Full plans: `docs/V0_3_0_PLAN.md`, `docs/V0_4_0_PLAN.md`, `docs/V0_5_0_PLAN.md`, `docs/V0_6_0_PLAN.md`, workflows `docs/02-workflows/DBOS.md`.

## Distribution

```bash
# Homebrew
brew tap leonortega/coderun
brew install coderun
brew services start coderun  # launchd

# Docker
docker build -t coderun:0.4.0 .
docker run -p 9527:9527 -v $PWD:/repo coderun serve
docker compose --file deploy/docker-compose.yml up  # with DBOS sidecar :3001
```

## License

MIT
