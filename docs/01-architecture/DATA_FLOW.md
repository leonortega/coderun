# Data Flow

## Purpose

Describe all important data flows through the AI Runtime. Each flow shows the sequence of operations, data transformations, and module interactions.

## Flow 1: Repository Indexing (Incremental)

### Trigger

- Git change detected (file system watcher or manual trigger)
- `coderun index` command
- SIGHUP or SIGUSR1 signal

### Sequence

```mermaid
sequenceDiagram
    participant GI as Git Change
    participant RI as Repository Intelligence
    participant TS as tree-sitter
    participant RG as ripgrep
    participant AG as ast-grep
    participant TV as BM25/Tantivy
    participant DB as SQLite
    participant KH as Knowledge Hub
    participant EB as Event Bus

    GI->>RI: RepositoryUpdated (file change)
    RI->>DB: SELECT path, hash FROM files
    DB-->>RI: known_files

    RI->>RI: walk_directory_tree()
    RI->>RI: diff(current, known)

    loop For each new/changed file
        RI->>TS: parse(content, language) [incremental]
        TS-->>RI: AST
        RI->>RI: extract_symbols(AST)
        RI->>TV: add_document(content, metadata)
        RI->>DB: insert/update file metadata
        RI->>DB: insert/update symbols
    end

    loop For each deleted file
        RI->>TV: delete_document(path)
        RI->>DB: DELETE WHERE path = ?
    end

    RI->>TV: commit_index()
    RI->>KH: extract_knowledge(index_results)
    KH->>KH: detect_patterns(index_results)
    KH->>DB: insert_knowledge(entries)
    RI->>EB: emit(RepositoryUpdated)
    RI-->>RI: IndexResult(statistics)
```

### Delta Detection

1. Query SQLite for all known file paths and hashes
2. Walk current filesystem
3. Compute three sets:
   - **New files**: in filesystem, not in database
   - **Changed files**: in both, but hash differs
   - **Deleted files**: in database, not in filesystem
4. Process each set accordingly

---

## Flow 2: Pre-Generation (BuildContext)

### Trigger

- Agent's pre-generation hook fires (e.g., `chat.message`, `UserPromptSubmit`)

### Sequence

```mermaid
sequenceDiagram
    participant Agent as Coding Agent
    participant AD as Adapter Layer
    participant CE as Context Engine
    participant RI as Repository Intelligence
    participant KH as Knowledge Hub
    participant SE as Skill Engine
    participant MR as Model Router
    participant TC as tiktoken-rs
    participant EB as Event Bus

    Agent->>AD: PreGeneration(message, session_id)
    AD->>AD: Validate request
    AD->>AD: Generate correlation ID

    AD->>CE: BuildContext(task)

    CE->>RI: search_code(query)
    RI->>TV: BM25 search + ripgrep
    TV-->>RI: ranked_results
    RI-->>CE: SearchResults

    CE->>KH: retrieve_knowledge(query)
    KH->>TV: BM25 search
    TV-->>KH: candidates
    KH-->>CE: Vec<KnowledgeEntry>

    CE->>SE: match_skills(task)
    SE->>SE: tag-based scoring
    SE-->>KH: Vec<SkillMatch>
    KH-->>CE: Vec<SkillMatch>

    CE->>TC: count_tokens(content)
    TC-->>CE: token_counts

    CE->>CE: Order: skills → docs → code
    CE->>CE: Apply frozen-prefix boundary
    CE->>CE: Deduplicate against fingerprint
    CE->>CE: Enforce token budget
    CE->>CE: Emit Context Pack as YAML

    CE->>MR: select_model(routing_request)
    MR->>MR: Heuristic scoring
    MR-->>CE: RoutingDecision

    CE->>EB: emit(ContextBuilt)
    MR->>EB: emit(ModelSelected)

    CE-->>AD: ContextPack + RoutingDecision
    AD-->>Agent: RewrittenMessage(with context)
```

---

## Flow 3: Pre-Tool (Tool Output Compression)

### Trigger

- Agent's pre-tool hook fires (e.g., `tool.execute.before`, `PreToolUse`)

### Sequence

```mermaid
sequenceDiagram
    participant Agent as Coding Agent
    participant AD as Adapter Layer
    participant EO as Execution Optimizer
    participant RTK as RTK Library
    participant TC as tiktoken-rs
    participant EB as Event Bus

    Agent->>AD: PreToolCall(tool_output)
    AD->>AD: Validate request
    AD->>AD: Generate correlation ID

    AD->>EO: compress_output(tool_output)

    EO->>TC: count_tokens(raw_output)
    TC-->>EO: original_token_count

    EO->>EO: detect_output_type(content)

    alt File Read
        EO->>EO: compress_file_content(content)
    else Search Result
        EO->>EO: compress_search_results(content)
    else Shell Output
        EO->>EO: compress_shell_output(content)
    end

    EO->>RTK: compress(compressed_content)
    RTK-->>EO: optimized_content

    alt RTK succeeded
        EO->>TC: count_tokens(optimized_content)
        TC-->>EO: compressed_token_count
        EO->>EB: emit(ToolExecuted)
        EO-->>AD: CompressedOutput
    else RTK failed
        EO->>EO: tee-on-failure: save full output to log
        EO-->>AD: OriginalPassthrough
    end

    AD-->>Agent: CompressedOutput or OriginalPassthrough
```

---

## Flow 4: Knowledge Retrieval

### Trigger

- Part of BuildContext pipeline (Flow 2)

### Sequence

```mermaid
sequenceDiagram
    participant CE as Context Engine
    participant KH as Knowledge Hub
    participant TV as BM25/Tantivy
    participant DB as SQLite
    participant ENG as engram

    CE->>KH: retrieve_knowledge(query, category_filter)
    KH->>TV: search(query, max_results=20)
    TV-->>KH: ranked_results

    KH->>DB: SELECT confidence WHERE id IN (...)
    DB-->>KH: confidence_scores

    KH->>KH: filter_by_confidence(min=0.3)
    KH->>KH: take(top_10)

    KH->>ENG: search(query)
    ENG-->>KH: memory_entries

    KH->>KH: merge(knowledge, memory)
    KH-->>CE: Vec<KnowledgeEntry>
```

### KnowledgeEntry Structure

```json
{
  "id": 42,
  "category": "convention",
  "key": "naming_functions",
  "value": "Functions use camelCase naming convention",
  "confidence": 0.95,
  "source": "detected from 15 function definitions",
  "relevance_score": 0.87
}
```

---

## Flow 5: Skill Selection

### Trigger

- Part of BuildContext pipeline (Flow 2)

### Sequence

```mermaid
sequenceDiagram
    participant CE as Context Engine
    participant KH as Knowledge Hub
    participant SE as Skill Engine

    CE->>KH: match_skills(task_description)
    KH->>SE: classify_task(task_description)
    SE->>SE: reuse Model Router signals

    loop For each skill in registry
        SE->>SE: compute_tag_overlap(skill.tags, task)
        SE->>SE: apply_category_bonus()
        SE->>SE: compute_final_score()
    end

    SE->>SE: sort_by_score_descending()
    SE->>SE: detect_conflicts()
    SE->>SE: resolve_priority()
    SE->>SE: filter(score > 0.3)
    SE->>SE: take(top_N)

    SE-->>KH: Vec<SkillMatch>
    KH-->>CE: Vec<SkillMatch>
```

### SkillMatch Structure

```json
{
  "skill_name": "Add Rate Limiting",
  "match_score": 0.85,
  "instructions": "1. Check existing middleware patterns...",
  "examples": ["src/middleware/rate_limit.rs"],
  "constraints": ["Do not modify authentication logic"]
}
```

---

## Flow 6: Context Construction (Cache-Aware)

### Trigger

- Part of BuildContext pipeline (Flow 2)
- Orchestrates Flows 4 and 5

### Sequence

```mermaid
flowchart TD
    A[Receive TaskRequest] --> B[Initialize token budget: 12000]
    B --> C[Search code: ~6600 tokens]
    C --> D[Retrieve knowledge: ~1800 tokens]
    D --> E[Match skills: ~2400 tokens]
    E --> F[Order for cache stability]

    F --> G[Section 1: behavioral_skills - 20%]
    G --> H[Section 2: docs_context - 15%]
    H --> I[Frozen-prefix boundary]
    I --> J[Section 3: code_context - 55%]

    J --> K[Deduplicate against session fingerprint]
    K --> L[Enforce token budget]
    L --> M[Emit Context Pack as YAML]

    style G fill:#e8f5e9
    style H fill:#f3e5f5
    style I fill:#fff3e0
    style J fill:#e1f5fe
```

### Cache-Aware Ordering Detail

```
Context Pack Structure (YAML):

┌─────────────────────────────────────────────────┐
│ behavioral_skills                20%  2,400 tok │
│ ████████████████████████████████████████         │
│ (Most cache-stable: byte-identical across tasks)│
├─────────────────────────────────────────────────┤
│ docs_context                       15%  1,800 tok│
│ ████████████████████████████                     │
│ (Moderately stable: changes rarely)              │
├───────── FROZEN-PREFIX BOUNDARY ────────────────┤
│ code_context                       55%  6,600 tok│
│ ████████████████████████████████████████         │
│ ████████████████████████████████████████         │
│ ████████████████████████████████████████         │
│ (Least stable: changes frequently)               │
└─────────────────────────────────────────────────┘
```

### Code File Selection

1. Search Repository Intelligence with task description
2. Get top 20 candidate files
3. Score each file by:
   - Text match relevance (0.0–1.0)
   - Structural relevance (imports, function calls) (0.0–1.0)
   - File proximity (same directory = higher) (0.0–1.0)
4. Sort by composite score
5. Add files to context until code budget is exhausted
6. For each file, truncate to `max_lines_per_file` if needed

---

## Flow 7: Model Routing

### Trigger

- Part of BuildContext pipeline (Flow 2)
- After context assembly is complete

### Sequence

```mermaid
sequenceDiagram
    participant CE as Context Engine
    participant MR as Model Router
    participant CFG as Configuration

    CE->>MR: select_model(RoutingRequest)
    MR->>CFG: get routing weights
    CFG-->>MR: weights

    MR->>MR: compute_structural_score(context)
    MR->>MR: compute_semantic_score(task_description)
    MR->>MR: compute_scope_score(context_size)

    MR->>MR: final_score = structural * 0.3 + semantic * 0.4 + scope * 0.3

    alt score < 0.3
        MR->>MR: tier = "fast"
    else score 0.3–0.7
        MR->>MR: tier = "balanced"
    else score > 0.7
        MR->>MR: tier = "capable"
    end

    MR->>CFG: get model_for_tier(tier)
    CFG-->>MR: model_name

    MR->>MR: build_reasoning()
    MR->>MR: emit(ModelSelected)

    MR-->>CE: RoutingDecision
```

### RoutingDecision Structure

```json
{
  "model": "gpt-4o",
  "tier": "balanced",
  "scores": {
    "structural": 0.5,
    "semantic": 0.6,
    "scope": 0.4,
    "final": 0.51
  },
  "reasoning": "Moderate complexity: middleware creation with clear task description and moderate context size"
}
```

---

## Flow 8: Memory Operations

### Read (In Hot Path)

```mermaid
sequenceDiagram
    participant CE as Context Engine
    participant KH as Knowledge Hub
    participant ENG as engram

    CE->>KH: retrieve_knowledge(query)
    KH->>ENG: search(query)
    ENG-->>KH: memory_entries
    KH->>KH: merge with knowledge entries
    KH-->>CE: Vec<KnowledgeEntry>
```

### Write (Agent-Invoked, Async)

```mermaid
sequenceDiagram
    participant Agent as Coding Agent
    participant AD as Adapter Layer
    participant KH as Knowledge Hub
    participant ENG as engram
    participant EB as Event Bus

    Agent->>AD: MemorySave(namespace, key, value)
    AD->>KH: memory_save(entry)
    KH->>ENG: save(entry)
    ENG-->>KH: confirmation
    KH->>EB: emit(MemorySaved)
    KH-->>AD: SaveResult
    AD-->>Agent: confirmation
```

---

## Flow 9: Event Bus (Async Observability)

### Sequence

```mermaid
sequenceDiagram
    participant CE as Context Engine
    participant MR as Model Router
    participant RI as Repository Intelligence
    participant EO as Execution Optimizer
    participant EB as Event Bus
    participant CLI as CLI Inspection
    participant MET as Metrics

    CE->>EB: emit(ContextBuilt {correlation_id, tokens, ...})
    MR->>EB: emit(ModelSelected {correlation_id, model, tier, ...})
    RI->>EB: emit(RepositoryUpdated {files_indexed, ...})
    EO->>EB: emit(ToolExecuted {tool_name, ratio, ...})

    EB->>CLI: dispatch(event)
    EB->>MET: dispatch(event)

    CLI->>CLI: Store in in-memory buffer
    MET->>MET: Aggregate metrics
```

---

## Flow 10: Fail-Open (Timeout/Error)

### Trigger

- BuildContext exceeds 30s timeout
- Any unrecoverable error in the pipeline

### Sequence

```mermaid
sequenceDiagram
    participant Agent as Coding Agent
    participant AD as Adapter Layer
    participant CE as Context Engine

    Agent->>AD: PreGeneration(message, session_id)
    AD->>AD: Validate request
    AD->>AD: Generate correlation ID

    AD->>CE: BuildContext(task)

    Note over CE: Processing...

    alt Timeout (> 30s)
        AD->>AD: Timeout fired
        AD->>AD: Log warning with correlation_id
        AD-->>Agent: OriginalPassthrough {reason: "timeout"}
    else Error
        CE-->>AD: Error
        AD->>AD: Log error with correlation_id
        AD-->>Agent: OriginalPassthrough {reason: "fail-open"}
    end

    Note over Agent: Agent continues with unmodified message
```

### Fail-Open Guarantees

| Condition | Response | Agent Impact |
|-----------|----------|--------------|
| BuildContext timeout | OriginalPassthrough | None — original message used |
| BuildContext error | OriginalPassthrough | None — original message used |
| Repository not indexed | OriginalPassthrough | None — original message used |
| LiteLLM unreachable | OriginalPassthrough | None — original message used |
| Any internal error | OriginalPassthrough | None — original message used |

The agent always gets a response. The runtime never blocks or breaks the agent.
