# Project: AI Runtime for Coding Agents

## Purpose

A local AI Runtime that improves existing coding agents by providing repository intelligence, optimized context construction, intelligent model routing, and tool-output compression. It sits between coding agents and LLM providers, making every agent interaction more efficient and contextually aware — without replacing the agent itself.

## Problem Statement

Current coding agents (opencode, Claude Code, Cursor, Gemini CLI, etc.) operate with significant limitations:

1. **No persistent repository understanding.** Agents re-analyze the entire repository on every request. They lack persistent knowledge of project structure, conventions, patterns, and architecture.

2. **Naive context selection.** Agents either dump entire files into context or rely on developers to manually specify what matters. This wastes tokens and misses critical information.

3. **No skill awareness.** Agents cannot discover or apply domain-specific workflows (e.g., "migrate a database schema," "add a new API endpoint following project conventions") without developers manually providing instructions each time.

4. **Tool-output bloat.** Tool outputs (file reads, search results, shell commands) are passed through uncompressed, inflating token usage by 3–10× with redundant or irrelevant content.

5. **One-size-fits-all model routing.** Agents use a single model for all tasks regardless of whether the task is simple formatting or complex architectural reasoning.

6. **No cost optimization.** Without prompt caching awareness, agents miss the single biggest available cost lever: ordering context for maximum cache hits.

## Goal

Build a local runtime that solves these six problems, without becoming a coding agent itself. The runtime exposes one clean API: `BuildContext(task)` plus a routing call. It works standalone for solo developers and can be extended with external orchestration for teams needing approvals and audit trails.

## Target Users

| User | Description |
|------|-------------|
| **Solo developers** | Individual developers using a coding agent for personal projects |
| **Small teams** | Teams of 2–10 developers using coding agents who want better context and lower costs |
| **Agent builders** | Developers building or extending coding agents who need a runtime layer |

## Target Agents (v1)

### Tier 1 — Programmatic Hooks (Full Support)

These agents expose true programmatic hooks that fire unconditionally on every message/tool call:

| Agent | Pre-Generation Hook | Pre-Tool Hook |
|-------|---------------------|---------------|
| opencode | `chat.message` | `tool.execute.before` |
| Claude Code | `UserPromptSubmit` | `PreToolUse` |
| Cursor | (TBD) | (TBD) |
| Gemini CLI | (TBD) | (TBD) |
| GitHub Copilot | (TBD) | (TBD) |
| OpenClaw | (TBD) | (TBD) |
| Pi | (TBD) | (TBD) |
| Factory Droid | (TBD) | (TBD) |

### Tier 2 — Convention-Based (Best-Effort)

These agents only expose convention-based integration (a rules file the agent may or may not follow). Support is explicitly labeled best-effort:

| Agent | Integration Method |
|-------|--------------------|
| Codex | Rules file |
| Windsurf | Rules file |
| Cline | Rules file |
| Kilo Code | Rules file |
| Antigravity | Rules file |
| Kimi | Rules file |

## Core Use Cases

### Use Case 1: Repository-Aware Task Execution

A developer asks a coding agent to "add rate limiting to the API." The runtime intercepts the message before the model sees it, retrieves the project's middleware conventions, the existing authentication middleware as a reference, the router file that needs modification, and relevant database schema details — then injects this context into the message.

### Use Case 2: Cache-Optimized Context

The runtime orders context as skills → docs → code (most to least cache-stable). An explicit frozen-prefix boundary ensures only content after that boundary changes between calls. This maximizes prompt cache hit rates, which is the single biggest cost lever available.

### Use Case 3: Smart Model Selection

A coding agent needs to generate a unit test (simple task) and refactor a core algorithm (complex task). The runtime routes the simple task to a fast, cheap model and the complex task to a capable, expensive model — through LiteLLM as the gateway.

### Use Case 4: Tool-Output Compression

The coding agent reads a large log file or search result. The runtime intercepts the tool output before it re-enters the context, compresses it via RTK, and passes the compact version — reducing token consumption by 50–80% while preserving the information the model needs.

### Use Case 5: Knowledge Accumulation

Over multiple interactions, the runtime builds persistent knowledge about the repository via local SQLite+tantivy: coding conventions, architectural patterns, frequently modified files, and domain terminology (engram removed — see `docs/01-architecture/ENGRAM_CBM_REMOVAL.md`). This knowledge improves future context packages.

## Primary Value Proposition

**Every coding agent interaction produces better output with fewer tokens and lower cost.**

| Metric | Value |
|--------|-------|
| Token reduction per request | 30–50% |
| First-pass accuracy improvement | 20–40% |
| Tool-output token reduction | 50–80% |
| Latency overhead (runtime processing) | < 30 seconds (target: low single digits) |
| Repository indexing time (100k lines) | < 30 seconds |

## Success Criteria

| Criterion | Measurement | Target |
|-----------|-------------|--------|
| Token reduction | Tokens sent vs. naive context packing | 30–50% reduction |
| First-pass accuracy | Correct output on first attempt (Promptfoo eval) | 20–40% improvement over baseline |
| Tool-output compression | Token reduction in tool outputs | 50–80% reduction |
| Cache hit rate | Prompt cache hit rate with cache-aware ordering | > 80% on repeat tasks |
| Latency overhead | Added latency from runtime processing | < 30s hard limit, < 5s typical |
| Fail-open reliability | Requests pass through unmodified on error | 100% fail-open compliance |
| Repository indexing time | Time to index a 100k-line repository | < 30 seconds |
| Model routing accuracy | Correct model tier selection | > 90% accuracy |
| Stability | Crash rate | < 0.1% of requests |

## v1 Capabilities

| Capability | Description |
|------------|-------------|
| Agent interception | Pre-generation and pre-tool-call hooks for Tier 1 agents |
| Repository indexing | Incremental AST parsing with tree-sitter, structural search with ast-grep, text search with ripgrep |
| Knowledge retrieval | BM25/tantivy lexical search (FlashRank removed — see `docs/01-architecture/FLASHRANK_REMOVAL.md`, reranker is passthrough) |
| Memory | Persistent memory via SQLite+tantivy local (engram removed — see ENGRAM_CBM_REMOVAL.md) |
| Skill matching | Deterministic tag-based skill activation from community formats |
| Context construction | Token-budgeted YAML context pack with cache-aware ordering |
| Model routing | Heuristic complexity scoring → tier selection → LiteLLM gateway |
| Tool-output optimization | RTK-based compression for tool outputs |
| Event bus | Async observability events (ContextBuilt, SkillActivated, etc.) |
| Local persistence | SQLite for index, metadata, and memory (engram removed) |

## v1 Limitations

| Limitation | Description | Future Resolution |
|------------|-------------|-------------------|
| Single repository | Runtime handles one repository per daemon process | Multi-repo in v2 |
| No conversation memory | Runtime does not persist conversation history across sessions | Session memory deferred (engram removed — see ENGRAM_CBM_REMOVAL.md) |
| No multi-agent coordination | Runtime serves one agent instance at a time | Concurrent agent support in v2 |
| No web UI | CLI-only interface | Dashboard in v2 |
| No plugin system | Skills are file-based community formats | Dynamic plugin system in v2 |
| No distributed deployment | Single local daemon process | Distributed runtime in v2 |
| No vector/semantic recall | Uses tantivy BM25 lexical recall only (engram removed) | Semantic recall only if lexical proves insufficient |
| No relationship-aware retrieval | Uses BM25 + reranking only | Graph-based retrieval only if concrete query pattern requires it |
| No external orchestration | Runtime works standalone | DBOS required since v0.6.0 (SQLite+Litestream native); Temporal deleted |
| Tier 2 agents | Best-effort only, no guarantee of hook compliance | N/A — by design (v0.6.0 keeps README-only) |
