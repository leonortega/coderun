#!/usr/bin/env bash
# Coderun v0.5.0 first-class installer (Unix: Linux/macOS, bash)
# Tools are FIRST-CLASS (no optional except LSP, no Temporal) + uses prebuilt coderun (no compile/test).
# Idempotent. Usage: bash scripts/install.sh [--skip-build] [--skip-external]
# Note: --skip-build is deprecated (build always skipped, prebuilt at target/release/coderun is used)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SKIP_BUILD=false; SKIP_EXTERNAL=false
for arg in "$@"; do case "$arg" in --skip-build) SKIP_BUILD=true;; --skip-external) SKIP_EXTERNAL=true;; esac; done
info(){ echo -e "\033[36m[coderun]\033[0m $*"; } ; ok(){ echo -e "  \033[32m[OK]\033[0m $*"; } ; warn(){ echo -e "  \033[33m[WARN]\033[0m $*"; }

info "Coderun v0.5.0 installer"
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
  command -v ast-grep >/dev/null && ok "ast-grep $(ast-grep --version)" || { info "  ast-grep via cargo..."; cargo install ast-grep --locked 2>/dev/null && ok "ast-grep" || warn "ast-grep install failed - fallback WARN"; }
  # engram - local binary from .coderun/engram/*.zip or .coderun/engram/engram (no git clone)
  ENGRAM_BIN="$HOME/bin/engram"
  if command -v engram >/dev/null 2>&1; then ok "engram $(engram --version 2>/dev/null | head -1)"
  elif [ -f "$ENGRAM_BIN" ]; then ok "engram binary at $ENGRAM_BIN"
  else
    REPO_ZIP=$(ls "$ROOT/.coderun/engram/"*.zip 2>/dev/null | head -1 || true)
    REPO_EXE="$ROOT/.coderun/engram/engram"
    [ -f "$ROOT/.coderun/engram/engram.exe" ] && REPO_EXE="$ROOT/.coderun/engram/engram.exe"
    if [ -f "$REPO_EXE" ]; then
      mkdir -p "$(dirname "$ENGRAM_BIN")"
      cp -f "$REPO_EXE" "$ENGRAM_BIN" 2>/dev/null && chmod +x "$ENGRAM_BIN" 2>/dev/null && ok "engram installed to $ENGRAM_BIN (from .coderun/engram)" || warn "engram copy failed - local LIKE fallback"
    elif [ -n "$REPO_ZIP" ] && [ -f "$REPO_ZIP" ]; then
      mkdir -p "$(dirname "$ENGRAM_BIN")" "$HOME/.cache/tmp/engram_extract"
      if command -v unzip >/dev/null 2>&1; then
        unzip -o "$REPO_ZIP" -d "$HOME/.cache/tmp/engram_extract" 2>/dev/null
        SRC=$(find "$HOME/.cache/tmp/engram_extract" -name "engram*" -type f 2>/dev/null | head -1 || true)
        if [ -n "$SRC" ] && [ -f "$SRC" ]; then cp -f "$SRC" "$ENGRAM_BIN" 2>/dev/null && chmod +x "$ENGRAM_BIN" 2>/dev/null && ok "engram installed to $ENGRAM_BIN (from $(basename "$REPO_ZIP"))" || warn "engram copy failed - local LIKE fallback"
        else warn "engram zip did not contain binary - local LIKE fallback"
        fi
      else warn "unzip not found - cannot extract engram; local LIKE fallback"
      fi
    else warn "engram zip not found at .coderun/engram/*.zip - local LIKE fallback"
    fi
  fi
  if [ -d "$ROOT/../engram" ]; then ok "legacy engram clone at ../engram (unused, local binary preferred)"; fi
  # FlashRank - local from .coderun/models/flashrank.onnx -> user profile
  REPO_MODEL="$ROOT/.coderun/models/flashrank.onnx"; MODEL_DIR="$HOME/.coderun/models"; mkdir -p "$MODEL_DIR"
  if [ -f "$MODEL_DIR/flashrank.onnx" ]; then ok "FlashRank $MODEL_DIR/flashrank.onnx"
  elif [ -f "$REPO_MODEL" ]; then cp -f "$REPO_MODEL" "$MODEL_DIR/flashrank.onnx" 2>/dev/null && ok "FlashRank installed to $MODEL_DIR/flashrank.onnx (from .coderun/models)" || warn "FlashRank copy failed - TF-IDF fallback"
  else warn "FlashRank model missing at $MODEL_DIR/flashrank.onnx - TF-IDF fallback (expected at .coderun/models/flashrank.onnx)"
  command -v npx >/dev/null && { npm list -g codebase-memory-mcp >/dev/null 2>&1 || npm i -g codebase-memory-mcp 2>/dev/null; ok "codebase-memory-mcp"; } || true
  pip3 show litellm >/dev/null 2>&1 || pip3 install "litellm[proxy]" 2>/dev/null; ok "litellm"
   if command -v rtk >/dev/null 2>&1; then ok "rtk $(rtk --version 2>/dev/null | head -1)"; else
     if command -v rustup >/dev/null 2>&1; then rustup update stable 2>/dev/null; rustup default stable 2>/dev/null; fi
     cargo install --git https://github.com/rtk-ai/rtk --locked 2>/dev/null && ok "rtk $(rtk --version 2>/dev/null | head -1) (cargo git)" || cargo install rtk --locked 2>/dev/null && ok "rtk $(rtk --version 2>/dev/null | head -1) (crates.io)" || warn "rtk cargo install failed - built-ins fallback (needs Rust 1.85+; try: rustup update stable)"
   fi
   if command -v mkdocs >/dev/null 2>&1; then ok "mkdocs $(mkdocs --version)"; elif python3 -m mkdocs --version >/dev/null 2>&1; then ok "mkdocs $(python3 -m mkdocs --version) (python3 -m)"; else pip3 install --user mkdocs mkdocs-material pymdown-extensions >/dev/null 2>&1; if command -v mkdocs >/dev/null 2>&1; then ok "mkdocs $(mkdocs --version)"; elif python3 -m mkdocs --version >/dev/null 2>&1; then ok "mkdocs $(python3 -m mkdocs --version) (python3 -m)"; else warn "mkdocs install failed - try: pip3 install --user mkdocs mkdocs-material pymdown-extensions"; fi; fi
   rustup component add clippy 2>/dev/null; ok "clippy"; command -v eslint >/dev/null || npm i -g eslint 2>/dev/null; command -v promptfoo >/dev/null || npm i -g promptfoo 2>/dev/null; if command -v promptfoo >/dev/null; then ok "promptfoo $(promptfoo --version 2>/dev/null | head -1)"; else warn "promptfoo install failed - try: npm i -g promptfoo"; fi; ok "eslint/promptfoo check"
    if [ -f "$ROOT/workflow/dbos/package.json" ]; then (cd "$ROOT/workflow/dbos" && npm install >/dev/null 2>&1 && npx tsc >/dev/null 2>&1 && npx tsc --noEmit >/dev/null 2>&1 && [ -f dist/main.js ] && ok "DBOS sidecar deps + built dist/main.js" || warn "DBOS build failed"); fi
fi

# 0b. Ensure workflow config exists (local DBOS sidecar, no secret required)
CFG_PATH="$ROOT/.coderun/config.toml"
mkdir -p "$(dirname "$CFG_PATH")"
if [ ! -f "$CFG_PATH" ]; then
  cat > "$CFG_PATH" <<EOF
[workflow]
enabled = true
engine = "dbos"
dbos_endpoint = "http://localhost:3001"
EOF
  ok "Created .coderun/config.toml with [workflow] dbos"
else
  if ! grep -q "^\[workflow\]" "$CFG_PATH" 2>/dev/null; then
    printf '\n[workflow]\nenabled = true\nengine = "dbos"\ndbos_endpoint = "http://localhost:3001"\n' >> "$CFG_PATH"
    ok "Appended [workflow] to .coderun/config.toml"
  else
    ok "workflow config at .coderun/config.toml"
  fi
  # Remove legacy dbos_shared_secret if present (local sidecar no longer uses CODERUN_DBOS_SECRET)
  if grep -q "dbos_shared_secret" "$CFG_PATH" 2>/dev/null; then
    python3 -c "
import pathlib, re
p=pathlib.Path('$CFG_PATH')
raw=p.read_text()
raw=re.sub(r'(?m)^\s*dbos_shared_secret\s*=.*\n', '', raw)
p.write_text(raw)
" 2>/dev/null || sed -i '/dbos_shared_secret/d' "$CFG_PATH" 2>/dev/null || true
    info "  Removed legacy dbos_shared_secret from $CFG_PATH (no longer required)"
  fi
fi

# 0c. Ensure DBOS health
DBOS_ENDPOINT="${CODERUN_DBOS_ENDPOINT:-http://localhost:3001}"
if [ -f "$CFG_PATH" ]; then
  CFG_EP=$(grep -E 'dbos_endpoint\s*=\s*"' "$CFG_PATH" 2>/dev/null | sed -E 's/.*dbos_endpoint\s*=\s*"([^"]+)".*/\1/' | head -1)
  [ -n "$CFG_EP" ] && DBOS_ENDPOINT="$CFG_EP"
fi
info "Checking DBOS health at $DBOS_ENDPOINT/health ..."
REACHABLE=false
if command -v curl >/dev/null 2>&1; then
  if curl -sf --max-time 2 "$DBOS_ENDPOINT/health" >/dev/null 2>&1; then REACHABLE=true; fi
elif command -v wget >/dev/null 2>&1; then
  if wget -qO- --timeout=2 "$DBOS_ENDPOINT/health" >/dev/null 2>&1; then REACHABLE=true; fi
fi
if [ "$REACHABLE" = true ]; then
  ok "DBOS reachable at $DBOS_ENDPOINT"
else
  info "  DBOS not reachable - attempting to start sidecar..."
  STARTED=false
  if [ -f "$ROOT/workflow/dbos/dist/main.js" ] && command -v node >/dev/null 2>&1; then
    (cd "$ROOT/workflow/dbos" && DBOS_PORT=$(echo "$DBOS_ENDPOINT" | sed -E 's/.*:([0-9]+).*/\1/' | head -1) nohup node dist/main.js >/tmp/coderun-dbos.log 2>&1 & echo $! > /tmp/coderun-dbos.pid) 2>/dev/null || true
    sleep 3
    STARTED=true
  elif [ -f "$ROOT/workflow/dbos/package.json" ] && command -v npm >/dev/null 2>&1; then
    (cd "$ROOT/workflow/dbos" && DBOS_PORT=$(echo "$DBOS_ENDPOINT" | sed -E 's/.*:([0-9]+).*/\1/' | head -1) nohup npm start >/tmp/coderun-dbos.log 2>&1 & echo $! > /tmp/coderun-dbos.pid) 2>/dev/null || true
    sleep 3
    STARTED=true
  fi
  for i in $(seq 1 12 2>/dev/null || echo "1 2 3 4 5 6 7 8 9 10 11 12"); do
    sleep 1
    if command -v curl >/dev/null 2>&1; then
      if curl -sf --max-time 2 "$DBOS_ENDPOINT/health" >/dev/null 2>&1; then REACHABLE=true; break; fi
    elif command -v wget >/dev/null 2>&1; then
      if wget -qO- --timeout=2 "$DBOS_ENDPOINT/health" >/dev/null 2>&1; then REACHABLE=true; break; fi
    fi
  done
  if [ "$REACHABLE" = true ]; then
    ok "DBOS sidecar started at $DBOS_ENDPOINT"
  else
    if [ "$STARTED" = true ]; then warn "DBOS started but not reachable at $DBOS_ENDPOINT - check: cd workflow/dbos; node dist/main.js"; else warn "DBOS not reachable at $DBOS_ENDPOINT - hint: cd workflow/dbos; npm run build; node dist/main.js"; fi
  fi
fi

# 1. Use prebuilt coderun (no compile/test - use repository binary)
if [ "$SKIP_BUILD" = true ]; then info "Skipping build check (--skip-build)"; fi
info "Checking prebuilt coderun..."
if [ -f "$ROOT/target/release/coderun" ] || [ -f "$ROOT/target/release/coderun.exe" ]; then ok "coderun at target/release/coderun(.exe)"; else warn "coderun binary not found at target/release/coderun - build manually: cargo build --release"; echo "prebuilt coderun missing - expected at target/release/coderun" >&2; exit 1; fi
if [ -f "$ROOT/target/release/coderun-daemon" ] || [ -f "$ROOT/target/release/coderun-daemon.exe" ]; then ok "coderun-daemon at target/release/coderun-daemon(.exe)"; else warn "coderun-daemon not found at target/release/coderun-daemon"; fi

info "Initializing repo..."
CODERUN_BIN="$ROOT/target/release/coderun"
[ -f "$CODERUN_BIN.exe" ] && [ ! -f "$CODERUN_BIN" ] && CODERUN_BIN="$CODERUN_BIN.exe"
"$CODERUN_BIN" init 2>/dev/null || true
"$CODERUN_BIN" index 2>/dev/null || true
"$CODERUN_BIN" doctor

info "Configuring opencode MCPs and plugin (use .opencode folder, no absolute repo path)..."
mkdir -p "$ROOT/.opencode" "$ROOT/.opencode/plugins" "$ROOT/.opencode/engram"
# Copy engram binary into .opencode/engram/ for portable relative reference (never use  absolute)
ENGRAM_SRC=""
if [ -f "$HOME/bin/engram" ]; then ENGRAM_SRC="$HOME/bin/engram"
elif [ -f "$HOME/bin/engram.exe" ]; then ENGRAM_SRC="$HOME/bin/engram.exe"
elif command -v engram >/dev/null 2>&1; then ENGRAM_SRC=$(command -v engram)
fi
if [ -n "$ENGRAM_SRC" ] && [ -f "$ENGRAM_SRC" ]; then
  cp -f "$ENGRAM_SRC" "$ROOT/.opencode/engram/engram" 2>/dev/null && chmod +x "$ROOT/.opencode/engram/engram" 2>/dev/null && ok "engram copied to .opencode/engram/engram (portable)"
  # Also handle .exe for Windows
  if [ -f "$ROOT/.opencode/engram/engram" ] && [ ! -f "$ROOT/.opencode/engram/engram.exe" ]; then cp -f "$ROOT/.opencode/engram/engram" "$ROOT/.opencode/engram/engram.exe" 2>/dev/null || true; fi
else
  REPO_ZIP=$(ls "$ROOT/.coderun/engram/"*.zip 2>/dev/null | head -1 || true)
  if [ -n "$REPO_ZIP" ] && [ -f "$REPO_ZIP" ] && command -v unzip >/dev/null 2>&1; then
    mkdir -p "$HOME/.cache/tmp/engram_opencode" 2>/dev/null
    unzip -o "$REPO_ZIP" -d "$HOME/.cache/tmp/engram_opencode" 2>/dev/null
    SRC=$(find "$HOME/.cache/tmp/engram_opencode" -name "engram*" -type f 2>/dev/null | head -1 || true)
    if [ -n "$SRC" ] && [ -f "$SRC" ]; then cp -f "$SRC" "$ROOT/.opencode/engram/engram" 2>/dev/null && chmod +x "$ROOT/.opencode/engram/engram" 2>/dev/null && ok "engram copied to .opencode/engram/engram (from zip)"; fi
  fi
fi
# Use relative .opencode path for MCP (never absolute )
cat > "$ROOT/.opencode/opencode.jsonc" <<EOF
{
    "\$schema": "https://opencode.ai/config.json",
    "plugin": ["opencode-coderun"],
    "mcp": {
        "codebase-memory": {
            "command": ["npx", "-y", "codebase-memory-mcp"],
            "type": "local",
            "enabled": true
        },
        "engram": {
            "command": [".opencode/engram/engram", "mcp", "--tools=agent"],
            "type": "local",
            "enabled": true
        }
    }
}
EOF
ok "opencode MCPs + plugin at .opencode/opencode.jsonc (codebase-memory + engram -> .opencode/engram/engram, plugin: opencode-coderun)"
# Remove legacy global path plugin (now npm)
GLOBAL_PLUGIN="$HOME/.config/opencode/plugins/coderun.ts"
if [ -f "$GLOBAL_PLUGIN" ]; then rm -f "$GLOBAL_PLUGIN" 2>/dev/null && info "Removed legacy global path plugin coderun.ts" || true; fi
LOCAL_PLUGIN="$ROOT/.opencode/plugins/coderun.ts"
if [ -f "$LOCAL_PLUGIN" ]; then rm -f "$LOCAL_PLUGIN" 2>/dev/null && info "Removed legacy local path plugin .opencode/plugins/coderun.ts" || true; fi
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
  # Install npm plugin into .opencode (file: reference, no registry needed)
  if command -v npm >/dev/null 2>&1; then
    info "Installing opencode-coderun into .opencode via npm..."
    mkdir -p "$ROOT/.opencode"
    # Ensure package.json has file: dependency (idempotent)
    if [ ! -f "$ROOT/.opencode/package.json" ]; then
      printf '%s\n' '{' '  "dependencies": {' '    "@opencode-ai/plugin": "1.18.22",' '    "opencode-coderun": "file:../packages/opencode-coderun"' '  }' '}' > "$ROOT/.opencode/package.json"
    else
      # Use node to inject file: dependency if missing (fallback to sed)
      if ! grep -q "opencode-coderun" "$ROOT/.opencode/package.json" 2>/dev/null; then
        if command -v node >/dev/null 2>&1; then
          node -e "const fs=require('fs');const p='$ROOT/.opencode/package.json';let j=JSON.parse(fs.readFileSync(p,'utf8'));j.dependencies=j.dependencies||{};j.dependencies['opencode-coderun']='file:../packages/opencode-coderun';j.dependencies['@opencode-ai/plugin']=j.dependencies['@opencode-ai/plugin']||'1.18.22';fs.writeFileSync(p,JSON.stringify(j,null,2))" 2>/dev/null || true
        else
          # sed fallback: inject before last }
          sed -i 's/"@opencode-ai\/plugin"[[:space:]]*:[^,}]*/"&,\n    \"opencode-coderun\": \"file:..\/packages\/opencode-coderun\"/' "$ROOT/.opencode/package.json" 2>/dev/null || true
        fi
      fi
    fi
    (cd "$ROOT/.opencode" && npm install --silent 2>/dev/null && ok "opencode-coderun installed to .opencode/node_modules") || warn "opencode-coderun npm install failed - try: cd .opencode && npm install"
  else
    warn "npm not found - skipping .opencode install (install Node.js 18+)"
  fi
else
  warn "packages/opencode-coderun not found - skipping npm plugin install"
fi
info "Restart opencode to load plugin 'opencode-coderun' (hooks: chat.message + message.updated + tool.execute.before, daemon http://127.0.0.1:9527)"

info "Done - next: coderun serve | coderun preview 'add auth' | coderun workflow start 'refactor' --require-approval | curl http://127.0.0.1:9527/metrics"
info "Docs: mkdocs serve | promptfoo eval --config eval/promptfooconfig.yaml  |  coderun doctor"
