# Engineering Principles

## Purpose

Define the engineering principles that govern every implementation decision. When a design choice arises, these principles resolve it.

## Principles

### 1. Deterministic Before AI

**Rule:** No LLM call decides whether to retrieve, compress, which skill applies, or which model tier to use. All such decisions use heuristics or deterministic algorithms. LLM calls are reserved for doing the actual work the user asked for.

**Implications:**
- Repository structure analysis is deterministic (tree-sitter parsing)
- Skill matching uses tag-based scoring, not semantic similarity
- Context selection uses structural relevance (file relationships, imports, BM25 scores)
- Model routing uses a heuristic complexity scorer, not an AI classifier
- Tool-output compression uses RTK's deterministic rules
- The only non-deterministic element is the LLM response itself

### 2. Interception Before the Model

**Rule:** Context injection and tool-output optimization happen before the model sees anything, via each agent's own native hooks — not a reverse proxy, not an MCP tool the agent can choose to skip.

**Implications:**
- Use opencode's `chat.message` and `tool.execute.before` hooks
- Use Claude Code's `UserPromptSubmit` and `PreToolUse` hooks
- Hooks fire unconditionally on every message/tool call
- The agent cannot bypass the runtime's context injection
- Fail-open on timeout or error: pass the raw message through unmodified

### 3. Fail-Open is Mandatory

**Rule:** On timeout or error, pass the raw message through unmodified rather than silently losing context injection with no signal. The runtime must never block or break the agent.

**Implications:**
- `UserPromptSubmit` has a hard 30-second timeout (Claude Code) that silently discards output if exceeded
- Target well under 30 seconds, ideally low single digits
- On any error in the Context Engine, return the original message unchanged
- Log the failure for debugging, but do not block the agent
- The agent always gets a response, even if it's the unmodified original

### 4. Cache-Awareness is First-Class

**Rule:** Prompt caching is the single biggest available cost lever. The Context Pack is ordered for maximum cache stability: skills → docs → code (most to least cache-stable). An explicit frozen-prefix boundary ensures only content after that boundary changes between calls.

**Implications:**
- Context Pack YAML has three sections in fixed order: `behavioral_skills`, `docs_context`, `code_context`
- Skills are byte-identical across many tasks (most cache-stable)
- Docs change rarely (second most stable)
- Code changes frequently (least stable)
- Frozen-prefix boundary marks where stable content ends
- Compression should be reversible by default — the model can request originals

### 5. Local-First

**Rule:** All core processing runs on the developer's machine. No external service is required for the runtime to function, except the LLM provider accessed through LiteLLM.

**Implications:**
- SQLite is the primary database, not a remote database
- SQLite+tantivy runs in-process (engram removed — see `docs/01-architecture/ENGRAM_CBM_REMOVAL.md`)
- BM25/tantivy runs in-process
- FlashRank runs in-process via `ort` (Rust ONNX Runtime bindings) — removed, offline only
- tree-sitter, ast-grep, ripgrep are embedded as native Rust crates, not shelled out
- Repository data never leaves the machine unless sent to an LLM provider
- Network failures (other than LLM provider) do not break the runtime

### 6. Reuse Existing Tools

**Rule:** Use established, mature tools for every capability they provide. Do not reimplement functionality that exists in a well-maintained library. Custom code is reserved for the two things genuinely specific to this problem: the Context Pack's ranking/schema logic and the heuristic router.

**Implications:**
- tree-sitter for all AST parsing — embedded as native Rust crate
- ast-grep for structural code search — embedded as native Rust crate
- ripgrep for text search — embedded as native Rust crate
- BM25/tantivy for full-text indexing and search
- FlashRank for reranking — via `ort` (ONNX Runtime) — removed (see `ENGRAM_CBM_REMOVAL.md` / `FLASHRANK_REMOVAL.md`)
- engram for memory — SQLite+FTS5, MCP-native — removed (see `ENGRAM_CBM_REMOVAL.md`; SQLite+tantivy local)
- LiteLLM for all LLM communication — model gateway
- RTK for tool-output compression — adopted directly
- tiktoken-rs for local token counting — never via model API round-trip

### 7. Minimal Runtime

**Rule:** The runtime does the minimum necessary to improve the coding agent. It does not replicate agent capabilities. It does not accumulate orchestration, governance, or SDLC features.

**Implications:**
- No code editing logic
- No shell execution
- No conversation state management
- No user interface beyond CLI
- No workflow orchestration
- No task scheduling
- No event-driven architecture for the hot path
- Components communicate through direct function calls, not message passing
- The event bus is strictly async/observability, never in the `BuildContext` call path

### 8. Concrete v1

**Rule:** Every design decision must resolve to a single concrete implementation for v1. No abstract interfaces without a concrete reason. No "TBD" fields. No generic plugin points.

**Implications:**
- One database schema, not a schema versioning system
- One configuration format (TOML)
- One IPC protocol (Unix domain socket with MessagePack)
- One context format (YAML with fixed sections)
- Direct struct usage, not trait objects for component interfaces
- Concrete error types, not generic error enums
- Interfaces (`IContextBuilder`, `IModelGateway`, `IWorkflowEngine`) are defined as contracts but implemented concretely for v1

### 9. Incremental Repository Intelligence

**Rule:** Repository knowledge builds up incrementally, triggered by git changes — exactly like a language server's own indexing. Never rebuilt on every prompt.

**Implications:**
- tree-sitter's native incremental parsing is used
- Git change triggers incremental reparse
- Stored repository model is updated, not rebuilt
- The Context Engine consumes stored metadata, not live parsing
- Indexing runs in the background, not on the hot path

### 10. Token Efficiency

**Rule:** Every token sent to a model must earn its place. Reduce token count at every stage without losing information the model needs.

**Implications:**
- Context packs have hard token budgets enforced by the Context Engine
- Tool outputs are compressed via RTK before inclusion in context
- Only relevant files are included, not entire directories
- Duplicated content is deduplicated across context sources
- Token counts are tracked locally via tiktoken-rs (never via model API)
- Skill instructions are concise and task-specific

### 11. Observable Behavior

**Rule:** Every significant operation produces structured logs. The runtime's behavior can be understood from logs alone. The event bus provides async observability without impacting the hot path.

**Implications:**
- Every component uses structured logging
- Log levels: ERROR for failures, WARN for recoverable issues, INFO for request lifecycle, DEBUG for component decisions
- Each request gets a unique correlation ID propagated across all components
- Token counts are logged at every stage
- Model routing decisions are logged with reasoning
- Event bus events: ContextBuilt, SkillActivated, RepositoryUpdated, ToolExecuted, ModelSelected, ResponseGenerated, MemorySaved
- CLI inspection command can preview/replay what a prompt would build/did build

### 12. Portability via Interfaces

**Rule:** Define `IContextBuilder` (in-process / daemon / remote), `IModelGateway` (default: LiteLLM), and `IWorkflowEngine` (external, optional) as explicit contracts. Reference implementations are chosen for concrete reasons and remain swappable behind these interfaces.

**Implications:**
- The Context Engine implements `IContextBuilder`
- The Model Router implements `IModelGateway`
- External orchestration implements `IWorkflowEngine` (separate product)
- Reference implementations are Rust (Context Engine), LiteLLM (Model Router), Temporal/DBOS (Workflow Engine)
- Swapping implementations requires only re-implementing the interface, not modifying the runtime

### 13. Knowledge is Organizationally Unified

**Rule:** Docs, skills, rules, ADRs, templates, and memory live behind one Knowledge Hub API — but skills are matched by a small, deterministic tag-based registry, while docs/code retrieval is large-corpus lexical search plus reranking. These are different problems; do not collapse them into one ranking pipeline.

**Implications:**
- Knowledge Hub has one API surface for storage and retrieval
- Skill matching uses tag-based scoring (deterministic, small registry)
- Doc/code retrieval uses BM25 lexical search (FlashRank removed — see `docs/01-architecture/FLASHRANK_REMOVAL.md`, reranker is passthrough; lexical search, large corpus)
- Memory uses SQLite+tantivy local (engram removed — see `ENGRAM_CBM_REMOVAL.md`)
- These three subsystems are composed, not unified into one algorithm

### 14. Report Savings Honestly

**Rule:** Separate "reduction in the specific thing measured" (e.g., bash output size) from "reduction in your bill" (diluted by system prompt, history, and output tokens). Do not oversell.

**Implications:**
- Report tool-output compression ratio separately from total cost reduction
- Report token reduction per request separately from bill impact
- Include caveats about system prompt, history, and output tokens in cost claims
- Use Promptfoo for objective evaluation, not cherry-picked examples

## Principle Hierarchy

When principles conflict, resolve in this order:

1. **Fail-Open is Mandatory** — The agent must never be blocked or broken
2. **Deterministic Before AI** — Predictable behavior builds trust
3. **Cache-Awareness is First-Class** — Prompt caching is the biggest cost lever
4. **Local-First** — Privacy and offline capability are non-negotiable
5. **Concrete v1** — Ship a working system, not a framework
6. **Minimal Runtime** — Do less, do it well
7. **Token Efficiency** — Every token has a cost
8. **Reuse Existing Tools** — Leverage the ecosystem
9. **Interception Before the Model** — Native hooks, not proxies
10. **Observable Behavior** — If you can't see it, you can't fix it
11. **Incremental Repository Intelligence** — Never rebuild on every prompt
12. **Portability via Interfaces** — Swappable implementations
13. **Knowledge is Organizationally Unified** — One API, different retrieval strategies
14. **Report Savings Honestly** — Measure and report accurately
