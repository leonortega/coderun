#!/usr/bin/env bash
# Coderun first-class installer (Unix: Linux/macOS, bash)
# V1 scope: local AI runtime only (DBOS/workflows in future/workflow, opt-in CODERUN_WORKFLOW_ENABLED=true)
# Tools are FIRST-CLASS (no optional except LSP, no Temporal) + uses prebuilt coderun (no compile/test).
# Idempotent. Usage: bash scripts/install.sh [--skip-build] [--skip-external]
# Note: --skip-build is deprecated (build always skipped, prebuilt at target/release/coderun is used)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SKIP_BUILD=false; SKIP_EXTERNAL=false
for arg in "$@"; do case "$arg" in --skip-build) SKIP_BUILD=true;; --skip-external) SKIP_EXTERNAL=true;; esac; done
info(){ echo -e "\033[36m[coderun]\033[0m $*"; } ; ok(){ echo -e "  \033[32m[OK]\033[0m $*"; } ; warn(){ echo -e "  \033[33m[WARN]\033[0m $*"; }

info "Coderun installer (DBOS/workflow future-only)"

# 0a. Stop any running daemon/CLI up front - later steps REPLACE binaries (~/.coderun/bin)
# and a locked exe would fail the copy. The fresh daemon is restarted at the end (step 4).
for p in coderun-daemon coderun; do
  if pgrep -x "$p" >/dev/null 2>&1; then pkill -x "$p" 2>/dev/null || true; ok "stopped $p"; fi
done

# Rust 1.85+ required for RTK (edition2024)
NEED_RUST=false
if ! command -v rustc >/dev/null 2>&1; then NEED_RUST=true
else
  RUST_VER=$(rustc --version 2>&1 || echo "0")
  if echo "$RUST_VER" | grep -qE "rustc 1\.([0-7][0-9]|[0-8][0-4])\."; then NEED_RUST=true; info "Rust $RUST_VER too old for RTK (needs 1.85+), upgrading..."; fi
fi
if [ "$NEED_RUST" = true ]; then
  info "Installing Rust stable via rustup..."
  if command -v rustup >/dev/null 2>&1; then rustup update stable 2>/dev/null; rustup default stable 2>/dev/null; ok "rustc $(rustc --version) (updated)"
  else curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable 2>/dev/null; export PATH="$HOME/.cargo/bin:$PATH"; fi
fi
ok "rustc $(rustc --version)"
command -v node >/dev/null || warn "node not found - install Node >=20 https://nodejs.org"; command -v node >/dev/null && ok "node $(node --version)"
if ! command -v python3 >/dev/null 2>&1 && ! command -v python >/dev/null 2>&1; then
  info "python3 not found - attempting install..."
  if command -v apt-get >/dev/null 2>&1; then sudo apt-get update -qq && sudo apt-get install -y python3 python3-pip 2>/dev/null && ok "python3 $(python3 --version)" || warn "python3 apt install failed - install manually: https://www.python.org/downloads/"
  elif command -v brew >/dev/null 2>&1; then brew install python@3.13 2>/dev/null && ok "python3 $(python3 --version)" || warn "python3 brew install failed"
  elif command -v dnf >/dev/null 2>&1; then sudo dnf install -y python3 python3-pip 2>/dev/null && ok "python3 $(python3 --version)" || warn "python3 dnf install failed"
  else warn "python3 not found - install Python 3.11+ https://www.python.org/downloads/ (required for litellm/mkdocs)"; fi
else command -v python3 >/dev/null 2>&1 && ok "python3 $(python3 --version)" || ok "python $(python --version)"; fi
command -v git >/dev/null || { echo "git not found"; exit 1; }; ok "$(git --version)"

if [ "$SKIP_EXTERNAL" = true ]; then info "Skipping external tools (--skip-external)"; else
  info "Installing first-class external tools..."
  command -v ast-grep >/dev/null && ok "ast-grep $(ast-grep --version)" || { info "  ast-grep via npm (@ast-grep/cli, prebuilt)..."; npm i -g @ast-grep/cli 2>/dev/null && ok "ast-grep installed" || warn "ast-grep npm install failed - fallback WARN"; }
  # engram - extract from .coderun/engram/*.tar.gz (Linux)
  ENGRAM_BIN="$HOME/bin/engram"
  if command -v engram >/dev/null 2>&1; then ok "engram $(engram --version 2>/dev/null | head -1)"
  elif [ -f "$ENGRAM_BIN" ]; then ok "engram binary at $ENGRAM_BIN"
  else
    REPO_TAR=$(ls "$ROOT/.coderun/engram/"*.tar.gz 2>/dev/null | head -1 || true)
    if [ -n "$REPO_TAR" ] && [ -f "$REPO_TAR" ]; then
      mkdir -p "$(dirname "$ENGRAM_BIN")" "$HOME/.cache/tmp/engram_extract"
      tar -xzf "$REPO_TAR" -C "$HOME/.cache/tmp/engram_extract" 2>/dev/null
      SRC=$(find "$HOME/.cache/tmp/engram_extract" -name "engram" -type f 2>/dev/null | head -1 || true)
      if [ -n "$SRC" ] && [ -f "$SRC" ]; then
        cp -f "$SRC" "$ENGRAM_BIN" 2>/dev/null && chmod +x "$ENGRAM_BIN" 2>/dev/null && ok "engram installed to $ENGRAM_BIN (from $(basename "$REPO_TAR"))" || warn "engram copy failed"
      else warn "engram tar.gz did not contain binary"
      fi
      rm -rf "$HOME/.cache/tmp/engram_extract" 2>/dev/null
    else warn "engram tar.gz not found at .coderun/engram/*.tar.gz"
    fi
  fi
  # FlashRank removed from v1 runtime per benchmark evaluation (see rerank.rs)
  # codebase-memory-mcp - extract from .coderun/codebase-memory/*.tar.gz (Linux, CLI mode)
  CBM_BIN="$HOME/bin/codebase-memory-mcp"
  if command -v codebase-memory-mcp >/dev/null 2>&1; then ok "codebase-memory-mcp $(codebase-memory-mcp --version 2>/dev/null | head -1)"
  elif [ -f "$CBM_BIN" ]; then ok "codebase-memory-mcp binary at $CBM_BIN"
  else
    REPO_TAR=$(ls "$ROOT/.coderun/codebase-memory/"*.tar.gz 2>/dev/null | head -1 || true)
    if [ -n "$REPO_TAR" ] && [ -f "$REPO_TAR" ]; then
      mkdir -p "$(dirname "$CBM_BIN")" "$HOME/.cache/tmp/cbm_extract"
      tar -xzf "$REPO_TAR" -C "$HOME/.cache/tmp/cbm_extract" 2>/dev/null
      SRC=$(find "$HOME/.cache/tmp/cbm_extract" -name "codebase-memory-mcp" -type f 2>/dev/null | head -1 || true)
      if [ -n "$SRC" ] && [ -f "$SRC" ]; then
        cp -f "$SRC" "$CBM_BIN" 2>/dev/null && chmod +x "$CBM_BIN" 2>/dev/null && ok "codebase-memory-mcp installed to $CBM_BIN (from $(basename "$REPO_TAR"))" || warn "codebase-memory-mcp copy failed"
      else warn "codebase-memory-mcp tar.gz did not contain binary"
      fi
      rm -rf "$HOME/.cache/tmp/cbm_extract" 2>/dev/null
    else warn "codebase-memory-mcp tar.gz not found at .coderun/codebase-memory/*.tar.gz"
    fi
  fi
  pip3 show litellm >/dev/null 2>&1 || pip3 install "litellm[proxy]" 2>/dev/null; ok "litellm"
   # RTK - extract from .coderun/rtk/*.tar.gz (Linux) -> ~/bin/rtk (NO COMPILE). Cargo only as last resort.
   RTK_BIN="$HOME/bin/rtk"
   if command -v rtk >/dev/null 2>&1; then ok "rtk $(rtk --version 2>/dev/null | head -1)"
   elif [ -f "$RTK_BIN" ]; then ok "rtk binary at $RTK_BIN"
   else
     REPO_TAR=$(ls "$ROOT/.coderun/rtk/"*.tar.gz 2>/dev/null | head -1 || true)
     if [ -n "$REPO_TAR" ] && [ -f "$REPO_TAR" ]; then
       mkdir -p "$(dirname "$RTK_BIN")" "$HOME/.cache/tmp/rtk_extract"
       tar -xzf "$REPO_TAR" -C "$HOME/.cache/tmp/rtk_extract" 2>/dev/null
       SRC=$(find "$HOME/.cache/tmp/rtk_extract" -name "rtk" -type f 2>/dev/null | head -1 || true)
       if [ -n "$SRC" ] && [ -f "$SRC" ]; then
         cp -f "$SRC" "$RTK_BIN" 2>/dev/null && chmod +x "$RTK_BIN" 2>/dev/null && ok "rtk installed to $RTK_BIN (from $(basename "$REPO_TAR"))" || warn "rtk copy failed"
       else warn "rtk tar.gz did not contain binary"
       fi
       rm -rf "$HOME/.cache/tmp/rtk_extract" 2>/dev/null
     fi
   fi
   if ! command -v rtk >/dev/null 2>&1 && [ ! -f "$RTK_BIN" ]; then
     info "  No prebuilt rtk found - building via cargo... FIRST BUILD TAKES SEVERAL MINUTES"
     if command -v rustup >/dev/null 2>&1; then rustup update stable 2>/dev/null; rustup default stable 2>/dev/null; fi
     cargo install --git https://github.com/rtk-ai/rtk --locked 2>/dev/null && ok "rtk (cargo git)" || cargo install rtk --locked 2>/dev/null && ok "rtk (crates.io)" || warn "rtk cargo install failed (or drop a prebuilt rtk into .coderun/rtk/)"
   fi
   if command -v mkdocs >/dev/null 2>&1; then ok "mkdocs $(mkdocs --version)"; elif python3 -m mkdocs --version >/dev/null 2>&1; then ok "mkdocs $(python3 -m mkdocs --version) (python3 -m)"; else pip3 install --user mkdocs mkdocs-material pymdown-extensions >/dev/null 2>&1; if command -v mkdocs >/dev/null 2>&1; then ok "mkdocs $(mkdocs --version)"; elif python3 -m mkdocs --version >/dev/null 2>&1; then ok "mkdocs $(python3 -m mkdocs --version) (python3 -m)"; else warn "mkdocs install failed - try: pip3 install --user mkdocs mkdocs-material pymdown-extensions"; fi; fi
   rustup component add clippy 2>/dev/null; ok "clippy"; command -v eslint >/dev/null || npm i -g eslint 2>/dev/null; command -v promptfoo >/dev/null || npm i -g promptfoo 2>/dev/null; if command -v promptfoo >/dev/null; then ok "promptfoo $(promptfoo --version 2>/dev/null | head -1)"; else warn "promptfoo install failed - try: npm i -g promptfoo"; fi; ok "eslint/promptfoo check"
     # v1: DBOS sidecar NOT built — future/workflow only, gated behind CODERUN_WORKFLOW_ENABLED=true (TASK-001)
     if [ "${CODERUN_WORKFLOW_ENABLED:-false}" = "true" ] && [ -f "$ROOT/future/workflow/dbos/package.json" ]; then (cd "$ROOT/future/workflow/dbos" && npm install >/dev/null 2>&1 && npx tsc >/dev/null 2>&1 && npx tsc --noEmit >/dev/null 2>&1 && [ -f dist/main.js ] && ok "future/workflow DBOS built" || warn "future DBOS build failed"); fi
     # legacy workflow/dbos never built in v1 (TASK-001 purge)
      if [ -f "$ROOT/workflow/dbos/package.json" ]; then warn "legacy workflow/dbos/package.json found — v1 excludes workflow/dbos (use future/workflow/dbos with CODERUN_WORKFLOW_ENABLED=true)"; fi
fi

# 1. Use prebuilt coderun (no compile/test - use repository binary)
if [ "$SKIP_BUILD" = true ]; then info "Skipping build check (--skip-build)"; fi
info "Checking prebuilt coderun..."
if [ -f "$ROOT/target/release/coderun" ] || [ -f "$ROOT/target/release/coderun.exe" ]; then ok "coderun at target/release/coderun(.exe)"; else warn "coderun binary not found at target/release/coderun - build manually: cargo build --release"; echo "prebuilt coderun missing - expected at target/release/coderun" >&2; exit 1; fi
if [ -f "$ROOT/target/release/coderun-daemon" ] || [ -f "$ROOT/target/release/coderun-daemon.exe" ]; then ok "coderun-daemon at target/release/coderun-daemon(.exe)"; else warn "coderun-daemon not found at target/release/coderun-daemon"; fi

# 1b. TASK-037: ship binaries to ~/.coderun/bin + persist on PATH, so coderun keeps working
# from any directory/shell even if this repo checkout is moved or cleaned. Idempotent re-run.
BIN_DIR="$HOME/.coderun/bin"
mkdir -p "$BIN_DIR"
SRC_CLI="$ROOT/target/release/coderun";   [ -f "$SRC_CLI" ]   || SRC_CLI="$ROOT/target/release/coderun.exe"
SRC_DAEMON="$ROOT/target/release/coderun-daemon"; [ -f "$SRC_DAEMON" ] || SRC_DAEMON="$ROOT/target/release/coderun-daemon.exe"
INSTALLED_CLI="$BIN_DIR/coderun"
if [ -f "$SRC_CLI" ]; then cp -f "$SRC_CLI" "$INSTALLED_CLI" 2>/dev/null && chmod +x "$INSTALLED_CLI" && ok "coderun installed to $INSTALLED_CLI" || { warn "failed to copy coderun to $BIN_DIR"; INSTALLED_CLI="$SRC_CLI"; }
else warn "no coderun binary to install (expected $ROOT/target/release/coderun)"; INSTALLED_CLI="$SRC_CLI"; fi
INSTALLED_DAEMON="$BIN_DIR/coderun-daemon"
if [ -f "$SRC_DAEMON" ]; then cp -f "$SRC_DAEMON" "$INSTALLED_DAEMON" 2>/dev/null && chmod +x "$INSTALLED_DAEMON" && ok "coderun-daemon installed to $INSTALLED_DAEMON" || { warn "failed to copy coderun-daemon to $BIN_DIR"; INSTALLED_DAEMON="$SRC_DAEMON"; }
else INSTALLED_DAEMON="$SRC_DAEMON"; fi
# Persist on PATH: idempotent append to ~/.profile and ~/.bashrc with a marker comment
for rc in "$HOME/.profile" "$HOME/.bashrc"; do
  if [ -f "$rc" ]; then
    grep -qs "CODERUN_BIN_PATH" "$rc" || printf '\n# CODERUN_BIN_PATH: coderun AI runtime CLI + daemon\nexport PATH="$HOME/.coderun/bin:$PATH"\n' >> "$rc" && ok "PATH entry ensured in $rc"
  fi
done
case ":$PATH:" in *":$BIN_DIR:"*) ;; *) export PATH="$BIN_DIR:$PATH" ;; esac

# 1c. Ship the bundled skills library to ~/.coderun/skills (installation destination),
# mirroring the ~/.coderun/models layout - works from any directory, repo-independent.
SRC_SKILLS="$ROOT/.coderun/skills"
DST_SKILLS="$HOME/.coderun/skills"
if [ -d "$SRC_SKILLS" ]; then
  mkdir -p "$DST_SKILLS"
  if cp -rf "$SRC_SKILLS"/. "$DST_SKILLS"/ 2>/dev/null; then
    ok "skills library copied to $DST_SKILLS ($(ls -1 "$DST_SKILLS" | wc -l) entries)"
  else warn "skills copy to $DST_SKILLS failed"; fi
else warn "no skills folder found at $SRC_SKILLS - skipped"; fi

# 2. Verify installation (doctor)
# NOTE: `coderun init` / `coderun index` are NOT run here on purpose - they bootstrap the
# repository they run IN (per-repo .coderun/ + index), which is meaningless for the coderun
# source checkout itself. Run them inside each repo you want analyzed.
info "Verifying installation (doctor)..."
"$INSTALLED_CLI" doctor

# 3. opencode plugin (GLOBAL: ~/.config/opencode - loaded in EVERY project)
OC_GLOBAL="$HOME/.config/opencode"
OC_GLOBAL_CFG="$OC_GLOBAL/opencode.jsonc"
ROOT_OPENCODE="$ROOT/.opencode"   # legacy project dir - cleaned below
info "Configuring opencode plugin (global ~/.config/opencode)..."
mkdir -p "$OC_GLOBAL" "$OC_GLOBAL/engram"
# Copy engram binary into global opencode dir (ABSOLUTE path reference - must resolve from any project)
ENGRAM_SRC=""
if [ -f "$HOME/bin/engram" ]; then ENGRAM_SRC="$HOME/bin/engram"
elif [ -f "$HOME/bin/engram.exe" ]; then ENGRAM_SRC="$HOME/bin/engram.exe"
elif command -v engram >/dev/null 2>&1; then ENGRAM_SRC=$(command -v engram)
fi
if [ -n "$ENGRAM_SRC" ] && [ -f "$ENGRAM_SRC" ]; then
  cp -f "$ENGRAM_SRC" "$OC_GLOBAL/engram/engram" 2>/dev/null && chmod +x "$OC_GLOBAL/engram/engram" 2>/dev/null && ok "engram copied to $OC_GLOBAL/engram/engram"
  # Also handle .exe for Windows
  if [ -f "$OC_GLOBAL/engram/engram" ] && [ ! -f "$OC_GLOBAL/engram/engram.exe" ]; then cp -f "$OC_GLOBAL/engram/engram" "$OC_GLOBAL/engram/engram.exe" 2>/dev/null || true; fi
else
  REPO_ZIP=$(ls "$ROOT/.coderun/engram/"*.zip 2>/dev/null | head -1 || true)
  if [ -n "$REPO_ZIP" ] && [ -f "$REPO_ZIP" ] && command -v unzip >/dev/null 2>&1; then
    mkdir -p "$HOME/.cache/tmp/engram_global" 2>/dev/null
    unzip -o "$REPO_ZIP" -d "$HOME/.cache/tmp/engram_global" 2>/dev/null
    SRC=$(find "$HOME/.cache/tmp/engram_global" -name "engram*" -type f 2>/dev/null | head -1 || true)
    if [ -n "$SRC" ] && [ -f "$SRC" ]; then cp -f "$SRC" "$OC_GLOBAL/engram/engram" 2>/dev/null && chmod +x "$OC_GLOBAL/engram/engram" 2>/dev/null && ok "engram copied to $OC_GLOBAL/engram/engram (from zip)"; fi
  fi
fi
# Global config: plugin only (MCPs are NOT exposed to the agent — engram/codebase used directly by daemon)
cat > "$OC_GLOBAL_CFG" <<EOF
{
    "\$schema": "https://opencode.ai/config.json",
    "plugin": ["opencode-coderun"]
}
EOF
ok "opencode plugin GLOBAL at $OC_GLOBAL_CFG (plugin: opencode-coderun, MCPs used internally by daemon)"
# Remove legacy global path plugin (now npm)
GLOBAL_PLUGIN="$HOME/.config/opencode/plugins/coderun.ts"
if [ -f "$GLOBAL_PLUGIN" ]; then rm -f "$GLOBAL_PLUGIN" 2>/dev/null && info "Removed legacy global path plugin coderun.ts" || true; fi
LOCAL_PLUGIN="$ROOT/.opencode/plugins/coderun.ts"
if [ -f "$LOCAL_PLUGIN" ]; then rm -f "$LOCAL_PLUGIN" 2>/dev/null && info "Removed legacy local path plugin .opencode/plugins/coderun.ts" || true; fi
# Migrate: remove per-project opencode config/deps (plugin is global now)
for f in "$ROOT_OPENCODE/opencode.jsonc" "$ROOT_OPENCODE/opencode.json" "$ROOT_OPENCODE/package.json" "$ROOT_OPENCODE/package-lock.json"; do
  if [ -f "$f" ]; then rm -f "$f" 2>/dev/null && info "Removed legacy project $(basename "$f") (MCPs/plugin are global now)" || true; fi
done
# Ensure npm plugin is built
if [ -d "$ROOT/packages/opencode-coderun" ]; then
  if [ ! -f "$ROOT/packages/opencode-coderun/dist/index.js" ]; then
    if command -v npm >/dev/null 2>&1; then
      info "Building opencode-coderun npm package..."
      (cd "$ROOT/packages/opencode-coderun" && npm install --silent 2>/dev/null && npm run build --silent 2>/dev/null && ok "opencode-coderun built to packages/opencode-coderun/dist") || warn "opencode-coderun build failed - run: cd packages/opencode-coderun && npm install && npm run build"
    else
      warn "npm not found - cannot build opencode-coderun (install Node.js 18+)"
    fi
  else
    ok "opencode-coderun dist at packages/opencode-coderun/dist/index.js"
  fi
  # Install npm plugin GLOBALLY (~/.config/opencode/node_modules) via file: reference to this repo
  if command -v npm >/dev/null 2>&1; then
    info "Installing opencode-coderun globally (~/.config/opencode)..."
    PKG_JSON="$OC_GLOBAL/package.json"
    PLUGIN_REF="file:$ROOT/packages/opencode-coderun"
    if [ ! -f "$PKG_JSON" ]; then
      printf '%s\n' '{' '  "dependencies": {' '    "@opencode-ai/plugin": "1.18.22",' "    \"opencode-coderun\": \"$PLUGIN_REF\"" '  }' '}' > "$PKG_JSON"
    elif command -v node >/dev/null 2>&1; then
      # Merge + always refresh the file: ref (repo may have moved)
      PKG_JSON_PATH="$PKG_JSON" PLUGIN_REF="$PLUGIN_REF" node -e "const fs=require('fs');const p=process.env.PKG_JSON_PATH;let j={};try{j=JSON.parse(fs.readFileSync(p,'utf8'))}catch(e){};j.dependencies=j.dependencies||{};j.dependencies['opencode-coderun']=process.env.PLUGIN_REF;j.dependencies['@opencode-ai/plugin']=j.dependencies['@opencode-ai/plugin']||'1.18.22';fs.writeFileSync(p,JSON.stringify(j,null,2))" 2>/dev/null || true
    fi
    (cd "$OC_GLOBAL" && npm install --silent 2>/dev/null && [ -f node_modules/opencode-coderun/dist/index.js ] && ok "opencode-coderun installed to ~/.config/opencode/node_modules (global)") || warn "opencode-coderun npm install failed - try: cd ~/.config/opencode && npm install"
  else
    warn "npm not found - skipping global opencode plugin install (install Node.js 18+)"
  fi
else
  warn "packages/opencode-coderun not found - skipping npm plugin install"
fi
info "Restart opencode to load global plugin 'opencode-coderun' (hooks: chat.message + message.updated + tool.execute.before, daemon http://127.0.0.1:9527). Plugin loads in EVERY project (global ~/.config/opencode). MCPs (engram, codebase-memory) used internally by daemon as fallback tools."

# 4. Start daemon - coderun must be in RUNNING state after installation
# TASK-037: launch from ~/.coderun/bin (installed copy), repo-independent working dir.
daemon_health() { curl -s -o /dev/null -m 2 http://127.0.0.1:9527/health; }
DAEMON_UP=no
if command -v curl >/dev/null 2>&1 && daemon_health; then
  DAEMON_UP=yes
  ok "coderun daemon already running at http://127.0.0.1:9527 (status: running)"
elif [ ! -x "$INSTALLED_DAEMON" ]; then
  warn "coderun-daemon binary not found at $INSTALLED_DAEMON - build first (cargo build --release) then re-run installer or start manually"
else
  # Stale processes (holding old binary/port but not answering /health) - stop them before restart
  pkill -f coderun-daemon >/dev/null 2>&1 || true
  info "Starting coderun daemon..."
  mkdir -p "$HOME/.coderun"
  (cd "$HOME/.coderun" && nohup "$INSTALLED_DAEMON" >/dev/null 2>&1 &)
  if command -v curl >/dev/null 2>&1; then
    for _ in $(seq 1 40); do
      sleep 0.5
      if daemon_health; then DAEMON_UP=yes; break; fi
    done
  else
    sleep 3
    if pgrep -f coderun-daemon >/dev/null 2>&1; then DAEMON_UP=yes; fi
  fi
  if [ "$DAEMON_UP" = yes ]; then ok "coderun daemon RUNNING (http://127.0.0.1:9527, from $INSTALLED_DAEMON)"; else warn "daemon not responding on :9527 within 20s - start manually: $INSTALLED_DAEMON"; fi
fi

info "Done (v1) - daemon: $(if [ "$DAEMON_UP" = yes ]; then echo 'RUNNING at http://127.0.0.1:9527'; else echo "NOT running (start: $INSTALLED_DAEMON)"; fi) | coderun preview 'add auth' | curl http://127.0.0.1:9527/metrics | coderun doctor"
info "Docs: mkdocs serve | promptfoo eval --config eval/promptfooconfig.yaml  |  coderun doctor"
info "Opt-in workflow: CODERUN_WORKFLOW_ENABLED=true bash future/workflow/dbos/build.sh (future only) | cargo build --features workflow"
