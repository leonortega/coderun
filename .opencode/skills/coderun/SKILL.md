---
name: coderun
description: Run coderun init, index, doctor and serve reliably via the installed absolute binary in ~/.coderun/bin. Use when the user asks to initialize, index, or check coderun, or when any agent needs repository intelligence for 63k-file repos.
license: MIT
compatibility: opencode
metadata:
  audience: developers
  workflow: coderun-init
---

# Coderun

Run the local AI Runtime (`coderun`) correctly on any repository — fast, repo-scoped, no filesystem walks for the binary.

## When to use

- User says `coderun init`, `coderun index`, `coderun doctor`, `coderun serve`, `coderun preview`
- You need repository intelligence before answering (code search, context)
- The repo has many files (~50k+ / 63k) and a previous `init` timed out

## Binary location — never search

**Installed absolute (always, TASK-037):**

- Windows: `%USERPROFILE%\.coderun\bin\coderun.exe`
- Unix: `~/.coderun/bin/coderun`

This is where `scripts/install.ps1:346` (`$binDir`) and `scripts/install.sh:118` (`BIN_DIR`) copy `target/release/coderun(.exe)` and add `~/.coderun/bin` to USER PATH.

**Rules:**

- Do **not** run `Get-ChildItem -Recurse -Filter coderun.exe`, `where coderun`, `find target/release`, or walk 63k files. That times out or finds `C:\LeonRepository\coderun\target\release\coderun.exe` (the dev checkout) instead of the user's project.
- If `coderun --version` fails, the shell predates the PATH update — use the absolute path or restart the shell. `coderun doctor` has a `Coderun PATH:` probe for this.
- Daemon is repo-scoped via `repository_path` (`packages/opencode-coderun/src/index.ts:152`), but **`init`/`index` must run in the target repo's `cwd`**.

**Correct per-project usage:**

```powershell
cd <project-root>
# Windows
& "$env:USERPROFILE\.coderun\bin\coderun.exe" init
& "$env:USERPROFILE\.coderun\bin\coderun.exe" doctor
# Unix
~/.coderun/bin/coderun init
~/.coderun/bin/coderun doctor
# If PATH already includes ~/.coderun/bin (after restart), bare `coderun` also works:
coderun init
```

## `coderun init` — 8-phase bootstrap

Idempotent, safe to re-run (`crates/coderun-cli/src/main.rs:176`):

```
[1/8] Scaffold (.coderun/, config, skills, database)
[2/8] Repository discovery
[3/8] Download tree-sitter grammars
[4/8] Parser validation
[5/8] Indexing (full-text BM25 + symbol extraction)  # was BM25+symbols+graph — graph now deferred
[6/8] Knowledge Hub + skills
[7/8] Validation queries
[8/8] Repository status report
```

- `[5/8]` is the heavy step for 63k repos. Current optimizations (`docs/INDEXING_PERF_PLAN.md`): dedup symbol parse, SQLite `BEGIN IMMEDIATE` batch every 1000, tantivy heap 150 MB + intermediate commit, `WalkBuilder::threads(4)` (`CODERUN_INDEX_THREADS`), `is_binary_extension` + `is_content_binary` without extra `fs::read`, mtime+size skip for warm re-index, large-repo hint.
- **Graph deferred:** `build_dependency_graph()` is skipped during `[5/8]` when `files > 5000` unless `CODERUN_BUILD_GRAPH=1` (`cli/src/main.rs:310`). Lazy on first `build_context` query. Saves a second 63k walk.
- **Removed:** `codebase-memory-mcp` and `engram` — see `docs/01-architecture/ENGRAM_CBM_REMOVAL.md` (replaced by local AST+regex and SQLite+tantivy).

**Tuning for 63k:**

```powershell
$env:CODERUN_INDEX_THREADS=8; & "$env:USERPROFILE\.coderun\bin\coderun.exe" init   # more parallelism
$env:CODERUN_SYMBOLS_ENABLED="false"; & "$env:USERPROFILE\.coderun\bin\coderun.exe" init  # BM25-only ~20% faster
$env:CODERUN_BUILD_GRAPH="1"; & "$env:USERPROFILE\.coderun\bin\coderun.exe" init   # force graph during init if needed
```

## Other commands

```bash
~/.coderun/bin/coderun index          # re-index only (also deferred graph)
~/.coderun/bin/coderun doctor         # 9+ probes — check Coderun PATH:, SQLite, Tantivy, Retrieval
~/.coderun/bin/coderun preview "add auth" --session <id>
~/.coderun/bin/coderun serve          # daemon already auto-started by installer from ~\.coderun\bin\coderun-daemon.exe
~/.coderun/bin/coderun config show
~/.coderun/bin/coderun skills list
```

All binaries unified to `~/.coderun/bin` (`coderun`, `coderun-daemon`, `rtk`). `engram` and `codebase-memory-mcp` removed.

## Verification

```powershell
& "$env:USERPROFILE\.coderun\bin\coderun.exe" doctor   # want: Coderun PATH: ✓ OK
& "$env:USERPROFILE\.coderun\bin\coderun.exe" --version
# From target repo:
cd <project-root>; & "$env:USERPROFILE\.coderun\bin\coderun.exe" init
```

If `doctor` says `NOT on PATH` but `~\.coderun\bin\coderun.exe` exists, restart the shell — USER PATH in HKCU is only read at shell start.

## Troubleshooting

- **Timeout on `[5/8]`:** see `docs/INDEXING_PERF_PLAN.md` — increase `CODERUN_INDEX_THREADS`, or disable symbols.
- **Found wrong exe (`.../coderun/target/release/...`):** you searched — use the installed absolute.
- **No index:** re-run `~/.coderun/bin/coderun init` inside the project.
