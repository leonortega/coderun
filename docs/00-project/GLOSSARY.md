# Glossary

## Purpose

Define all terms used across the AI Runtime for Coding Agents specification documents. This is the authoritative terminology reference.

## Terms

### Runtime

**Definition:** The AI Runtime system itself — a local application that improves coding agents by providing repository intelligence, context optimization, model routing, and tool-output compression.

**Scope:** Everything that runs as the daemon process. Excludes the coding agent, LiteLLM, and model providers.

### Daemon

**Definition:** The long-lived local process that hosts the Context Engine and all runtime modules. Communicates with the Adapter Layer over a Unix domain socket using MessagePack.

**Scope:** The process lifecycle is: start → initialize → listen → serve requests → shutdown.

### Agent

**Definition:** A coding agent that interacts with a developer to write, modify, and understand code. Examples: opencode, Claude Code, Cursor, Gemini CLI.

**Scope:** External to the runtime. The runtime improves the agent but does not become the agent.

### Tier 1 Agent

**Definition:** An agent with true programmatic hooks that fire unconditionally on every message/tool call. The runtime can intercept and rewrite messages with guaranteed execution.

**Agents:** opencode, Claude Code, Cursor, Gemini CLI, GitHub Copilot, OpenClaw, Pi, Factory Droid.

### Tier 2 Agent

**Definition:** An agent that only exposes convention-based integration (a rules file the agent may or may not follow). Support is best-effort, never with the same guarantee as Tier 1.

**Agents:** Codex, Windsurf, Cline, Kilo Code, Antigravity, Kimi.

### Adapter Layer

**Definition:** One thin adapter per agent CLI, implementing two operations: intercept-before-generation (rewrite the message) and intercept-before-tool-call (allow/deny/modify). Adapters translate between agent-specific hooks and the runtime's internal format.

**Scope:** The entry point of the runtime. One adapter type per supported agent.

### Pre-Generation Hook

**Definition:** A hook that fires before the model generates a response. The runtime intercepts the message, builds context, and rewrites the message with injected context.

**Examples:** opencode `chat.message`, Claude Code `UserPromptSubmit`.

### Pre-Tool Hook

**Definition:** A hook that fires before a tool executes. The runtime can intercept the tool output and compress it before it re-enters the model's context.

**Examples:** opencode `tool.execute.before`, Claude Code `PreToolUse`.

### Fail-Open

**Definition:** The behavior where, on timeout or error, the runtime passes the raw message through unmodified rather than blocking the agent or silently losing context injection.

**Scope:** Mandatory for all hook implementations. The agent always gets a response.

### Context Engine

**Definition:** The central component that implements `BuildContext(task)`. Retrieves relevant information, ranks and reranks results, deduplicates, compresses, orders for cache stability, enforces token budgets, and emits a Context Pack as YAML.

**Scope:** The one public entry point of the runtime. Runs as a long-lived daemon.

### BuildContext

**Definition:** The single public API of the Context Engine. Takes a task description and returns a Context Pack containing all context needed for the model to complete the task.

**Signature:** `BuildContext(task: TaskRequest) -> ContextPack`

### Context Pack

**Definition:** The final, token-budgeted package of context assembled for a single LLM request. Emitted as YAML with three sections in fixed order: `behavioral_skills`, `docs_context`, `code_context`.

**Scope:** Output of the Context Engine. Input to the model.

### Frozen-Prefix Boundary

**Definition:** An explicit boundary in the Context Pack that marks where cache-stable content ends and variable content begins. Content before the boundary is byte-identical across many tasks; content after changes between calls.

**Scope:** Part of the cache-awareness strategy. Maximizes prompt cache hit rates.

### Task

**Definition:** A description of work provided by the developer to the coding agent. The runtime receives this as input to `BuildContext`.

**Scope:** The runtime does not decompose tasks. Task decomposition is the agent's responsibility.

### Repository Intelligence

**Definition:** The system that parses, indexes, and understands a codebase incrementally. Uses tree-sitter for AST parsing, ast-grep for structural search, ripgrep for text search. Updated on git changes, not per-request.

**Scope:** Owns: incremental parsing, structural search, text search, metadata storage.

### Knowledge Hub

**Definition:** One organizational surface for project docs, skills, rules, ADRs, templates, and long-term memory. Composes three retrieval strategies: tag-based skill matching, BM25/tantivy lexical search with FlashRank reranking for docs/code, and engram for memory.

**Scope:** Owns: storage and retrieval of all knowledge types.

### Skill

**Definition:** A named, reusable instruction set that teaches the coding agent how to perform a specific type of task. Skills come from community formats: Claude, Cursor, Continue, agentskills.io. Matched by deterministic tag-based scoring.

**Scope:** Part of the Knowledge Hub. Injected into the Context Pack as `behavioral_skills`.

### Skill Engine

**Definition:** The component that performs task classification, skill activation, conflict detection, priority resolution, and instruction injection. Deterministic, tag-based, against a small registry.

**Scope:** Part of the Knowledge Hub's skill subsystem.

### Skill Registry

**Definition:** The in-memory collection of loaded skill definitions. Small (dozens of entries, not thousands). Skills are loaded from community-format files at daemon startup.

**Scope:** Managed by the Skill Engine.

### Context

**Definition:** Information provided to the LLM to help it understand and complete a task. Includes: skill instructions, documentation, code snippets, repository metadata.

**Scope:** Built by the Context Engine. Managed within token budgets.

### Model Router

**Definition:** The component that selects which LLM model to use for a given task based on task complexity, available models, latency targets, budget, and required capabilities.

**Scope:** Owns: heuristic complexity scoring, model tier selection, LiteLLM configuration.

### Model Tier

**Definition:** A classification of models by capability and cost. Three tiers in v1:

| Tier | Description | Example Models |
|------|-------------|----------------|
| Fast | Low cost, high speed, suitable for simple tasks | gpt-4o-mini, claude-3-haiku |
| Balanced | Moderate cost, moderate speed, suitable for most tasks | gpt-4o, claude-3-sonnet |
| Capable | High cost, lower speed, suitable for complex reasoning | o1, claude-3-opus |

### Model Gateway

**Definition:** The infrastructure layer that unifies multiple LLM providers behind one API shape. In v1, LiteLLM serves as the model gateway with routing strategies, per-key budgets, cost tracking, and fallback chains.

**Scope:** External to the runtime's core logic, but integrated via the Model Router.

### Execution Optimizer

**Definition:** The component that compresses and optimizes tool outputs before they re-enter the model's context. Uses RTK directly rather than building an equivalent.

**Scope:** Intercepts tool outputs via pre-tool-call hooks.

### RTK

**Definition:** RunTime Kit — a Rust binary for tool-output compression. Zero dependencies, <10ms overhead. Intercepts tool/command output and rewrites it to compact form.

**Scope:** Adopted directly, not reimplemented.

### Tool Output

**Definition:** The result of a tool execution by the coding agent. Includes: file contents read by the agent, search results, shell command output, and any other structured output returned to the model.

**Scope:** Compressed by the Execution Optimizer via pre-tool hooks.

### Event Bus

**Definition:** An async-only system for observability events. Events: ContextBuilt, SkillActivated, RepositoryUpdated, ToolExecuted, ModelSelected, ResponseGenerated, MemorySaved. Never in the `BuildContext` call path.

**Scope:** Consumed by CLI inspection, metrics, and future orchestrators.

### Memory

**Definition:** Long-term storage of information across sessions. Implemented via engram: a single Go binary, SQLite+FTS5, MCP-native, no LLM/embedding dependency for its core save/search path.

**Scope:** Used by the Knowledge Hub for cross-session knowledge persistence.

### engram

**Definition:** A memory system (`Gentleman-Programming/engram`): single Go binary, SQLite+FTS5, MCP-native. Provides save and search capabilities without LLM or embedding dependencies.

**Scope:** Used for persistent memory in the Knowledge Hub.

### BM25

**Definition:** A lexical scoring algorithm used for full-text search. Computes relevance based on term frequency and inverse document frequency. Used by tantivy for code and documentation retrieval.

**Scope:** Part of the Knowledge Hub's retrieval pipeline.

### FlashRank

**Definition:** A reranking model that reorders search results using a cross-encoder. Runs in-process via `ort` (Rust ONNX Runtime bindings). Applied after BM25 scoring to improve result quality.

**Scope:** Part of the Knowledge Hub's retrieval pipeline.

### Token Budget

**Definition:** The maximum number of tokens allocated for a Context Pack. Configured in TOML. The Context Engine enforces this budget by selecting and truncating content to fit.

**Scope:** Enforced by the Context Engine.

### Token Counting

**Definition:** The process of counting tokens locally using `tiktoken-rs`. Never via a model API round-trip. Provides accurate token counts for budget enforcement.

**Scope:** Used by the Context Engine throughout context construction.

### Correlation ID

**Definition:** A unique identifier assigned to each request, propagated across all components and included in log entries. Enables tracing a single request through the entire runtime.

**Scope:** Generated by the Adapter Layer. Used by all components for logging.

### Configuration

**Definition:** Runtime settings defined in TOML format. Includes: model settings, token budgets, skill paths, daemon settings, agent-specific options, logging levels, and database paths.

**Scope:** Loaded at daemon startup.

### Index

**Definition:** The repository metadata store (SQLite) and search indices (BM25/tantivy) that collectively represent the runtime's understanding of a repository.

**Scope:** Owned by Repository Intelligence. Persistent across daemon restarts.

### IContextBuilder

**Definition:** The interface contract for context building. Supports in-process, daemon, and remote implementations. The Context Engine implements this interface.

**Scope:** Defined as a contract for portability. Concrete implementation is the Rust daemon.

### IModelGateway

**Definition:** The interface contract for model routing and inference. Default implementation is LiteLLM. Supports swapping to other gateways.

**Scope:** Defined as a contract for portability. Concrete implementation is LiteLLM.

### IWorkflowEngine

**Definition:** The interface contract for external workflow orchestration. Optional, external to the runtime. Implementations: Temporal, DBOS Transact.

**Scope:** Not implemented in v1. Defined for future extensibility.

### Prompt Caching

**Definition:** A cost optimization where the model provider caches a prefix of the prompt and charges less for cached tokens. The runtime maximizes cache hits by ordering context for stability.

**Scope:** First-class concern in the Context Engine's pack ordering.

### Inspection Command

**Definition:** A CLI command that can preview or replay what a given prompt would build (or did build). Generalizes the session-trace-inspection pattern.

**Scope:** Consumes event bus events. Part of the CLI.
