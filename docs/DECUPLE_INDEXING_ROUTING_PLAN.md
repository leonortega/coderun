# Plan: Decouple Lexical Indexing, Auto-Configure Languages, Router Zero-Result Safeguard

## Problem Statement

Three related issues degrade retrieval quality and routing intelligence:

1. **Lexical indexing is coupled to AST parsing**: Files without tree-sitter grammar support (e.g. `.cs`, `.cshtml.cs`) are indexed into tantivy BM25 but symbol extraction falls back to regex. More critically, the *perception* is that unsupported languages get no indexing — but the real issue is that the code path conflates two concerns. The tantivy full-text index **does** index all text files, but the symbol enrichment (which feeds the dependency graph and structural search) is language-gated. A cleaner separation would make the system's behavior more predictable and observable.

2. **`coderun init` hardcodes 4 languages**: The `IndexConfig` default is `["rust", "typescript", "javascript", "python"]` regardless of what `discover_repository()` finds. If a repo is 90% C#, the config still says rust/ts/js/py. The discovery results are printed but never written back to config.

3. **Router treats zero retrieval results as simplicity**: When `search_fulltext` and `retrieve_knowledge` both return empty (e.g., for an unsupported language repo), the `select_model()` function computes `knowledge_entries=0`, `skills_matched=0`, `file_count=0` → scope score ≈ 0. This *lowers* the complexity score, routing to the cheapest model — the opposite of what should happen. Zero context is a signal of insufficient information, not simplicity.

---

## Fix 1: Decouple Lexical Indexing from AST Parsing

### Current Flow (in `index_repository()`, `coderun-repo-intel/src/lib.rs:189-383`)

```
for each file:
  detect_language(ext) → Option<String>
  if language.is_none() && !is_indexable_text_file(ext) → skip
  if is_likely_binary(path) → skip
  read content
  compute hash (incremental skip)
  db.upsert_file(path, hash, size, language)
  extract_symbols(content, patterns, language) → AST or regex
  db.insert_symbol(...) for each symbol
  tantivy.delete + add(path, content, language, symbols, repository_id)
```

### Problem

The flow is actually correct for indexing — tantivy gets all non-binary text files. The real issue is:
- Symbol extraction for unsupported languages uses regex patterns that are Rust/TS/JS/Py-centric (e.g. `fn name`, `class Name`). For C# (`OrderController.cs`), these patterns miss `public class OrderController : Controller`.
- The `extract_symbols()` function hardcodes a 4-language check (`if lang == "rust" || lang == "python" || ...`).

### Changes

**File: `crates/coderun-repo-intel/src/lib.rs`**

1. **Refactor `extract_symbols()` (line 811-919)**: Remove the hardcoded 4-language gate. Instead, always try tree-sitter first (via `parser::get_language()` which already returns `None` for unsupported languages), then always fall back to regex. The current code skips tree-sitter entirely for non-4 langs and goes straight to regex.

   ```rust
   // BEFORE (line 813-823):
   if let Some(lang) = language {
       if lang == "rust" || lang == "python" || lang == "javascript" || lang == "typescript" {
           let ast_symbols = parser::extract_symbols_ast(content, lang);
           return ast_symbols.into_iter()...
       }
   }

   // AFTER:
   if let Some(lang) = language {
       let ast_symbols = parser::extract_symbols_ast(content, lang);
       if !ast_symbols.is_empty() {
           return ast_symbols.into_iter()...
       }
   }
   // Fall through to regex for all languages (supported or not)
   ```

2. **Broaden regex `SymbolPatterns` (line 86-118)**: Add C#-compatible patterns to the fallback regex. Add patterns for:
   - C#: `public/private/protected/internal [static] class/struct/interface/enum/void/int/string Name`
   - General: `public/private [abstract] [static] class Name`

3. **Rename `index_repository` log message** (line 239): Change from "Indexing (tree-sitter symbols + tantivy BM25 + dependency graph)" to "Indexing (full-text BM25 + symbol extraction + dependency graph)" to reflect that BM25 is primary and symbol extraction is enrichment.

4. **Add `is_indexable_text_file()` extension for `.cshtml`, `.razor`, `.vb`** (line 741-748): These are text files that should be indexed even without language detection.

### Why This Works

- Tantivy full-text indexing already covers all text files (unchanged).
- Tree-sitter symbol extraction now attempts all languages transparently — if `get_language()` returns `None`, it returns empty, and regex picks up the slack.
- The regex patterns are broadened to catch more C#-style declarations.
- The net effect: a C# file gets BM25 indexing (already worked) + better regex-based symbols (improved) + broader text file coverage (improved).

---

## Fix 2: Auto-Configure Languages from Discovered Tech Stack

### Current Flow (in `cmd_init()`, `coderun-cli/src/main.rs:170-345`)

```
[2/6] discover_repository() → Discovery { languages: [...], frameworks: [...] }
  → prints languages
  → does NOT write to .coderun/config.toml

[3/6] index_repository() → uses hardcoded IndexConfig.languages = ["rust","ts","js","py"]
```

### Problem

The discovery knows the repo has C# files, but the config still says rust/ts/js/py. The `index_repository()` method doesn't directly use `IndexConfig.languages` for filtering (it indexes all non-binary files), but the config is misleading and the `doctor` command shows wrong languages.

### Changes

**File: `crates/coderun-cli/src/main.rs`**

1. **After discovery (line 208-218), write discovered languages to config**:

   ```rust
   // After printing discovery.languages...
   // Update .coderun/config.toml with discovered languages for tree-sitter awareness
   if !discovery.languages.is_empty() {
       let discovered_langs: Vec<String> = discovery.languages
           .iter()
           .map(|(l, _)| l.clone())
           .collect();
       update_config_languages(&config_path, &discovery.languages)?;
   }
   ```

2. **Add `update_config_languages()` helper**: Read the existing config TOML, update only the `[index].languages` field, write back. Use `toml::Value` for merge-safe updates so other config fields are preserved.

3. **Print the update**: Show which languages were written to config.

### Why This Works

- The discovered languages are now persisted in `.coderun/config.toml` under `[index]`.
- Future `coderun index` and daemon startup read this config, making the system self-documenting.
- The `doctor` command and `status` command now reflect actual repo languages.
- This closes the gap between discovery and configuration.

---

## Fix 3: Router Zero-Result Safeguard

### Current Flow (in `select_model()`, `coderun-router/src/lib.rs:75-145`)

```
structural = (file_count/20 + symbol_count/100) / 2
semantic = (word_count/50 + tech_count/5 + action_count/3) / 3
scope = (knowledge/10 + skills/3 + tokens/8000) / 3
final = structural * 0.3 + semantic * 0.4 + scope * 0.3
→ tier = fast/balanced/capable based on final_score
```

### Problem

When retrieval returns zero results:
- `file_count = 0` (code_context empty → 0 lines)
- `knowledge_entries = 0` (knowledge_context empty → 0 lines)
- `skills_matched = 0` (skills_context empty → 0 separator matches)
- `token_count ≈ 0`

This yields `structural ≈ 0`, `scope ≈ 0`. Even with a complex semantic message (score 0.5-0.7), the weighted average drops below 0.3 → **fast tier**. The model gets *less* capable when it has *less* context — exactly backwards.

### Changes

**File: `crates/coderun-router/src/lib.rs`**

1. **Add `retrieval_empty` field to `RoutingRequest`** (line 52-60):

   ```rust
   pub struct RoutingRequest {
       // ... existing fields ...
       /// True when both code search and knowledge retrieval returned zero results.
       /// This signals insufficient context, not simplicity — should escalate tier.
       pub retrieval_empty: bool,
   }
   ```

2. **In `select_model()`, apply zero-result floor** (after line 106):

   ```rust
   // Zero-result safeguard: when retrieval returns nothing, the agent is flying blind.
   // Floor the scope score at 0.9 to prevent routing to cheap models on empty context.
   // With scope_weight=0.3, this contributes ~0.27 to the final score — enough to push
   // even a simple semantic message (0.05-0.15) above the fast_threshold (0.3).
   let effective_scope = if request.retrieval_empty && scope < 0.9 {
       debug!("zero-result retrieval: floored scope from {:.2} to 0.9", scope);
       0.9
   } else {
       scope
   };
   let final_score = structural * self.config.structural_weight
       + semantic * self.config.semantic_weight
       + effective_scope * self.config.scope_weight;
   ```

3. **Add `retrieval_empty` to the `IModelGateway` trait impl** (line 209-230): Pass through from the trait method. Default to `false` for backward compatibility.

4. **Update `ContextEngine::select_model()`** (in `coderun-context/src/lib.rs:429-448`) to pass `retrieval_empty`:

   ```rust
   let retrieval_empty = code_context.is_empty() && knowledge_context.is_empty();
   let request = RoutingRequest {
       // ... existing fields ...
       retrieval_empty,
   };
   ```

5. **Update `RoutingDecision.reasoning`** to include the zero-result flag when active:

   ```rust
   let reasoning = if request.retrieval_empty && scope < 0.9 {
       format!("Structural: {:.2}, Semantic: {:.2}, Scope: {:.2} (zero-result floor 0.9 applied), Final: {:.2} → {}",
           structural, semantic, effective_scope, final_score, tier)
   } else {
       format!("Structural: {:.2}, Semantic: {:.2}, Scope: {:.2}, Final: {:.2} → {}",
           structural, semantic, scope, final_score, tier)
   };
   ```

### Why This Works

- When retrieval is empty, scope is floored to 0.9, which contributes `0.9 * 0.3 = 0.27` to the final score.
- A simple semantic message (score ~0.14) + floor (0.27) = `0.14 * 0.4 + 0.27 = 0.33` → **balanced tier** instead of fast.
- A complex semantic message (score ~0.6) + floor (0.27) = `0.6 * 0.4 + 0.27 = 0.51` → **balanced tier** (would have been fast without floor).
- The safeguard only activates when BOTH code AND knowledge are empty — if either has results, normal scoring applies.
- The reasoning string documents the override for observability.

---

## Files to Modify

| File | Change |
|------|--------|
| `crates/coderun-repo-intel/src/lib.rs` | Refactor `extract_symbols()`, broaden regex patterns, add text file extensions |
| `crates/coderun-cli/src/main.rs` | Add `update_config_languages()`, write discovered languages to config |
| `crates/coderun-router/src/lib.rs` | Add `retrieval_empty` field, zero-result floor logic |
| `crates/koderun-context/src/lib.rs` | Pass `retrieval_empty` to router |
| `crates/coderun-core/src/ipc.rs` | No change needed (RoutingRequest is in router crate) |

## Verification

1. **Fix 1**: `cargo test -p coderun-repo-intel` — existing tests pass, new test for C#-style regex patterns.
2. **Fix 2**: `cargo test -p coderun-cli` — existing tests pass, new test for `update_config_languages()`.
3. **Fix 3**: `cargo test -p coderun-router` — existing tests pass, new test for zero-result floor behavior.
4. **Integration**: `cargo check --workspace` compiles cleanly.

## Risk Assessment

- **Fix 1**: Low risk. Broadening regex patterns is additive. Tree-sitter path unchanged. Tantivy path unchanged.
- **Fix 2**: Low risk. Config file write is best-effort. Existing config preserved via TOML merge.
- **Fix 3**: Medium risk. Changes routing behavior for edge case (empty retrieval). Mitigated by: only floors scope, doesn't override semantic; reasoning string documents the override; existing tests with non-empty context are unaffected.
