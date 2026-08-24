# Scope

## Purpose

Define what the AI Runtime for Coding Agents does, what it does not do, and who owns each responsibility. This is the authoritative boundary reference for all implementation work.

## In Scope (v1)

| Area | What Is Included |
|------|------------------|
| **Agent Interception** | Pre-generation and pre-tool-call hooks for Tier 1 agents (opencode, Claude Code, Cursor, Gemini CLI, Copilot, OpenClaw, Pi, Factory Droid). Tier 2 agents supported as best-effort via convention-based integration. |
| **Repository Intelligence** | Incremental AST parsing (tree-sitter), structural search (ast-grep), text search (ripgrep), git-change-triggered incremental updates, metadata storage. Optional LSP enrichment via agent's own language server. |
| **Knowledge Hub** | Unified organizational surface for docs, skills, rules, ADRs, templates, and memory. BM25/tantivy for lexical retrieval. FlashRank for reranking. engram (SQLite+FTS5) for persistent memory. |
| **Skill Engine** | Deterministic tag-based skill matching from community formats (Claude, Cursor, Continue, agentskills.io). Task classification, skill activation, conflict detection, priority, instruction injection. |
| **Context Engine** | `BuildContext(task)` — the one public API. Retrieve → rank → rerank → deduplicate → compress → cache-order → token-budget → emit YAML Context Pack. Runs as a long-lived local daemon with Unix socket IPC. Local token counting via `tiktoken-rs`. |
| **Model Router** | Heuristic complexity/budget/capability scorer. Tier selection (fast/balanced/capable). LiteLLM as model gateway for multi-provider routing, fallback chains, per-key budgets, cost tracking. |
| **Execution Optimizer** | RTK adopted directly for tool-output compression. Intercepts tool outputs via pre-tool-call hooks. |
| **Event Bus** | Async-only observability events: ContextBuilt, SkillActivated, RepositoryUpdated, ToolExecuted, ModelSelected, ResponseGenerated, MemorySaved. Consumed by inspection CLI, metrics, and future orchestrators. |
| **Local Persistence** | SQLite for repository index and metadata. engram for memory. Filesystem for skill definitions, configuration, and logs. |
| **CLI** | Start daemon, initialize repository, inspect events, preview/replay prompts, health check. |
| **Configuration** | TOML-based configuration for model settings, token budgets, skill paths, daemon settings, agent-specific options. |
| **Offline Evaluation** | Promptfoo configuration for CI regression and scheduled eval against real usage logs. |

## Out of Scope (v1)

| Area | Why It Is Excluded |
|------|---------------------|
| **Code editing** | The coding agent owns all code modification |
| **Shell execution** | The coding agent owns all shell commands |
| **Test execution** | The coding agent owns test running |
| **Git operations** | The coding agent owns commits, branches, and merges |
| **Deployment** | External tool responsibility |
| **CI/CD orchestration** | External tool responsibility |
| **Conversational state** | The coding agent owns conversation history |
| **User interaction** | The coding agent owns the user interface |
| **Multi-tenancy** | Single user, single repository per daemon |
| **Plugin marketplace** | Skills are community-format files, not a marketplace |
| **Workflow orchestration** | No Temporal, LangGraph, or workflow engine in the runtime |
| **Distributed infrastructure** | Single local daemon process |
| **Web dashboard** | CLI-only interface |
| **Authentication** | Local daemon, no auth needed |
| **Rate limiting** | LiteLLM handles provider rate limiting |
| **Model fine-tuning** | Routes to existing models only |
| **Data labeling** | No human-in-the-loop labeling |
| **Audit trail** | Logging and events only, no formal audit system |
| **Collaborative editing** | Single developer per session |
| **Background jobs** | All processing is request-response (daemon) or async (event bus) |
| **Human approval workflows** | Out of scope; added by external orchestrator if needed |
| **Governance dashboards** | Out of scope; added by external orchestrator if needed |
| **Mission management** | No mission/workflow decomposition in the runtime |
| **Quality gates** | Native per-language analyzers run externally; runtime does not enforce them |

## Responsibility Boundaries

### Runtime Owns

| Responsibility | Details |
|----------------|---------|
| Agent interception | Pre-generation and pre-tool-call hooks |
| Repository parsing | tree-sitter AST parsing, incremental updates on git change |
| Code indexing | Structural search (ast-grep), text search (ripgrep), metadata storage |
| Knowledge storage | Docs, skills, rules, ADRs, templates, memory (engram) |
| Knowledge retrieval | BM25/tantivy lexical search + FlashRank reranking |
| Skill matching | Deterministic tag-based activation from community formats |
| Context assembly | Token-budgeted YAML Context Pack with cache-aware ordering |
| Model selection | Heuristic complexity scoring, tier selection |
| Tool-output compression | RTK-based compression via pre-tool-call hooks |
| Token accounting | Local token counting via tiktoken-rs |
| Observability | Event bus for async metrics and inspection |

### Coding Agent Owns

| Responsibility | Details |
|----------------|---------|
| User interaction | Prompts, responses, UI rendering |
| Conversation management | Multi-turn state, message history |
| Code editing | File creation, modification, deletion |
| Shell execution | Running commands, scripts, tools |
| Test execution | Running tests, parsing results |
| Git operations | Commits, branches, diffs, merges |
| Tool definitions | Defining available tools for the model |
| Error presentation | Showing errors to the developer |
| Retry logic | Deciding when and how to retry failed operations |
| Model API calls | The runtime routes to the model; the agent may also call models directly |

### External Tools Own

| Responsibility | Details |
|----------------|---------|
| LLM inference | Model providers (OpenAI, Anthropic, Google, etc.) |
| Provider authentication | API keys and credentials |
| Provider rate limiting | Quota management |
| LiteLLM gateway | Model routing, fallbacks, load balancing (if deployed externally) |
| Language servers | Optional LSP enrichment (agent's own processes) |
| Static analysis | Native per-language analyzers |
| External orchestration | Temporal/DBOS (only if governance needed, separate product) |

## v1 Boundaries

### Process Boundary

```
┌──────────────────────────────────────────────────────────┐
│                     Developer Machine                     │
│                                                          │
│  ┌─────────────────────┐    ┌──────────────────────────┐ │
│  │    Coding Agent     │    │   Coderun Daemon         │ │
│  │                     │    │                          │ │
│  │  - UI               │    │  ┌────────────────────┐  │ │
│  │  - Code editing     │◄──►│  │  Adapter Layer     │  │ │
│  │  - Shell exec       │ UDS│  │  (Tier 1/Tier 2)   │  │ │
│  │  - Git ops          │    │  └────────┬───────────┘  │ │
│  │  - Conversation     │    │           │              │ │
│  │                     │    │  ┌────────▼───────────┐  │ │
│  └─────────────────────┘    │  │  Context Engine    │  │ │
│                              │  │  (BuildContext)    │  │ │
│                              │  └────────┬───────────┘  │ │
│                              │           │              │ │
│           ┌──────────────────┼───────────┼──────────┐   │
│           │                  │           │          │   │
│  ┌────────▼──────┐  ┌───────▼────┐ ┌────▼─────┐ ┌─▼──────────┐ │
│  │ Repo Intel    │  │Knowledge Hub│ │Skill Eng │ │Model Router│ │
│  │ (tree-sitter, │  │(engram,    │ │(tag-     │ │(heuristic, │ │
│  │  ast-grep,    │  │ BM25,      │ │ based)   │ │ LiteLLM)   │ │
│  │  ripgrep)     │  │ FlashRank) │ │          │ │            │ │
│  └───────────────┘  └────────────┘ └──────────┘ └────────────┘ │
│                              │                                   │
│  ┌───────────────────────────▼──────────────────────────────┐   │
│  │                    Event Bus (async)                      │   │
│  │  ContextBuilt, SkillActivated, RepositoryUpdated,        │   │
│  │  ToolExecuted, ModelSelected, ResponseGenerated,         │   │
│  │  MemorySaved                                              │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                 Local Storage                             │   │
│  │  - SQLite (index, metadata)                               │   │
│  │  - engram (memory, FTS5)                                  │   │
│  │  - Filesystem (skills, config, logs)                      │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
           │
           │  HTTP/HTTPS
           ▼
  ┌─────────────────┐
  │     LiteLLM     │
  │  (Local/Remote) │
  └────────┬────────┘
           │
           │  HTTPS
           ▼
  ┌─────────────────┐
  │  Model Provider │
  │  (OpenAI, etc.) │
  └─────────────────┘
```

### Communication Boundary

| Path | Protocol | Direction | Purpose |
|------|----------|-----------|---------|
| Agent → Daemon | Unix Domain Socket (MessagePack) | Bidirectional | Pre-generation hooks, pre-tool hooks |
| Daemon → LiteLLM | HTTP/HTTPS | Outbound | Model routing and inference |
| Daemon → engram | HTTP API | Bidirectional | Memory read/write |
| Daemon → SQLite | In-process (rusqlite) | Bidirectional | Index and metadata |
| Daemon → Event Bus | Internal async channel | Outbound only | Observability events |

### Data Boundary

| Data | Owner | Persistence |
|------|-------|-------------|
| Repository source code | Developer / Git | Filesystem (read-only by runtime) |
| Repository index | Runtime | SQLite database |
| Repository metadata | Runtime | SQLite database |
| Memory entries | Runtime | engram (SQLite+FTS5) |
| Skill definitions | Developer | TOML/community-format files on filesystem |
| Configuration | Developer | TOML files on filesystem |
| Conversation history | Coding Agent | Not stored by runtime |
| Token usage metrics | Runtime | SQLite database |
| Logs | Runtime | Log files on filesystem |
| API keys | Developer / LiteLLM | Environment variables or LiteLLM config |

## Future Features (Must NOT Affect v1)

| Feature | Planned Version | v1 Impact |
|---------|-----------------|-----------|
| Multi-repository support | v2 | None. v1 uses single-repo schema |
| Conversation memory | v2 | None. v1 is stateless across requests |
| Plugin system | v2 | None. v1 uses community-format skills |
| Web dashboard | v2 | None. v1 is CLI-only |
| Distributed deployment | v2 | None. v1 is single daemon |
| Multi-agent coordination | v2 | None. v1 serves one agent |
| Collaborative editing | v3 | None. v1 is single-developer |
| Model fine-tuning | v3 | None. v1 routes to existing models |
| CI/CD integration | v2 | None. v1 is request-response |
| Workflow engine | v2 | None. v1 has no workflow abstraction |
| Enterprise governance | v3 | None. v1 has no auth or audit |
| Vector/semantic recall | Deferred | None. v1 uses FTS5 lexical recall only |
| Graph-based retrieval | Deferred | None. v1 uses BM25 + reranking only |
| External orchestration | Separate product | None. Runtime is consumed via API |
