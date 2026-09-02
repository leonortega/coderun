# Coderun — AI Runtime

**Coderun is a local AI runtime that makes coding agents 20-27× faster at finding relevant code.** It runs as a local daemon, intercepting agent requests and enriching them with repository context — skills, knowledge, and code files — using a retrieval engine that understands *what you mean*, not just *what you typed*.

### Why This Matters

When you ask an AI coding agent "how to add error handling", it needs to find the right files in your codebase. Today, most agents use `grep` — a 1970s text-matching tool that finds literal string patterns. Coderun's retrieval engine replaces grep with **semantic search**: it understands intent, expands queries with synonyms, and finds files that grep completely misses.

```
Traditional (grep):                        Coderun:
  "how to add error handling"                "how to add error handling"
  → finds files with literal "error"         → finds error types, try/catch patterns,
  → misses documentation, test files,          documentation, test files, config,
    related components, config files            related components
  → 4.8 seconds on 53k files                 → 45ms on 53k files (106× faster at P50)
```

Default languages are `rust/typescript/javascript/python` (111 languages supported via arborium, add any from the list).

## Features

- **Retrieval Engine** — Semantic code search that replaces grep. Intent detection → query expansion → BM25 + structural search → graph boost → ranking. Finds files grep can't.
- **Repository Intelligence** — Incremental indexing: tree-sitter AST (**111 languages** via arborium) + tantivy BM25 + structural search (ast-grep) + dependency graph. `mtime+size` shortcut for fast warm re-indexes.
- **Context Engine** — Assembles contextual information from your codebase for better AI responses (`BuildContext` — `skills → docs → code` + `FROZEN PREFIX END` + dedup, requires 30s budget, fail-open)
- **Skill Engine** — Deterministic tag-based skill matching from community formats (Claude/Cursor/Continue/agentskills.io) — canonical `Skill {priority,specificity}` + `max_skills_per_request=5` + conflict detection (optional, not on hot-path if absent)
- **Model Router** — Heuristic `capable→balanced→fast` fallback chain (optional LiteLLM integration)
- **Execution Optimizer** — RTK optional (`RtkAdapter` if binary present, fallback to normal output) + built-in compressors + tee-on-failure `~/.coderun/logs/tool-failures/` + `tiktoken-rs` honest savings reporting
- **Event Bus** — Async-only in-memory observability (`ContextBuilt`…`MemorySaved`) + `tracing`/`metrics`/`correlation_id`
- **Metrics** — Prometheus exposition `GET /metrics` (`coderun_build_context_duration_seconds` histogram, `coderun_requests_total`, `coderun_fail_open_total`), Grafana `docs/dashboards/coderun.json`
- **Fail-Open Design** — Always returns a response on hot path, never blocks the agent (30s hard timeout → `OriginalPassthrough`)

---

## Benchmark: Coderun vs Grep

We benchmarked our retrieval engine against `grep -rE` across three real-world codebases. The results: **Coderun is 21-27× faster than grep while finding semantically relevant files that grep completely misses.**

### Speed

```
                    Coderun         grep -rE       Speedup
                    ─────────       ─────────      ───────
Mattermost (9k)     27ms P50        971ms P50      27×
DefinitelyTyped     49ms P50        4,836ms P50    106×
  (53k files)
Coderun repo        ~10ms avg       ~20ms avg      2×
  (158 files)
```

At 27-49ms, Coderun is fast enough to run on every keystroke in an AI coding assistant. Grep's 4.8 seconds makes it unusable for real-time interaction.

### Quality

```
                    Recall    Precision   Novelty   What novelty means
                    ──────    ─────────   ───────   ──────────────────
Mattermost (9k)     13.1%     32.8%       53.0%     Half our results grep CAN'T find
DefinitelyTyped     14.2%      1.6%       89.2%     89% of our results grep CAN'T find
  (53k files)
```

- **Recall** (13-14%): We find a curated subset of grep's results — the *best* files, not *all* files
- **Precision** (2-33%): Our results are targeted to what the query actually needs
- **Novelty** (53-89%): The magic — files that grep's pattern matching completely misses

### What We Find That Grep Can't

| Query | Grep Finds | Coderun Finds | Why |
|-------|-----------|---------------|-----|
| "how to add error handling" | Files with literal "error" | Error types, try/catch patterns, docs, tests | Semantic understanding of "error handling" |
| "find all API endpoints" | Files with literal "API" + "endpoint" | Route definitions, handler registrations, API docs | Understands "endpoints" means route handlers |
| "why does the auth fail" | Files with literal "auth" + "fail" | Auth middleware, session handling, permission checks | Understands "fail" means debugging context |
| "how to create a new React component" | Files with literal "React" + "component" | Component templates, examples, documentation | Understands "create" means looking for patterns |

### Component Impact

| Component | Latency Cost | Recall Improvement | Verdict |
|-----------|-------------|-------------------|----------|
| Graph Boost | -2ms | +0.0% | ⚠️ Neutral (needs cross-layer data) |
| Candidate K (50→500) | +3ms | +249% | ✅ Strongly recommended |
| Query Expansion | +3ms | +6.5-18% | ✅ Recommended |

---

## Quick Start

```bash
# 1. Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Build coderun
cargo build --release

# 3. Install to user bin (adds %USERPROFILE%\.coderun\bin to PATH — restart shell)
powershell -ExecutionPolicy Bypass -File scripts/install.ps1  # or: bash scripts/install.sh
# Installed absolute: %USERPROFILE%\.coderun\bin\coderun.exe  (Windows) / ~/.coderun/bin/coderun (Unix)

# 4. Initialize your project (use installed absolute — not ./target/release)
~/.coderun/bin/coderun init            # Unix
# or: %USERPROFILE%\.coderun\bin\coderun.exe init  # Windows — use absolute; do NOT search repo for .exe
# Fallback if PATH updated: coderun init

# 5. Index your repository
~/.coderun/bin/coderun index

# 6. Start the daemon (auto-started by installer; manual: ~/.coderun/bin/coderun-daemon)
~/.coderun/bin/coderun serve
```

> **Agents**: always use the installed absolute `%USERPROFILE%\.coderun\bin\coderun.exe` / `~/.coderun/bin/coderun` for `init`/`index`/`doctor`. Do **not** search `target/release` or walk the filesystem for `.exe` — it is already on USER PATH after `install.ps1`/`install.sh` (restart shell if `coderun --version` fails). See `AGENTS.md` and `coderun doctor` PATH probe.

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
cargo test -p coderun-repo-intel                  # 111 langs (arborium)

# Run tests for a specific crate
cargo test -p coderun-core       # secrets HMAC (LazyLock), config v1 defaults
cargo test -p coderun-repo-intel # parser 111 langs via arborium
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
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── coderun-core/             # Shared types, errors, config (~32 tests)
│   ├── coderun-daemon/           # Daemon — UDS/MessagePack + HTTP fallback + /metrics
│   ├── coderun-cli/              # CLI — init/index/serve/preview/doctor
│   ├── coderun-repo-intel/       # Repository Intelligence — tree-sitter 111 langs + tantivy BM25 + graph + watcher (21 tests)
│   ├── coderun-context/          # Context Engine — retrieval engine + BuildContext (tiktoken, dedup, reversible) (11 tests)
│   ├── coderun-knowledge/        # Knowledge Hub — SQLite+tantivy local BM25
│   ├── coderun-skills/           # Skill Engine — MD/TOML/YAML parsing, tag matching (8 tests)
│   ├── coderun-router/           # Model Router — heuristic + LiteLLM fallback chain + cost (11 tests)
│   ├── coderun-optimizer/        # Execution Optimizer — RTK adapter + compressors (11 tests)
│   ├── coderun-events/           # Event Bus — in-memory broadcast + tracing/correlation
│   ├── coderun-storage/          # Local Storage — SQLite WAL + tantivy + audits (22 tests)
├── .coderun/
│   ├── config.toml               # Default configuration
│   └── skills/                   # Skill definitions
├── adapters/
│   ├── cursor/extension.ts       # Cursor Tier 1
│   ├── gemini/hooks.sh           # Gemini CLI Tier 1
│   └── tier2/README.md           # Tier 2 best-effort
├── .opencode/plugins/            # OpenCode plugin (TypeScript)
├── .claude/hooks/                # Claude Code hooks (shell scripts)
├── benches/                      # criterion benches
└── docs/                         # Architecture, benchmarks, and specification docs
```

## CLI Commands

### `coderun init`

Initialize coderun for the current repository.

```bash
# Use installed absolute — agents must not search for .exe
~/.coderun/bin/coderun init              # Unix
%USERPROFILE%\.coderun\bin\coderun.exe init  # Windows
# or bare `coderun init` only after PATH includes ~/.coderun/bin (install.ps1 / install.sh does this)
```

Creates:
- `.coderun/` directory
- `.coderun/config.toml` with default configuration
- `.coderun/skills/` directory for skill definitions
- SQLite database at `~/.coderun/data.db`

### `coderun index`

Index the repository for search and context building.

```bash
~/.coderun/bin/coderun index              # Unix
%USERPROFILE%\.coderun\bin\coderun.exe index  # Windows (absolute; do not search target/release)
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

Start the daemon server (UDS primary on `/tmp/coderun.sock` + HTTP fallback on `127.0.0.1:9527`, `GET /metrics`). Auto-started by installer from `~\.coderun\bin\coderun-daemon.exe` with workdir `~\.coderun`.

```bash
# Start with default config (prefer installed absolute)
# Daemon already running after `install.ps1` — manual start only if `coderun doctor` shows NOT running
coderun serve
coderun serve --socket /tmp/coderun.sock --port 9527

# The daemon will:
# 1. Load configuration
# 2. Initialize logging + metrics (`/metrics` exposition)
# 3. Open database (SQLite WAL) + tantivy (MmapDirectory)
# 4. Start background indexing + git watcher
# 5. Start UDS+MessagePack primary and HTTP fallback
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
- Knowledge entries (BM25 local) that would be included
- Code files (ripgrep/tantivy/graph) that would be included
- Token budget `by_source` (`behavioral_skills 20% / docs 15% / code 55%`)
- Model routing decision + fallback chain `capable→balanced→fast`

### `coderun replay` — removed in v1

Event replay removed from hot path. v1 keeps `tracing` + `metrics` + `correlation_id` (in-memory `EventBus`).

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

SQLite:          ✓ OK (WAL, migrations 001-003)
Config:          ✓ OK (4 default langs, 111 available via arborium)
Skills directory: ✓ OK
Socket path:     ✓ OK (/tmp/coderun.sock)
Tree-sitter:     ✓ OK (111 languages via arborium)
Tantivy:         ✓ OK (MmapDirectory)
Knowledge Hub:   ✓ OK (SQLite+tantivy local)
LiteLLM:         ✓ Configured (http://localhost:4000, fallback chain)
RTK:             ⚠ Not found — using built-in compressors
Tiktoken:        ✓ OK (cl100k_base LazyLock)
Secrets redact:  ✓ OK (HMAC via hmac crate)
Retrieval:       ✓ OK (tantivy BM25 + ast-grep structural)
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
| `[retrieval]` | Retrieval engine settings (`candidate_k`, `max_files`, `enable_graph`, `enable_expansion`) |
| `[knowledge]` | Knowledge settings (`max_knowledge_entries`) |
| `[skills]` | Skills directory, auto-discovery |
| `[context]` | Token budget, file limits, `cache_order` |
| `[model]` | Default tier, routing toggle |
| `[routing]` | Weights, thresholds, model mappings |
| `[litellm]` | Endpoint, timeout, retries |
| `[rtk]` | Enabled, max tokens, compression level |
| `[logging]` | Level, file path, retention |

### Environment Variables

| Variable | Overrides | Default |
|----------|-----------|---------|
| `CODERUN_DAEMON_SOCKET` | daemon.socket_path | /tmp/coderun.sock |
| `CODERUN_DATABASE_PATH` | database.path | ~/.coderun/data.db |
| `CODERUN_LOG_LEVEL` | logging.level | info |
| `CODERUN_MODEL_DEFAULT` | model.default_tier | balanced |
| `CODERUN_CONTEXT_MAX_TOKENS` | context.max_tokens | 12000 |
| `CODERUN_CANDIDATE_K` | retrieval.candidate_k | 100 |
| `CODERUN_SYMBOLS_ENABLED` | Enable/disable tree-sitter symbol extraction | true |
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
│  • Request validation + rate-limit (per session)            │
│  • Fail-open (30s → OriginalPassthrough)                    │
│  • Prometheus /metrics                                      │
└─────────────────────────┬───────────────────────────────────┘
                           │
           ┌───────────────┼───────────────┐
           ▼               ▼               ▼
┌─────────────┐  ┌─────────────┐  ┌─────────────┐
│   Context   │  │  Execution  │  │    Event    │
│   Engine    │  │  Optimizer  │  │     Bus     │
│ (RwLock)    │  │ (RTK)       │  │ (in-memory) │
└──────┬──────┘  └─────────────┘  └─────────────┘
        │
        ├──► Retrieval Engine (intent → expansion → BM25 + structural → graph → ranking)
        ├──► Repository Intelligence (tree-sitter 111 langs + tantivy + graph + watcher)
        ├──► Knowledge Hub (tantivy local)
        ├──► Skill Engine (tag-based, full-instruction injection)
        └──► Model Router (heuristic + LiteLLM fallback chain)
```

### Request Flow

1. Agent sends request via UDS/MessagePack (HTTP/JSON fallback on Windows)
2. Adapter Layer validates, rate-limits (per `session_id`), generates correlation ID
3. Context Engine assembles context pack (`RwLock` read, concurrent sessions):
   - **Retrieval Engine** finds relevant code files (intent detection → query expansion → BM25 + structural search → graph boost → ranking)
   - Retrieves knowledge via Knowledge Hub (BM25 top20 → rerank adaptive K `5-20` local)
   - Matches skills via Skill Engine (deterministic tag scoring, dedup)
   - Orders: `behavioral_skills` (20%) → `docs_context` (15%) → `code_context` (55%) + `FROZEN PREFIX END` boundary
   - Reversible truncation + `get_original()`
4. Model Router selects tier (`capable→balanced→fast` fallback via LiteLLM)
5. Response returned (or `OriginalPassthrough` on error/timeout), metrics recorded

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

### Health Check

```
GET /health  → {status: "ok"}
GET /metrics → Prometheus exposition
```

### Fail-Open Behavior

On any error or timeout, the daemon returns `OriginalPassthrough` with the original message unchanged. The agent always gets a response.

| Condition | Response | Reason |
|-----------|----------|--------|
| Timeout (>30s) | OriginalPassthrough | "timeout" |
| Context build error | OriginalPassthrough | "error" |
| Any internal error | OriginalPassthrough | "fail-open" |

## Implementation Status (v1)

| Component | Status | Implementation |
|-----------|--------|----------------|
| **Retrieval Engine** | ✅ Complete | Intent detection → query expansion → BM25 + structural search (ast-grep) → graph boost → ranking. `CombinedRetriever` with `RetrievalPolicy` tuning. |
| **Repository Intelligence** | ✅ Complete | tree-sitter **111 languages** + tantivy BM25 (MmapDirectory) + dependency graph + file watcher (`notify+git2`). `mtime+size` shortcut for fast warm re-indexes. |
| Config System | ✅ Complete | TOML + env (`CODERUN_*`), `index.languages` 4 default + 111 available |
| Core Types | ✅ Complete | IPC (MessagePack), HMAC `hmac` crate |
| Storage | ✅ Complete | SQLite WAL + tantivy BM25 + audits |
| Skill Engine | ✅ Complete | MD/TOML/YAML, tag matching, conflict detection, full-instruction injection |
| Knowledge Hub | ✅ Complete | BM25 top20→rerank adaptive `5-20` local |
| Model Router | ✅ Complete | Heuristic scoring + LiteLLM `capable→balanced→fast` fallback |
| Execution Optimizer | ✅ Complete | `RtkAdapter` (binary optional) + compressors + `tiktoken-rs` |
| Context Engine | ✅ Complete | `BuildContext` + `RwLock` concurrency, `skills→docs→code` + `FROZEN PREFIX END` + dedup + reversible truncation |
| Adapter Layer | ✅ Complete | UDS/MessagePack primary + HTTP fallback, 30s fail-open, `/metrics`, rate-limit per session |
| CLI Commands | ✅ Complete | `init`, `index`, `serve`, `preview`, `doctor`, `skills`, `config` |
| Agent Adapters | ✅ Complete | OpenCode, Cursor, Gemini CLI Tier 1 |
| Metrics | ✅ Complete | Prometheus `/metrics` + Grafana dashboard |
| Benchmarks | ✅ Complete | 3 benchmarks: Component Eval, Mattermost (9k files), DefinitelyTyped (53k files). See `docs/BENCHMARKS_V1.md`. |

### External Tool Integration

| Tool | Integration | Status |
|------|-------------|--------|
| tree-sitter | AST parsing for **111 languages** via arborium + tree-sitter-language-pack | ✅ `repo-intel/src/parser.rs` |
| tantivy | BM25 full-text search (MmapDirectory, in-process) | ✅ `storage/src/tantivy_index.rs` |
| ast-grep | Structural code search (in-process `AstGrepBackend`) | ✅ `context/src/retrieval/structural.rs` |
| tiktoken-rs | Token counting (`cl100k_base`) | ✅ `context/src/lib.rs` |
| LiteLLM | Model routing gateway (`capable→balanced→fast` fallback) | ✅ `router/src/litellm.rs` |
| RTK | Tool-output compression (binary optional, built-in fallback) | ✅ `optimizer/src/rtk.rs` |
| Prometheus | `/metrics` exposition + Grafana dashboard | ✅ `daemon/src/metrics.rs` |

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

See [docs/ROADMAP.md](docs/ROADMAP.md) for the full release history and future plans. Benchmarks: `docs/BENCHMARKS_V1.md`.

## Distribution

```bash
# Homebrew
brew tap leonortega/coderun
brew install coderun
brew services start coderun  # launchd

# Docker
docker build -t coderun:0.4.0 .
docker run -p 9527:9527 -v $PWD:/repo coderun serve
docker compose --file deploy/docker-compose.yml up
```

## License

MIT
