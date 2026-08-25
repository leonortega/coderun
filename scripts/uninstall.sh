#!/usr/bin/env bash
# Coderun v0.6.0 uninstaller (Unix: Linux/macOS, bash)
# Reverses scripts/install.sh — removes everything by default (strict).
# Idempotent. Usage: bash scripts/uninstall.sh [--keep-external] [--keep-data] [--keep-build] [--force] [--dry-run]
# Default: remove binaries, plugins, ALL external tools and ALL data (prompts unless --force).
# Use --keep-* to preserve. Legacy --remove-external/--remove-data still accepted (now default).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KEEP_EXTERNAL=false; KEEP_DATA=false; KEEP_BUILD=false; FORCE=false; DRY_RUN=false
REMOVE_EXTERNAL=false; REMOVE_DATA=false
for arg in "$@"; do case "$arg" in
  --keep-external) KEEP_EXTERNAL=true ;;
  --keep-data) KEEP_DATA=true ;;
  --keep-build) KEEP_BUILD=true ;;
  --remove-external) REMOVE_EXTERNAL=true ;;
  --remove-data) REMOVE_DATA=true ;;
  --force) FORCE=true ;;
  --dry-run|--whatif) DRY_RUN=true ;;
  -h|--help) echo "Usage: $0 [--keep-external] [--keep-data] [--keep-build] [--force] [--dry-run]"; echo "  Default: remove everything (strict). Use --keep-* to preserve."; exit 0 ;;
esac; done
# Default is to remove everything unless --keep-* is set (legacy --remove-* also triggers)
if [ "$KEEP_EXTERNAL" = false ]; then REMOVE_EXTERNAL=true; fi
if [ "$KEEP_DATA" = false ]; then REMOVE_DATA=true; fi
if [ "$REMOVE_EXTERNAL" = true ]; then KEEP_EXTERNAL=false; fi
if [ "$REMOVE_DATA" = true ]; then KEEP_DATA=false; fi
# Re-derive effective flags
DO_REMOVE_EXTERNAL=true; [ "$KEEP_EXTERNAL" = true ] && DO_REMOVE_EXTERNAL=false
DO_REMOVE_DATA=true; [ "$KEEP_DATA" = true ] && DO_REMOVE_DATA=false

info(){ echo -e "\033[36m[coderun]\033[0m $*"; }
ok(){ echo -e "  \033[32m[OK]\033[0m $*"; }
warn(){ echo -e "  \033[33m[WARN]\033[0m $*"; }
skip(){ echo -e "  \033[90m[SKIP]\033[0m $*"; }
run(){ if $DRY_RUN; then skip "would $*"; else eval "$*"; fi; }

info "Coderun v0.6.0 uninstaller - $ROOT"
$DRY_RUN && warn "DryRun active - no changes will be made"
info "Options: RemoveExternal(effective=$DO_REMOVE_EXTERNAL KeepExternal=$KEEP_EXTERNAL) RemoveData(effective=$DO_REMOVE_DATA KeepData=$KEEP_DATA) KeepBuild=$KEEP_BUILD Force=$FORCE"

if $DO_REMOVE_DATA && ! $FORCE && ! $DRY_RUN; then
  read -p "This will permanently delete ~/.coderun and .coderun/ (project config). Continue? [y/N] " ans
  case "$ans" in y|Y|yes|YES) ;; *) info "Aborted."; exit 0;; esac
fi

# 1. Stop daemon / socket
info "Stopping daemon and cleaning socket..."
for p in coderun-daemon coderun; do
  if pgrep -x "$p" >/dev/null 2>&1; then
    if $DRY_RUN; then skip "would pkill $p"; else pkill -TERM "$p" 2>/dev/null || true; ok "stopped $p"; fi
  else skip "no running $p process"; fi
done
for sock in "$HOME/.coderun/coderun.sock" "/tmp/coderun.sock" "$ROOT/.coderun/coderun.sock" "/tmp/coderun.sock.lock"; do
  if [ -e "$sock" ]; then
    if $DRY_RUN; then skip "would rm $sock"; else rm -f "$sock" && ok "removed socket $sock" || warn "failed $sock"; fi
  fi
done

# 2. Build artifacts
if $KEEP_BUILD; then info "Skipping build artifact removal (--keep-build)";
else
  info "Removing build artifacts..."
  for bin in "$ROOT/target/release/coderun" "$ROOT/target/release/coderun-daemon" "$ROOT/target/debug/coderun" "$ROOT/target/debug/coderun-daemon"; do
    if [ -e "$bin" ]; then if $DRY_RUN; then skip "would rm $bin"; else rm -f "$bin" && ok "removed $bin"; fi; else skip "not found $bin"; fi
  done
  if $DO_REMOVE_DATA && [ -d "$ROOT/target" ]; then
    if $DRY_RUN; then skip "would rm -rf $ROOT/target"; else rm -rf "$ROOT/target" && ok "removed target/ (cargo clean)"; fi
  else skip "keeping target/ cache (--keep-data)"; fi
  for p in "$ROOT/workflow/dbos/node_modules" "$ROOT/workflow/dbos/package-lock.json"; do
    if [ -e "$p" ]; then if $DRY_RUN; then skip "would rm -rf $p"; else rm -rf "$p" && ok "removed $p"; fi; else skip "not found $p"; fi
  done
fi

# 3. Plugins
info "Removing opencode plugins..."
for pp in "$ROOT/.opencode/plugins/coderun.ts" "$HOME/.config/opencode/plugins/coderun.ts"; do
  if [ -e "$pp" ]; then if $DRY_RUN; then skip "would rm $pp"; else rm -f "$pp" && ok "removed plugin $pp"; fi; else skip "not found $pp"; fi
done

# 4. External tools (default: remove)
if ! $DO_REMOVE_EXTERNAL; then info "Skipping external tools (--keep-external)";
else
  info "Removing external tools (strict default)..."
  if command -v sg >/dev/null 2>&1; then if $DRY_RUN; then skip "would cargo uninstall ast-grep"; else cargo uninstall ast-grep 2>/dev/null && ok "uninstalled ast-grep" || warn "ast-grep uninstall failed"; fi; else skip "ast-grep not installed"; fi
  if command -v rtk >/dev/null 2>&1; then if $DRY_RUN; then skip "would cargo uninstall rtk"; else cargo uninstall rtk 2>/dev/null && ok "uninstalled rtk" || warn "rtk uninstall failed"; fi; else skip "rtk not installed"; fi
  if command -v npm >/dev/null 2>&1; then
    for pkg in codebase-memory-mcp promptfoo eslint; do
      if npm list -g "$pkg" >/dev/null 2>&1; then if $DRY_RUN; then skip "would npm uninstall -g $pkg"; else npm uninstall -g "$pkg" 2>/dev/null && ok "uninstalled $pkg (npm -g)"; fi; else skip "$pkg not installed"; fi
    done
  fi
  if [ -d "$ROOT/../engram" ]; then if $DRY_RUN; then skip "would rm -rf $ROOT/../engram"; else rm -rf "$ROOT/../engram" && ok "removed engram clone"; fi; else skip "engram clone not found"; fi
  if [ -f "$HOME/.coderun/models/flashrank.onnx" ]; then if $DRY_RUN; then skip "would rm $HOME/.coderun/models/flashrank.onnx"; else rm -f "$HOME/.coderun/models/flashrank.onnx" && ok "removed FlashRank model"; fi; else skip "FlashRank model not found"; fi
  if command -v pip3 >/dev/null 2>&1; then
    for pkg in litellm mkdocs mkdocs-material pymdown-extensions; do
      if pip3 show "$pkg" >/dev/null 2>&1; then if $DRY_RUN; then skip "would pip3 uninstall -y $pkg"; else pip3 uninstall -y "$pkg" 2>/dev/null && ok "uninstalled $pkg (pip)"; fi; else skip "$pkg not installed"; fi
    done
  fi
fi

# 5. Data (default: remove)
if ! $DO_REMOVE_DATA; then
  info "Skipping data removal (--keep-data)"
  info "  Kept: ~/.coderun and $ROOT/.coderun"
else
  info "Removing data (strict default)..."
  for d in "$HOME/.coderun" "$ROOT/.coderun"; do
    if [ -d "$d" ] || [ -f "$d" ]; then if $DRY_RUN; then skip "would rm -rf $d"; else rm -rf "$d" && ok "removed $d"; fi; else skip "not found $d"; fi
  done
fi

info "Uninstall complete."
! $DO_REMOVE_EXTERNAL && info "  External tools were kept (--keep-external)" || info "  External tools removed"
! $DO_REMOVE_DATA && info "  Data was kept (--keep-data)" || info "  Data removed"
$KEEP_BUILD && info "  Build artifacts were kept (--keep-build)" || info "  Build artifacts removed"
info "To reinstall: bash scripts/install.sh"
