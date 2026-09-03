# V1 Fix Plan 0.8.1 — DefinitelyTyped 53k Diagnosis (candidate_k 100)

> Based on `Knocode 0.8.0` on `DefinitelyTyped-master` `53825 files` `crates/knocode-context/src/lib.rs:67 candidate_k=100` `config.rs:72`, `lifecycle.rs:104 candidate_k: config.context.candidate_k`, `397s re-index (KNOCODE_INDEX_THREADS=8)`, `Tantivy 53842 docs / Symbols 194989 vs 37370` (before fix), `rg 1001ms vs preview 5561ms (0/20)`, `cached ≈ cold`, `BM25 ~4200ms`, `Precision K 50/100/200 via KNOCODE_CANDIDATE_K=200` `crates/knocode-storage/src/tantivy_index.rs:715` → `rg P 0.01 R 0.2 (NextFunction R1) vs knocode P 0 R 0` diag `LEXICAL_MISS/RANKED_TOO_LOW`.

## 1. Observed

- **Build** `cargo build --release 11s` passes, `doctor 31/33 parsers` ok.
- **Index:** `53842 docs` ≈ files, but `194989 symbols` >> `37370` before candidate_k fix — split/extract blow-up `crates/knocode-storage/src/tantivy_index.rs:205,471` `crates/knocode-repo-intel/src/parser.rs` / `lib.rs:275` (tree-sitter pack 111 langs vs regex fallback, `KNOCODE_SYMBOLS_ENABLED`). Index size drives 4.2s search.
- **Speed:** `bench.ps1:14` `bench-cached.ps1:1` `rg 1s` wins 20/20 (Select-String fallback false 19/20 `5419 vs 4654ms`). Cached 5435ms ≈ cold 5596ms — dedup `crates/knocode-context/src/lib.rs:499` not latency win. Amortization `133-397s / 0.8s ≈170-500 queries` confirms `V1_PLAN.md:1` don't compete with `rg`.
- **Precision:** `bench-precision.ps1:14` `K 50→200` (`KNOCODE_CANDIDATE_K=200`) no change: `knocode 0/10` at both K, `0/2 below top-20` because `max_files 20` `crates/knocode-core/src/config.rs:69` `crates/knocode-context/src/lib.rs:71` truncates after ranking; `LEXICAL_MISS` tokens `find/type/NextFunction` absent, `RANKED_TOO_LOW` below top-20. `rg` needs `K 50` to hit `express/index.d.ts:99` ( `-l` alphabetical).

## 2. Root Causes

1. **Lexical miss before K matters:** `sanitize_code_query` `crates/knocode-storage/src/tantivy_index.rs:332` drops `find`/`type` via `STOP_WORDS` `tantivy_index.rs:119` (correct) but should keep `NextFunction → next/function/nextfunction` via `split_pascal_case` `tantivy_index.rs:138`. If `symbol_name` `STRING` vs `TEXT` or split dilution (`preprocess_code_content` `tantivy_index.rs:205` expands every content token) breaks `symbol_name 3.0×`/`path 2.5×` `tantivy_index.rs:621`, recall stays 0 regardless of `candidate_k`.
2. **Index bloat:** `194k symbols` + splits → large `content_field` `TEXT|STORED` `tantivy_index.rs:491` + `symbols_text` `tantivy_index.rs:471` → `150MB` heap writer `tantivy_index.rs:411`, `MmapDirectory` reload per commit `crates/knocode-storage/src/tantivy_index.rs:9,419` (`open_cached`/`cached_reader`) still `~4.2s` materialization `tantivy_index.rs:740` even though content not fetched (writer cost remains).
3. **K vs max_files conflated:** `fetch_limit = candidateK` `tantivy_index.rs:715` but `results.len() >= max_results` `tantivy_index.rs:728` + `ContextConfig.max_files 20` caps final Context Pack. `candidate_k 100→200` alone can't surface `rank 30` if `max_files=20`.

## 3. Changes (plan, no code yet)

### P0 — Fix correctness (prove with single query before sweeping K)

- **Log tokens:** `KNOCODE_PROFILE=1` add `sanitized` log `tantivy_index.rs:657` and `tokens` for `"NextFunction"` vs `"find type NextFunction"`; verify `rg -n NextFunction DefinitelyTyped --type ts | head` hits exist and `symbol_name` contains `nextfunction`.
- **Field test:** `cargo run -p knocode-cli -- preview "NextFunction" --candidate-k 100` should hit `symbol_name:nextfunction` high. If not, try `symbol_name` as `STRING` exact + lower split field separate (`symbol_name_exact` `STRING 3.5×` vs `symbol_name_split` `TEXT 1.5×`) to avoid idf dilution.
- **Idempotent check:** re-index with `KNOCODE_SYMBOLS_ENABLED=1` vs `0` vs `tree-sitter-language-pack` version to explain `37k→194k` jump; keep extractor deterministic (tree-sitter primary `parser.rs`, regex fallback `lib.rs:275`).

### P0 — Latency <1s on 53k

- Keep `open_cached` `lib.rs:419` already `0ms` (`tomm` open_cached) — confirm on 53k not just 531-file test (`KNOCODE_PROFILE` `code_search.tantivy_open_cached` `crates/knocode-repo-intel/src/lib.rs:632`, `tantivy.search` `tantivy_index.rs:722`).
- Make `content_field` `TEXT` not `STORED` (lazy `RepositoryIntelligence::get_file_content` `crates/knocode-context/src/lib.rs:359` for Top 20); keep `raw_path` `STRING|STORED` for delete exact `tantivy_index.rs:517`.
- Gate `build_dependency_graph` `crates/knocode-context/src/lib.rs:324` already gated for `doc_count>5000` — verify `doc_count` from `validate_index` `lib.rs:148` is 53842 so graph skips on 53k (currently 687ms on 531-file test when not gated).

### P1 — Make K vs max_files both sweepable

- Expose both via `Config.context` `crates/knocode-core/src/config.rs:65` + `ContextConfig` `crates/knocode-context/src/lib.rs:60` + CLI `crates/knocode-cli/src/main.rs:68` `preview --candidate-k` / `--max-files` (or env `KNOCODE_MAX_FILES`), default `candidate_k 100` `max_files 20` but eval on 53k uses `--candidate-k 50 --max-files 50` to match rg `K 50` hit.
- Change `TantivyIndex::search` `tantivy_index.rs:715,728` `fetch_limit = min(candidateK,200)` and `results` cap = `candidateK` (not `max_files`); `ContextEngine` `lib.rs:191` then `merged.sort` → `take(max_files)` deterministic ranking `lib.rs:284`.

### P1 — Benchmark harness

- Keep `bench.ps1` vs `rg` as sanity, but primary is `eval/metrics/retrieval.py` `Recall@5` on `eval/datasets/repository_tasks.yaml` with `--candidate-k 20/50/100/200 --max-files 20/50` matrix; log `retrieval_diagnostic` `crates/knocode-core/src/ipc.rs` `LEXICAL_MISS` vs `RANKED_TOO_LOW`.
- Document `0/20 rg wins` expected per `V1_MINIMAL_STACK_PLAN.md:2` — value is `Context Pack` (`BuildContext` `crates/knocode-context/src/lib.rs:426` `FROZEN PREFIX END` dedup) not raw grep.

### Deferred (not 0.8.1)

- `RRF / embeddings / vector DB / custom reranker / LLM expansion` stay out per `V1_MINIMAL_STACK_PLAN.md:2.7-2.10`.
- `MkDocs` plain markdown only `lib.rs:378` removed; `LiteLLM`/`RTK` optional `-WithOptional` `scripts/install.ps1:24`.
