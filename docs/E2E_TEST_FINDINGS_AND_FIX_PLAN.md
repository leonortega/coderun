# E2E Test Findings & Fix Plan — eShopOnWeb (2026-08-25)

> First end-to-end validation of the v0.6.0 runtime against a foreign repository
> (`C:\LeonRepository\eShopOnWeb`, ASP.NET Core/C#) using the exact hook payloads the
> `opencode-coderun` plugin sends. Daemon: `target/release/coderun-daemon.exe` on
> `http://127.0.0.1:9527`. Repo indexed via `coderun init && coderun index`
> (324 files, 20 dep edges, 643 ms).

## How to reproduce

```bash
# health
curl http://127.0.0.1:9527/health          # {"status":"ok","version":"0.4.0"}

# PreGeneration (prompt enrichment) — same shape as packages/opencode-coderun
curl -X POST http://127.0.0.1:9527/hook -H "Content-Type: application/json" -d '{
  "correlation_id": "test_pg_001",
  "hook_type": "PreGeneration",
  "payload": {"type": "MessageRewrite", "session_id": "test-session",
              "message": "How is the order checkout flow implemented in this shop?",
              "context_hints": {"files_mentioned": ["src/Web"], "language": "csharp"}}
}'

# PreToolCall (output compression)
# payload: {"type":"ToolOutput","tool_name":"bash","output_type":"text","content":"<300 log lines>"}

curl http://127.0.0.1:9527/metrics         # before/after diff of coderun_* counters
```

## Results summary

| Probe | Result |
|---|---|
| `GET /health`, `GET /metrics` | PASS |
| PreGeneration returns `RewrittenMessage` | PASS (structure correct) |
| BM25 ranking for repo-specific queries | PASS (`BasketViewModelService.cs`, `CatalogContext.cs`, `Checkout.cshtml.cs` ranked top) |
| PreToolCall compression | PASS — 8999 → 3005 tokens (**−67 %**), sub-ms latency |
| Fail-open / rate limiting paths | PASS (`coderun_fail_open_total 0`) |
| Enrichment serves the *requested* repo | **FAIL — F-1, F-7** |
| Injected context actually carries content | **FAIL — F-2** |
| Provenance hygiene | **FAIL — F-3, F-4** |
| Metrics truthfulness | **FAIL — F-5** |

## Findings (evidence-backed)

### F-1 (P0): Retrieval is not repository-scoped — global index contamination
One shared Tantivy dir serves every repo: `crates/coderun-repo-intel/src/lib.rs:753`
(`default_index_path()` → `~/.coderun/index`). `search_fulltext` (`lib.rs:459`) applies no
repo filter, and Knowledge Hub docs are ingested globally too. Consequence: for the generic
query above, ALL provenance hits came from the **coderun repo's own docs**
(`docs/coderun_v1_review_and_tasks.md`, `DATA_FLOW.md`, `GLOSSARY.md`) instead of
eShopOnWeb. `AgentRequest.repository_id` exists (computed in
`crates/coderun-daemon/src/http_server.rs:280`, TASK-021) but is **never passed into**
`ContextEngine::build_context` nor used at query time. Restarting the daemon with a
different CWD changes nothing (index is global).

### F-2 (P0): Rewritten message adds metadata-only overhead
With empty sections the pack still emits a full YAML skeleton
(`code_context: ''`, `total_tokens: 0`, `budget_remaining: 12000`, duplicated provenance
list ≈ 2 KB) and the plugin replaces the user prompt with it
(`packages/opencode-coderun/README.md`: MessageRewrite semantics). Net effect today:
every prompt pays ~500–700 tokens for zero retrievable value when hits are weak.

### F-3 (P1): Duplicate provenance entries
Identical `path+source+retriever+score` rows appear 4×. Root cause chain:
MkDocs ingestion re-inserts on every `index` run without a stable upsert key
(`crates/coderun-repo-intel/src/lib.rs:308-326`) → duplicates accumulate in the knowledge
store → `retrieve_knowledge` returns them all
(`crates/coderun-knowledge/src/lib.rs:130-179`) → provenance push loop
(`crates/coderun-context/src/lib.rs:136-146`) copies them verbatim.

### F-4 (P1): Path rendering garbage in provenance
Knowledge keys embed a collection prefix plus a Windows verbatim prefix:
`docs:\\?\C:\LeonRepository\coderun\docs\...`. The `\\?\` is never stripped
(no `dunce::simplify`), and the `docs:` category leaks into what provenance renders as a
path (`crates/coderun-context/src/lib.rs:140-145` maps `entry.key` straight to `path`).

### F-5 (P1): Metrics lie
- `coderun_tokens_saved_total` is exported (`crates/coderun-daemon/src/metrics.rs:111`)
  but has **no increment call-site anywhere** — stays 0 despite measured −67 % compression.
- `coderun_index_files` reported `1`→`2` while indexing 300+ files.

### F-6 (P2): No E2E regression net
The two hook contracts have no automated test that asserts compression > 0, provenance
uniqueness, or repo scoping — regressions ship silently.

### F-7 (P0): Repository identity comes from process CWD — not from the agent's workspace
Product requirement (user-clarified during testing): when a user asks for "coderun init"
they mean *set up coderun itself* — the runtime must then always operate on the **same
repository the coding agent (opencode) is currently working in**, regardless of where the
daemon was started. Today the opposite happens:
- `crates/coderun-daemon/src/http_server.rs:280-286` derives `repository_id` from the
  **daemon's own** `std::env::current_dir()` at request time.
- Hook payloads (`HttpRequestPayload`, `http_server.rs:30-44`) carry **no workspace path**
  from the agent, so the daemon cannot know which project opencode has open.
- Net: one global daemon silently serves whichever folder it happened to start in.

## Fix plan

### P0 — must-fix before plugin is useful cross-repo

- [x] **TASK-030: repository-scoped retrieval (fixes F-1)**
  - `crates/coderun-storage/src/tantivy_index.rs`: add `repository_id` (stored+indexed
    text field) to the schema; bump index version/migrate (or accept one-time rebuild).
  - `crates/coderun-repo-intel/src/lib.rs:273,321`: stamp docs with
    `repository_id = hash(repo_path)[..12]` at upsert time (same hash formula as
    `http_server.rs:280`; extract into `coderun-core` so both sides share it).
  - `crates/coderun-repo-intel/src/lib.rs:459` `search_fulltext`: accept
    `Option<&str> repository_id` and add a TermQuery filter.
  - Knowledge Hub: add `repository_id` column to knowledge records +
    `db.search_knowledge(...)` filter; MkDocs/codebase ingestion stamps it.
  - `crates/coderun-context/src/lib.rs:90,94,242,276`: thread
    `request.repository_id` into `search_code_scored` / `retrieve_knowledge_scored`.
  - Accept: with daemon CWD anywhere, the pg_001 payload returns ONLY eShopOnWeb paths;
    a second repo indexed afterwards cannot leak into results.

- [x] **TASK-031: no-value rewrite suppression (fixes F-2)**
  - `crates/coderun-context/src/lib.rs` (`assemble_context_pack` / YAML formatter):
    omit empty sections entirely; emit compact single-line metadata only when
    `total_tokens > 0`.
  - `crates/coderun-daemon/src/adapter.rs` (`handle_pre_generation`): if all three
    content sources are empty → return `OriginalPassthrough { reason: "no_context_hits" }`
    instead of `RewrittenMessage`.
  - Accept: prompt with zero hits is byte-identical to input; with hits, appended block
    contains actual snippet/knowledge content, ≤ budget.

- [x] **TASK-036: per-request repository resolution — follow opencode's workspace (fixes F-7)**
  - `packages/opencode-coderun/src/index.ts`: include the agent's active workspace root in
    BOTH hook payloads (new `repository_path` field on `MessageRewrite` + `ToolOutput`;
    plugin resolves it from the opencode project/session directory).
  - `crates/coderun-core/src/ipc.rs`: extend `HttpRequestPayload` / `RequestPayload` with
    optional `repository_path: Option<String>`.
  - `crates/coderun-daemon/src/http_server.rs:280`: derive `repository_id` by hashing
    `payload.repository_path` when present; fall back to daemon CWD only for direct API
    callers (curl, tests). Same hash formula as TASK-030's shared helper in `coderun-core`.
  - CLI unchanged for standalone use (`coderun init` / `index` keep CWD semantics) —
    but document that daemon-side scoping always wins over startup CWD.
  - Accept: ONE daemon instance, started anywhere, correctly serves two opencode windows
    open on different repos simultaneously (eShopOnWeb prompts → eShopOnWeb context;
    coderun-repo prompts → coderun context), verified via provenance paths.

### P1 — correctness & observability

- [x] **TASK-032: idempotent ingestion + provenance dedup (fixes F-3)**
  - `crates/coderun-repo-intel/src/lib.rs:308-326`: upsert knowledge docs on
    `(category, key, repository_id)` unique constraint instead of blind insert.
  - One-shot cleanup migration: collapse duplicate `(category,key)` rows keeping max
    confidence.
  - `crates/coderun-context/src/lib.rs:124-163`: dedup provenance pushes by
    `(path, source, retriever)` keeping highest score.
  - Accept: pg_001-style response shows each doc once; re-running `coderun index` N times
    does not grow the knowledge table.

- [x] **TASK-033: clean provenance paths (fixes F-4)**
  - Strip verbatim prefix via `dunce::simplify` (add dep to `coderun-context`) before
    pushing provenance; keep category in the existing `source` field rather than the
    path string.
  - Accept: provenance paths are plain absolute paths
    (`C:\LeonRepository\eShopOnWeb\src\Web\Pages\Basket\Checkout.cshtml.cs`).

- [x] **TASK-034: honest metrics (fixes F-5)**
  - Add `Metrics::inc_tokens_saved(u64)` in `crates/coderun-daemon/src/metrics.rs`; call
    it from the ToolOutput branch of `handle_hook` /
    `crates/coderun-optimizer` using `original_tokens - compressed_tokens`.
  - `coderun_index_files`: set gauge from the SQLite file-count after indexing completes
    (not a constant).
  - Accept: after the compression repro above,
    `coderun_tokens_saved_total ≥ 5900`; `coderun_index_files ≥ 300`.

- [x] **TASK-037: installer ships `coderun.exe` to the user `.coderun` folder**
  - `scripts/install.ps1`: copy prebuilt CLI to `%USERPROFILE%\.coderun\bin\coderun.exe`
    (alongside the existing `~\.coderun\models` layout) and persist that dir on the **user
    PATH** (HKCU Environment via `[Environment]::SetEnvironmentVariable(..., 'User')` +
    `$env:Path` for the current session), so `coderun --version` resolves from any shell
    without the repo checkout.
  - `scripts/install.sh`: same for `~/.coderun/bin/coderun` + PATH line in
    `~/.profile`/`~/.bashrc` (idempotent append).
  - Copy `coderun-daemon.exe` alongside it — installer step 4 should then launch the
    daemon from `~\.coderun\bin`, not `target\release`, so the runtime keeps working if
    the repo is moved/cleaned (`cargo clean`, `-RemoveRepo`).
  - `scripts/uninstall.ps1` / `uninstall.sh`: remove `~/.coderun/bin/` binaries + revert
    PATH entry.
  - Accept: fresh machine, repo deleted → `coderun status` still works from any directory;
    re-running installer is idempotent (no duplicate PATH entries).

### P2 — hardening

- [x] **TASK-035: E2E regression tests (fixes F-6)**
  - Rust integration test (`crates/coderun-daemon/tests/`): boot daemon on ephemeral port
    in temp repo → POST both hook payloads → assert
    (a) compression ratio > 1.2×, (b) provenance unique, (c) zero cross-repo leakage with
    a second seeded repo, (d) passthrough on empty-hit prompt.
  - `packages/opencode-coderun/test/`: extend vitest suite to mock daemon responses
    matching the schemas in `crates/coderun-daemon/src/http_server.rs:22-59`.
  - Accept: `cargo test` + `npm test` green in CI; failures block release.

- [x] **TASK-038: per-repo artifact home — `.coderun/` inside the analyzed repository**
  - Convention (user-mandated): any GENERATED output for an analyzed repo (mkdocs site
    build of the target's `docs/**/*.md`, retrieval/context reports, exported context
    packs) must be written to `<analyzed-repo>/.coderun/artifacts/` — NEVER back into the
    coderun source repository.
  - `crates/coderun-cli/src/main.rs` (`cmd_index` / docs-site build path): resolve the
    artifact root as `<repo_path>/.coderun/artifacts/<name>/`; create on demand
    (`coderun init` already owns `.coderun/`).
  - Keep read-only analysis (index DBs, tantivy global store) where it is today; this task
    only relocates *rendered/generated deliverables*.
  - Do not auto-edit analyzed repos' `.gitignore`; instead print a hint suggesting
    `.coderun/artifacts/` as an ignore candidate.
  - Update `mkdocs.yml` deploy comment + docs: `docs-site/build` applies to the coderun
    repo's own site only; per-target builds land in the target's `.coderun/artifacts/`.
  - Accept: running index + docs build against eShopOnWeb creates
    `C:\LeonRepository\eShopOnWeb\.coderun\artifacts\...` and zero new files under
    `C:\LeonRepository\coderun`.

## Verification checklist (post-fix rerun)

1. `coderun index` in eShopOnWeb, daemon started from any CWD.
2. Repro queries: generic ("order checkout flow") AND repo-specific ("CatalogItem BrandType").
   Both must return eShopOnWeb-only provenance, no duplicate rows, clean absolute paths.
3. Empty-hit prompt ("zzzqqq unrelated") must pass through untouched.
4. Compression repro: `tokens_saved_total` reflects real savings.
5. `curl /metrics` sane after full run.
6. TASK-036: opencode session in eShopOnWeb + second session in coderun repo, one shared
   daemon — each prompt's provenance resolves to its own workspace (F-7 closed).
7. TASK-037: after install (repo folder renamed away), `coderun status` and the running
   daemon both still work via `%USERPROFILE%\.coderun\bin`.
8. TASK-038: docs build against eShopOnWeb writes only to
   `eShopOnWeb\.coderun\artifacts\` — `git -C coderun status` stays clean.
