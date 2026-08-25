#!/usr/bin/env bash
# Coderun uninstaller (Unix: Linux/macOS, bash)
# V1 scope: local runtime only — reverses scripts/install.sh — removes everything by default (strict) but preserves future/workflow source unless --remove-repo.
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

info "Coderun uninstaller (DBOS/workflow future-only)"
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
for sock in "$HOME/.coderun/coderun.sock" "/tmp/coderun.sock" ".coderun/coderun.sock" "/tmp/coderun.sock.lock"; do
  # Use absolute for check but log relative for repo file
  check_sock="$sock"
  [ "$sock" = ".coderun/coderun.sock" ] && check_sock="$ROOT/.coderun/coderun.sock"
  if [ -e "$check_sock" ]; then
    if $DRY_RUN; then skip "would rm $sock"; else rm -f "$check_sock" && ok "removed socket $sock" || warn "failed $sock"; fi
  fi
done

# 1b. TASK-037: remove installed binaries (~/.coderun/bin) + revert PATH entries.
# Always executed: PATH is shell state, independent of --keep-data/--remove-repo.
info "Removing installed coderun binaries from ~/.coderun/bin..."
for bin in "$HOME/.coderun/bin/coderun" "$HOME/.coderun/bin/coderun-daemon"; do
  if [ -e "$bin" ]; then if $DRY_RUN; then skip "would rm $bin"; else rm -f "$bin" && ok "removed $bin"; fi; else skip "not found $bin"; fi
done
if [ -d "$HOME/.coderun/bin" ] && [ -z "$(ls -A "$HOME/.coderun/bin" 2>/dev/null)" ]; then
  if $DRY_RUN; then skip "would rmdir ~/.coderun/bin"; else rmdir "$HOME/.coderun/bin" 2>/dev/null && ok "removed empty ~/.coderun/bin/"; fi
fi
# Revert PATH: drop our marker block from ~/.profile and ~/.bashrc (idempotent)
for rc in "$HOME/.profile" "$HOME/.bashrc"; do
  if [ -f "$rc" ] && grep -qs "CODERUN_BIN_PATH" "$rc"; then
    if $DRY_RUN; then skip "would remove coderun PATH lines from $rc"
    else
      sed -i '/# CODERUN_BIN_PATH/d; /\.coderun\/bin/d' "$rc" 2>/dev/null && ok "removed coderun PATH entry from $rc" || warn "failed to edit $rc"
    fi
  else skip "no coderun PATH entry in $rc"; fi
done

# 2. Build artifacts (use .opencode/.coderun relative, not absolute repo)
if $KEEP_BUILD; then info "Skipping build artifact removal (--keep-build)";
else
  info "Removing build artifacts..."
  for bin in "target/release/coderun" "target/release/coderun-daemon" "target/debug/coderun" "target/debug/coderun-daemon"; do
    if [ -e "$ROOT/$bin" ]; then if $DRY_RUN; then skip "would rm $bin"; else rm -f "$ROOT/$bin" && ok "removed $bin"; fi; else skip "not found $bin"; fi
  done
  if $DO_REMOVE_DATA && [ -d "$ROOT/target" ]; then
    if $DRY_RUN; then skip "would rm -rf target/"; else rm -rf "$ROOT/target" && ok "removed target/ (cargo clean)"; fi
  else skip "keeping target/ cache (--keep-data)"; fi
  for p in "workflow/dbos/node_modules" "workflow/dbos/package-lock.json" "future/workflow/dbos/node_modules" "future/workflow/dbos/package-lock.json" "future/workflow/dbos/dist"; do
    if [ -e "$ROOT/$p" ]; then if $DRY_RUN; then skip "would rm -rf $p"; else rm -rf "$ROOT/$p" && ok "removed $p (v1 workflow future-only)"; fi; else skip "not found $p"; fi
  done
  # v1: never delete future/workflow source (.ts, Cargo.toml) unless --remove-data forced
  if [ -d "$ROOT/future/workflow" ]; then skip "preserving future/workflow source (use --remove-data + rm -rf future/workflow if needed)"; fi
fi

# 3. Plugins (use .opencode folder, never absolute repo) - plugin 'coderun'
info "Removing opencode plugins..."
for pp in ".opencode/plugins/coderun.ts" "$HOME/.config/opencode/plugins/coderun.ts"; do
  check_pp="$pp"
  [ "$pp" = ".opencode/plugins/coderun.ts" ] && check_pp="$ROOT/.opencode/plugins/coderun.ts"
  if [ -e "$check_pp" ]; then if $DRY_RUN; then skip "would rm plugin 'coderun'"; else rm -f "$check_pp" && ok "removed plugin 'coderun'"; fi; else skip "not found plugin 'coderun'"; fi
done
# Also clean portable engram copies: GLOBAL ~/.config/opencode/engram (current install) + legacy .opencode/engram
for pp in "$HOME/.config/opencode/engram/engram" "$HOME/.config/opencode/engram/engram.exe"; do
  if [ -e "$pp" ]; then if $DRY_RUN; then skip "would rm $pp"; else rm -f "$pp" && ok "removed $pp (global engram copy)"; fi; fi
done
if [ -d "$HOME/.config/opencode/engram" ] && [ -z "$(ls -A "$HOME/.config/opencode/engram" 2>/dev/null)" ]; then
  if $DRY_RUN; then skip "would rmdir ~/.config/opencode/engram"; else rmdir "$HOME/.config/opencode/engram" 2>/dev/null && ok "removed empty ~/.config/opencode/engram/"; fi
fi
for pp in ".opencode/engram/engram" ".opencode/engram/engram.exe"; do
  check_pp="$ROOT/$pp"
  if [ -e "$check_pp" ]; then if $DRY_RUN; then skip "would rm $pp"; else rm -f "$check_pp" && ok "removed $pp (portable engram)"; fi; fi
done
if [ -d "$ROOT/.opencode/engram" ] && [ -z "$(ls -A "$ROOT/.opencode/engram" 2>/dev/null)" ]; then
  if $DRY_RUN; then skip "would rmdir .opencode/engram"; else rmdir "$ROOT/.opencode/engram" 2>/dev/null && ok "removed empty .opencode/engram/"; fi
fi

# 3b. Opencode npm plugin — installed into GLOBAL ~/.config/opencode/node_modules (and legacy .opencode/node_modules)
info "Removing opencode npm plugin (opencode-coderun)..."
for pp in "$HOME/.config/opencode/node_modules/opencode-coderun" "$HOME/.config/opencode/node_modules/@opencode-ai" "$HOME/.config/opencode/package-lock.json" "$HOME/.cache/opencode/node_modules/opencode-coderun" ".opencode/node_modules/opencode-coderun" ".opencode/node_modules/@opencode-ai" ".opencode/package-lock.json"; do
  case "$pp" in /*) check_pp="$pp" ;; *) check_pp="$ROOT/$pp" ;; esac
  if [ -e "$check_pp" ]; then if $DRY_RUN; then skip "would rm -rf $pp"; else rm -rf "$check_pp" && ok "removed $pp (opencode npm plugin)"; fi; else skip "not found $pp"; fi
done
# Clean package.json deps: GLOBAL ~/.config/opencode/package.json + legacy .opencode/package.json — remove opencode-coderun dep or delete if only that
for pj in "$HOME/.config/opencode/package.json" "$ROOT/.opencode/package.json"; do
  if [ -f "$pj" ]; then
    if $DRY_RUN; then skip "would clean $pj (remove opencode-coderun dep)"; else
      if command -v node >/dev/null 2>&1; then
        PKG_JSON_PATH="$pj" node -e "const fs=require('fs');const p=process.env.PKG_JSON_PATH;try{let j=JSON.parse(fs.readFileSync(p,'utf8'));let changed=false;if(j.dependencies&&j.dependencies['opencode-coderun']){delete j.dependencies['opencode-coderun'];changed=true}if(j.dependencies&&Object.keys(j.dependencies).length===0){fs.unlinkSync(p);console.log('removed empty '+p)}else if(changed){fs.writeFileSync(p,JSON.stringify(j,null,2));console.log('cleaned opencode-coderun from '+p)} }catch(e){}" 2>/dev/null && ok "cleaned $pj" || warn "failed to clean $pj"
      else
        # fallback: remove file if it only contains opencode-coderun
        if grep -q "opencode-coderun" "$pj" 2>/dev/null && [ "$(wc -l < "$pj")" -lt 10 ]; then rm -f "$pj" && ok "removed $pj"; fi
      fi
    fi
  else skip "not found $pj"; fi
done
# Remove empty .opencode dir if only empty after plugin removal (keep if has other config)
if [ -d "$ROOT/.opencode" ] && [ -z "$(ls -A "$ROOT/.opencode" 2>/dev/null)" ]; then
  if $DRY_RUN; then skip "would rmdir .opencode (empty)"; else rmdir "$ROOT/.opencode" 2>/dev/null && ok "removed empty .opencode/"; fi
else skip "keeping .opencode/ (has opencode.jsonc or other config)"; fi
# Clean opencode.jsonc plugin + mcp entries (always, even without --RemoveRepo — so plugin not showing after uninstall)
for cfg in "$ROOT/.opencode/opencode.jsonc" "$ROOT/.opencode/opencode.json" "$HOME/.config/opencode/opencode.jsonc" "$HOME/.config/opencode/opencode.json"; do
  # repo configs always cleaned; global only if DO_REMOVE_DATA
  is_global=false; case "$cfg" in *".config/opencode"*) is_global=true;; esac
  if $is_global && ! $DO_REMOVE_DATA; then skip "keeping global $cfg (use --remove-data to clean)"; continue; fi
  if [ -f "$cfg" ]; then
    if $DRY_RUN; then skip "would clean coderun plugin/mcp from $cfg"; else
      if command -v node >/dev/null 2>&1; then
        node -e "
const fs=require('fs');const p='$cfg';
try{
  let raw=fs.readFileSync(p,'utf8');
  // strip comments for jsonc
  let stripped=raw.replace(/\/\/.*$/gm,'').replace(/\/\*[\s\S]*?\*\//g,'');
  stripped=stripped.replace(/,\s*([\}\]])/g,'\$1');
  let j=JSON.parse(stripped);
  let changed=false;
  if(Array.isArray(j.plugin)){
    const orig=j.plugin.length;
    j.plugin=j.plugin.filter(x=>x!=='opencode-coderun' && x!=='coderun');
    if(j.plugin.length!==orig) changed=true;
    if(j.plugin.length===0) delete j.plugin;
  }
  if(j.mcp && typeof j.mcp==='object'){
    let m=j.mcp;
    let had=false;
    for(const k of ['codebase-memory','engram','codebase-memory-mcp']){
      if(k in m){ delete m[k]; had=true; }
    }
    if(had) changed=true;
    if(Object.keys(m).length===0) delete j.mcp;
  }
  // if file now only has \$schema or empty, remove it; else write back
  const keys=Object.keys(j).filter(k=>k!=='\$schema');
  if(keys.length===0){
    fs.unlinkSync(p); console.log('removed empty '+p);
  } else if(changed){
    // preserve \$schema if existed
    if(raw.includes('\"\$schema\"') && !('\$schema' in j)) j['\$schema']='https://opencode.ai/config.json';
    fs.writeFileSync(p, JSON.stringify(j,null,2));
    console.log('cleaned coderun plugin/mcp from '+p);
  }
}catch(e){ console.error(e.message); process.exit(1); }
" 2>/dev/null && ok "cleaned coderun plugin/mcp from $cfg" || warn "failed to clean $cfg"
      else
        # fallback sed: remove plugin line and mcp blocks
        if grep -q "opencode-coderun" "$cfg" 2>/dev/null; then
          sed -i '/opencode-coderun/d' "$cfg" 2>/dev/null && ok "cleaned opencode-coderun from $cfg (sed)" || true
        fi
      fi
    fi
  fi
done
# Also check global npm plugin (if ever published)
if command -v npm >/dev/null 2>&1; then
  if npm list -g opencode-coderun >/dev/null 2>&1; then if $DRY_RUN; then skip "would npm uninstall -g opencode-coderun"; else npm uninstall -g opencode-coderun 2>/dev/null && ok "uninstalled opencode-coderun (npm -g)" || warn "npm uninstall opencode-coderun failed"; fi; else skip "opencode-coderun not installed globally (npm -g)"; fi
fi

# 4. External tools (default: remove)
if ! $DO_REMOVE_EXTERNAL; then info "Skipping external tools (--keep-external)";
else
  info "Removing external tools (strict default)..."
  # ast-grep: npm @ast-grep/cli (current) or legacy cargo install
  if command -v npm >/dev/null 2>&1 && npm list -g @ast-grep/cli >/dev/null 2>&1; then if $DRY_RUN; then skip "would npm uninstall -g @ast-grep/cli"; else npm uninstall -g @ast-grep/cli 2>/dev/null && ok "uninstalled @ast-grep/cli (npm -g)" || warn "@ast-grep/cli uninstall failed"; fi; else skip "@ast-grep/cli not installed (npm -g)"; fi
  if command -v sg >/dev/null 2>&1 || command -v ast-grep >/dev/null 2>&1; then if $DRY_RUN; then skip "would cargo uninstall ast-grep (legacy)"; else cargo uninstall ast-grep 2>/dev/null && ok "uninstalled ast-grep (legacy cargo)" || warn "ast-grep cargo uninstall failed"; fi; else skip "legacy cargo ast-grep not installed"; fi
  # rtk: user-bin prebuilt copy (current) + legacy cargo install
  if [ -f "$HOME/bin/rtk" ]; then if $DRY_RUN; then skip "would rm ~/bin/rtk"; else rm -f "$HOME/bin/rtk" && ok "removed ~/bin/rtk"; fi; else skip "~/bin/rtk not found"; fi
  if command -v rtk >/dev/null 2>&1; then if $DRY_RUN; then skip "would cargo uninstall rtk (legacy)"; else cargo uninstall rtk 2>/dev/null && ok "uninstalled rtk (legacy cargo)" || warn "rtk cargo uninstall failed"; fi; else skip "legacy cargo rtk not installed"; fi
  if command -v npm >/dev/null 2>&1; then
    for pkg in codebase-memory-mcp promptfoo eslint; do
      if npm list -g "$pkg" >/dev/null 2>&1; then if $DRY_RUN; then skip "would npm uninstall -g $pkg"; else npm uninstall -g "$pkg" 2>/dev/null && ok "uninstalled $pkg (npm -g)"; fi; else skip "$pkg not installed"; fi
    done
  fi
  if [ -d "$ROOT/../engram" ]; then if $DRY_RUN; then skip "would rm -rf ../engram"; else rm -rf "$ROOT/../engram" && ok "removed engram clone at ../engram"; fi; else skip "engram clone not found at ../engram"; fi
  if [ -f "$HOME/.coderun/models/flashrank.onnx" ]; then if $DRY_RUN; then skip "would rm ~/.coderun/models/flashrank.onnx"; else rm -f "$HOME/.coderun/models/flashrank.onnx" && ok "removed FlashRank model at ~/.coderun/models/flashrank.onnx"; fi; else skip "FlashRank model not found"; fi
  if command -v pip3 >/dev/null 2>&1; then
    for pkg in litellm mkdocs mkdocs-material pymdown-extensions; do
      if pip3 show "$pkg" >/dev/null 2>&1; then if $DRY_RUN; then skip "would pip3 uninstall -y $pkg"; else pip3 uninstall -y "$pkg" 2>/dev/null && ok "uninstalled $pkg (pip)"; fi; else skip "$pkg not installed"; fi
    done
  fi
  if command -v rustup >/dev/null 2>&1; then
    if $DRY_RUN; then skip "would rustup component remove clippy"; else rustup component remove clippy 2>/dev/null && ok "removed rustup component clippy" || warn "clippy remove failed"; fi
    skip "keeping rustup toolchain (never uninstall rustup)"
  else skip "rustup not installed"; fi
fi

# 5. Data (default: remove) - use .coderun relative
if ! $DO_REMOVE_DATA; then
  info "Skipping data removal (--keep-data)"
  info "  Kept: ~/.coderun and .coderun/"
else
  info "Removing data (strict default)..."
  for d in "$HOME/.coderun" "$ROOT/.coderun"; do
    disp="$d"
    [ "$d" = "$ROOT/.coderun" ] && disp=".coderun/"
    if [ -d "$d" ] || [ -f "$d" ]; then if $DRY_RUN; then skip "would rm -rf $disp"; else rm -rf "$d" && ok "removed $disp"; fi; else skip "not found $disp"; fi
  done
fi

info "Uninstall complete."
! $DO_REMOVE_EXTERNAL && info "  External tools were kept (--keep-external)" || info "  External tools removed"
! $DO_REMOVE_DATA && info "  Data was kept (--keep-data)" || info "  Data removed"
$KEEP_BUILD && info "  Build artifacts were kept (--keep-build)" || info "  Build artifacts removed"
info "To reinstall: bash scripts/install.sh"
