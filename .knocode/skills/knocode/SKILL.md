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

**Transparent AI Runtime** — the plugin intercepts your messages and tool outputs automatically. You receive enriched context without calling knocode explicitly.

Run the local AI Runtime (`knocode`) correctly on any repository — fast, repo-scoped, no filesystem walks for the binary.

## When to use

**Automatic (via plugin):**
- Every user message is enriched with relevant code context automatically
- Every tool output (read, grep, bash) is compressed automatically
- You don't need to call knocode — the plugin handles it

**Manual CLI (when needed):**
- User says `knocode init` — run in terminal to initialize/rebuild index
- User says `knocode doctor` — run in terminal to diagnose issues

**Not for:**
- Single literal `grep -n "exact string"` where `rg` is faster — use `rg` directly

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

## CLI commands

### `knocode init`

Initialize or rebuild the repository index. Safe to re-run.

```bash
# Run from project root
cd <project-root>
knocode init
```

**When to run:**
- First time using knocode on a project
- User says `knocode init` or `knocode index`
- Index is missing or corrupted

### `knocode doctor`

Check knocode installation and diagnose issues.

```bash
knocode doctor
```

**When to run:**
- User says `knocode doctor`
- knocode is not working as expected
- Verify installation is correct

### Other commands

```bash
knocode --version              # Check version
knocode status                 # Daemon status (if running)
```

## How knocode works with the agent

The opencode-knocode plugin intercepts messages **transparently** — you don't need to call knocode explicitly. The plugin handles everything:

### Automatic context enrichment

When a user sends a message, the plugin:
1. Calls `knocode_context` via MCP to find relevant code
2. Enriches the message with context (file paths, code snippets, provenance)
3. You receive the enriched prompt — use the context to give better answers

**Example:** User asks "implement auth middleware" → plugin finds `auth.rs`, `middleware.rs`, related tests → you receive enriched prompt with relevant code context.

### Automatic output compression

When you use tools (read, grep, bash), the plugin:
1. Calls `knocode_compress` via MCP to reduce output size
2. Removes redundancy while preserving essential information
3. You receive compressed output — saves tokens, keeps signal

### What you should do

- **Use the enriched context** — the plugin already found relevant code for the user's task
- **Reference file paths** from the context when explaining solutions
- **Don't search for files manually** if context is already provided
- **Still use tools** (read, grep, bash) for specific investigation — outputs will be compressed automatically







## Troubleshooting

- **knocode not found:** Run `knocode doctor` to check installation
- **Index missing:** Run `knocode init` from project root
- **Slow initialization:** Large repos may take a few minutes on first init