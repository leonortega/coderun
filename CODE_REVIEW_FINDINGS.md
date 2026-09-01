# Code Review Findings — Coderun Repository

**Date:** 2026-08-31  
**Reviewer:** Buffy (Codebuff AI)

---

## 1. Duplicated Files (Exact Copies)

### 1.1 Benchmark Files — `benches/` vs `crates/coderun-bench/benches/`

The root-level `benches/` directory contains **identical copies** of files in `crates/coderun-bench/benches/`:

| Root (leftover) | Active location | Status |
|---|---|---|
| `benches/context_bench.rs` | `crates/coderun-bench/benches/context_bench.rs` | **Exact duplicate** |
| `benches/rtk_bench.rs` | `crates/coderun-bench/benches/rtk_bench.rs` | **Exact duplicate** |

**Recommendation:** Delete the root `benches/` directory. Only `crates/coderun-bench/` is a workspace member.

### 1.2 Workflow Code — `future/workflow/` vs `crates/coderun-workflow/`

The `future/workflow/src/` directory is a **complete copy** of `crates/coderun-workflow/src/`:

| File | `future/workflow/` | `crates/coderun-workflow/` | Status |
|---|---|---|---|
| `lib.rs` | ✅ | ✅ | **Identical** |
| `dbos.rs` | ✅ | ✅ | **Identical** |
| `types.rs` | ✅ | ✅ | **Identical** |

`future/workflow/` is excluded from the workspace in `Cargo.toml` (`exclude = ["crates/coderun-workflow", "future/workflow"]`), so it's dead code.

**Recommendation:** Delete `future/workflow/` entirely. If it's a "future" prototype, the active code lives in `crates/coderun-workflow/`.

---

## 2. Hardcoded User Path (Bug)

**File:** `scripts/uninstall.ps1` line ~280  
**Code:**
```powershell
$hardcodedGlobalPlugin = "C:\Users\marce\.config\opencode\plugins\coderun.ts"
```

This is a **hardcoded developer username** (`marce`) embedded in the uninstall script. It will fail on any other user's machine.

**Recommendation:** Replace with `$env:USERPROFILE\.config\opencode\plugins\coderun.ts` (which is already `$pluginGlobal` defined earlier in the same block, making this line fully redundant).

---

## 3. Dead Code / Unused Items

### 3.1 `#[allow(dead_code)]` items that should be cleaned up

| Location | Item | Notes |
|---|---|---|
| `coderun-context/src/lib.rs:15` | `STOP_WORDS` constant | Only used in `classify_misses()` (inline copy, not this constant). The constant is truly dead. |
| `coderun-context/src/lib.rs:37` | `is_valid_file_path()` | Not called anywhere in production code. |
| `coderun-repo-intel/src/lib.rs:39` | `import_pattern` field | Unused struct field. |
| `coderun-repo-intel/src/lib.rs:117` | `file_hashes` field | Unused struct field. |
| `coderun-knowledge/src/lib.rs:33` | `config` field | Unused struct field. |
| `coderun-daemon/src/adapter.rs:20-24` | `socket_path`, `max_concurrent` | Unused struct fields. |
| `coderun-daemon/src/adapter.rs:203` | `shutdown()` method | Never called. |
| `coderun-storage/src/tantivy_index.rs:395` | `invalidate_reader_cache()` | Empty no-op method. |
| `coderun-storage/src/tantivy_index.rs:562` | `expand_query_with_symbols()` | Never called. |
| `coderun-daemon/src/lifecycle.rs:21` | `db`, `event_bus` fields | Marked dead_code but part of `DaemonState` — likely intentional for future use. |

### 3.2 Legacy alias

**File:** `coderun-context/src/lib.rs`  
```rust
pub fn estimate_tokens(text: &str) -> usize {
    count_tokens(text)
}
```
This is a legacy alias. Should be deprecated with `#[deprecated]` or removed if no external callers exist.

---

## 4. Duplicated Stop Words List

The stop words list is defined in **two places** and they're independent copies:

1. **`coderun-context/src/lib.rs:15`** — `STOP_WORDS` constant (dead, never used)
2. **`coderun-context/src/lib.rs:923`** — Inline `stop_words` variable inside `classify_misses()`

The inline copy is the one actually used. The `STOP_WORDS` constant is dead code.

**Recommendation:** Either use the constant in `classify_misses()`, or delete the constant.

---

## 5. Duplicated Test Helpers

The following test utilities are copy-pasted across test files:

| Helper | Files |
|---|---|
| `Database::open(&PathBuf::from(":memory:"))` | 18 occurrences across 7 crates |
| `RepositoryIntelligence::new(path, db, event_bus)` boilerplate | 12+ occurrences |
| `temp_dir()` / `test_repo_path()` | `e2e_hooks.rs`, `integration_tests.rs` |
| `find_file_with_ext()` / `find_files_with_ext()` / `filename_matches()` | `integration_tests.rs` only (but good candidate for shared test utils) |

**Recommendation:** Extract shared test helpers into a `test-utils` module or a shared test fixture crate.

---

## 6. Debug `eprintln!` in Production Code

There are **78+ `eprintln!` calls** across production (non-test) Rust code, used for profiling and diagnostics:

| Location | Count | Notes |
|---|---|---|
| `coderun-context/src/lib.rs` | 8 | `prof()` + explain logging |
| `coderun-storage/src/tantivy_index.rs` | 2 | `[profile]` logging |
| `coderun-repo-intel/src/lib.rs` | 2 | `[profile]` logging |
| `coderun-cli/src/main.rs` | 2 | `println!` for banner |
| `coderun-daemon/src/lifecycle.rs` | 8 | Banner `println!` |
| `coderun-daemon/src/main.rs` | 4 | Error `eprintln!` |
| `coderun-core/src/ipc.rs` | 12 | Retrieval diagnostic display |

Some are gated behind `CODERUN_PROFILE` env var (good), but many are unconditional.

**Recommendation:** 
- Keep `prof()` and `[profile]` lines gated behind `CODERUN_PROFILE`
- Convert unconditional `eprintln!` to `tracing::debug!` / `tracing::warn!` for structured logging
- The banner `println!` in `lifecycle.rs` is fine for startup, but should use `tracing::info!`

---

## 7. `unwrap()` in Non-Test Production Code

There are **189 `unwrap()` calls** across the codebase. Most are in test code (acceptable), but notable production-code instances:

| Location | Risk |
|---|---|
| `coderun-events/src/lib.rs:103,125,136,152` | Mutex lock `.unwrap()` — will panic if poisoned |
| `coderun-daemon/src/ratelimit.rs:32` | Mutex lock `.unwrap()` — same risk |
| `coderun-storage/src/tantivy_index.rs:185` | `.pop().unwrap()` on Vec — could panic on empty |
| `coderun-repo-intel/src/graph.rs:180-184` | Regex compilation `.unwrap()` — will panic on bad pattern |

The regex ones are `lazy_static` / `LazyLock`-style patterns (acceptable for known-valid regex). The mutex ones are the main concern.

**Recommendation:** Replace `.lock().unwrap()` with `.lock().map_err(|e| ...)` or use `tracing::error!` on poison.

---

## 8. Inconsistent Error Handling Scripts

The install/uninstall scripts use different patterns:

| Script | Error Handling |
|---|---|
| `install.ps1` | `$ErrorActionPreference = "Stop"` + try/catch |
| `uninstall.ps1` | `$ErrorActionPreference = "Continue"` + ShouldProcess |
| `compile.ps1` | `$ErrorActionPreference = "Stop"` + explicit $LASTEXITCODE checks |
| `uninstall.sh` | `set -euo pipefail` + manual error checks |

The `uninstall.sh` has **syntax issues** (see below).

---

## 9. `uninstall.sh` Has Broken Syntax

**File:** `scripts/uninstall.sh`  
Lines ~130-140 contain malformed code:

```bash
done
fi
  check_pp="$ROOT/$pp"
done
fi
  if [ -e "$pp" ]; then ...
```

These orphaned `done`/`fi` statements suggest incomplete editing. The script may not run correctly on Linux/macOS.

**Recommendation:** Review and fix the plugin removal section of `uninstall.sh`.

---

## 10. Stale/Leftover Files

| File/Directory | Issue |
|---|---|
| `bench-precision.ps1`, `bench.ps1` | Referenced in repo stats but not present (may be gitignored or deleted) |
| `bench-*.csv`, `bench-*.log` | Same — listed in repo stats but not in working tree |
| `eval/results/` | Created by benchmark scripts at runtime, should be in `.gitignore` |
| `experiments/ast-grep-tree-sitter-interop/` | May be stale experiment code |
| `future/workflow/` | Dead duplicate (see §1.2) |

---

## 11. Workspace Configuration Issue

**File:** `Cargo.toml`

```toml
exclude = ["crates/coderun-workflow", "future/workflow"]
```

Both `coderun-workflow` and `future/workflow` are excluded from the workspace. The `future/workflow` copy is dead code (see §1.2), but `crates/coderun-workflow` being excluded means it's **not built or tested** as part of the workspace.

**Recommendation:** Either include `crates/coderun-workflow` in the workspace, or document why it's intentionally excluded.

---

## Summary — Priority Actions

| Priority | Issue | Impact |
|---|---|---|
| 🔴 High | `uninstall.sh` broken syntax | Linux/macOS uninstall is broken |
| 🔴 High | Hardcoded `C:\Users\marce\` path | Fails on other users' machines |
| 🟡 Medium | Delete `future/workflow/` duplicate | Reduce confusion |
| 🟡 Medium | Delete root `benches/` duplicate | Reduce confusion |
| 🟡 Medium | Remove dead `STOP_WORDS` constant | Reduce dead code |
| 🟡 Medium | Clean up `#[allow(dead_code)]` items | Improve code quality |
| 🟢 Low | Replace `eprintln!` with `tracing` | Structured logging |
| 🟢 Low | Extract test helper utilities | Reduce duplication |
| 🟢 Low | Replace `.unwrap()` in production mutex locks | Panic safety |
| 🟢 Low | Deprecate `estimate_tokens` alias | API cleanup |
