---
name: knocode
description: Use knocode (the local AI Runtime) via the installed binary or MCP tools for ranked, cross-file repository context before answering code questions; for literal single-string search use rg/grep first; run knocode init/doctor if the index is missing.
license: MIT
compatibility: opencode
metadata:
  audience: developers
  workflow: knocode-init
---

# Knocode

Run the local AI Runtime (`knocode`) correctly on any repository — fast, repo-scoped, no filesystem walks for the binary.

## When to use

- User asks to implement/fix/understand code across files, find relevant files/symbols for a task, or needs a context pack before answering — prefer `knocode` ranked context (default `candidate_k 100` / `max_files 20`, auto-tuned to `200/50` for repos with >5000 files)
- Need `class/interface/function` lookup via the symbol index before answering
- Repo has many files or a previous `init` timed out
- **Not** for a single literal `grep -n "exact string"` where `rg` is faster — use `rg`, then `knocode` only if ranking matters
- User says `knocode init`, `knocode index`, `knocode doctor`, `knocode serve`, `knocode preview`

## Binary location — never search

**Installed absolute (always):**

- Windows: `%USERPROFILE%\.knocode\bin\knocode.exe`
- Unix: `~/.knocode/bin/knocode`

This is where `scripts/install.ps1` / `scripts/install.sh` copy `target/release/knocode(.exe)` and add `~/.knocode/bin` to USER PATH.

**Rules:**

- Do **not** run `Get-ChildItem -Recurse -Filter knocode.exe`, `where knocode`, `find target/release`, or walk the repo looking for the binary — that times out or finds the dev checkout (`.../knocode/target/release/knocode.exe`) instead of the user's install.
- If `knocode --version` fails, the shell predates the PATH update — use the absolute path or restart the shell. `knocode doctor` has a `Knocode PATH:` probe for this.
- The daemon is repo-scoped via `repository_path`, but `init`/`index` must run in the target repo's `cwd`.

**Correct per-project usage:**

```powershell
cd <project-root>
# Windows
& "$env:USERPROFILE\.knocode\bin\knocode.exe" init
& "$env:USERPROFILE\.knocode\bin\knocode.exe" doctor
# Unix
~/.knocode/bin/knocode init
~/.knocode/bin/knocode doctor
# If PATH already includes ~/.knocode/bin (after restart), bare `knocode` also works:
knocode init
```

## `knocode init` — 7-phase bootstrap

Idempotent, safe to re-run:

```
[1/7] Scaffold (.knocode/, config, database)
[2/7] Repository discovery (languages, frameworks, commands)
[3/7] Downloading tree-sitter grammars
[4/7] Parser validation
[5/7] Indexing (full-text BM25 + symbol extraction)
[6/7] Knowledge Hub initialization
[7/7] Validation queries + repository profile
```

- `[5/7]` is the heavy step for large repos. Warm re-runs skip unchanged files (mtime+size).

**Tuning for large repos:**

```powershell
$env:KNOCODE_INDEX_THREADS=8; & "$env:USERPROFILE\.knocode\bin\knocode.exe" init   # more parallelism
$env:KNOCODE_SYMBOLS_ENABLED="false"; & "$env:USERPROFILE\.knocode\bin\knocode.exe" init  # BM25-only, faster
$env:KNOCODE_BUILD_GRAPH="1"; & "$env:USERPROFILE\.knocode\bin\knocode.exe" init   # force dependency graph during init (deferred by default for >5000 files)
$env:KNOCODE_CANDIDATE_K=200; $env:KNOCODE_MAX_FILES=50   # override retrieval pool / final files
```

## MCP — preferred for agents (no CLI shell conversions)

Two MCP surfaces, both conversion-free for the agent:

1. **Daemon-hosted MCP (opencode plugin)** — the opencode-knocode plugin drives its hooks through `POST /mcp` on the daemon (`http://127.0.0.1:9527/mcp`, JSON-RPC 2.0). Tools:
   - `knocode_context(prompt, repository_path?)` → enriched context answer + provenance
   - `knocode_compress(content, tool_name, output_type?, context?)` → compressed tool output
   - Readiness: `tools/call` returns `-32001 daemon_indexing` until the initial index completes.

2. **`knocode-mcp` stdio bridge (other agents: Codex, VS Code Copilot, Claude)** — `packages/knocode-mcp` relays stdio JSON-RPC to the same daemon `POST /mcp` surface, so those agents get the identical `knocode_context` / `knocode_compress` tools. There is no local tool registry — the surface cannot drift from the daemon’s. Daemon down answers a `-32000` error.

CLI `preview` is debug-only; agents should call MCP.

## Other commands

```bash
~/.knocode/bin/knocode index --watch          # re-index + watch (--watch-mode commit|filesystem)
~/.knocode/bin/knocode doctor                 # 8 probes — Knocode PATH:, SQLite, Tantivy, Retrieval
~/.knocode/bin/knocode preview "add auth" --candidate-k 200 --max-files 50   # debug-only
~/.knocode/bin/knocode serve                  # daemon on 127.0.0.1:9527 (UDS primary + HTTP fallback)
~/.knocode/bin/knocode status                 # daemon status + metrics
~/.knocode/bin/knocode config show            # effective configuration
```

Binaries unified to `~/.knocode/bin` (`knocode`, `knocode-daemon`, `rtk`). The daemon is readiness-gated: `GET /health` reports `state: "indexing" | "ready"` (and UDS clients can `Probe`); wait for `ready` before sending requests — the plugin does this automatically.

## Verification

```powershell
& "$env:USERPROFILE\.knocode\bin\knocode.exe" doctor   # want: Knocode PATH: ✓ OK
& "$env:USERPROFILE\.knocode\bin\knocode.exe" --version
# From target repo:
cd <project-root>; & "$env:USERPROFILE\.knocode\bin\knocode.exe" init
```

If `doctor` says `NOT on PATH` but `~\\.knocode\\bin\\knocode.exe` exists, restart the shell — USER PATH is only read at shell start.

## Troubleshooting

- **Timeout on `[5/7]`:** raise `KNOCODE_INDEX_THREADS`, or disable symbols (`KNOCODE_SYMBOLS_ENABLED=false`).
- **Found wrong exe (`.../knocode/target/release/...`):** you searched — use the installed absolute.
- **No index:** re-run `~/.knocode/bin/knocode init` inside the project.
- **Daemon not ready / 503:** indexing is still running — poll `GET /health` until `state: "ready"`.