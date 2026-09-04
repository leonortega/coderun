#!/usr/bin/env bash
# Knocode installer v0.9.7 minimal (Unix: Linux/macOS, bash)
# Minimal v1: Git + SQLite(bundled)/tree-sitter/tantivy/tiktoken embedded + RTK optional (no Rust - prebuilt binaries; compile via scripts/compile.sh)
# Agent integrations (OpenCode/Copilot) are selectable: --agents opencode,copilot | --all-agents | --no-agents
# Idempotent. Usage: bash scripts/install.sh [--skip-build] [--agents a,b,c|--all-agents|--no-agents] [--skip-prereqs]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SKIP_BUILD=false; AGENTS=""; ALL_AGENTS=false; NO_AGENTS=false; SKIP_PREREQS=false
for arg in "$@"; do case "$arg" in
  --skip-build) SKIP_BUILD=true;;
  --agents) AGENTS="$2"; shift;;
  --agents=*) AGENTS="${arg#--agents=}";;
  --all-agents) ALL_AGENTS=true;;
  --no-agents) NO_AGENTS=true;;
  --skip-prereqs) SKIP_PREREQS=true;;
  -h|--help) echo "Usage: $0 [--skip-build] [--agents opencode,copilot | --all-agents | --no-agents] [--skip-prereqs]"; exit 0;;
esac; done
info(){ echo -e "\033[36m[knocode]\033[0m $*"; } ; ok(){ echo -e "  \033[32m[OK]\033[0m $*"; } ; warn(){ echo -e "  \033[33m[WARN]\033[0m $*"; } ; skip(){ echo -e "  \033[90m[SKIP]\033[0m $*"; }

# --- Agent catalog & selection -------------------------------------------------
AGENT_CATALOG="opencode copilot"
select_agents() {
  if [ "$NO_AGENTS" = true ]; then echo ""; return; fi
  if [ -n "$AGENTS" ]; then
    local sel=""
    IFS=',' read -ra parts <<< "$AGENTS"
    for a in "${parts[@]}"; do
      a="$(echo "$a" | tr '[:upper:]' '[:lower:]' | xargs)"
      case " $AGENT_CATALOG " in *" $a "*) sel="$sel $a";; *) warn "unknown agent '$a' - valid: opencode, copilot";; esac
    done
    if [ -z "$sel" ]; then echo "error: no valid agents in --agents ('$AGENTS')" >&2; exit 1; fi
    echo "$sel"; return
  fi
  if [ "$ALL_AGENTS" = true ]; then echo "$AGENT_CATALOG"; return; fi
  # Interactive multi-select when stdin is a terminal; default to ALL otherwise
  if [ ! -t 0 ]; then
    info "non-interactive run - installing agent integrations for ALL agents (use --agents opencode or --no-agents to change)"
    echo "$AGENT_CATALOG"; return
  fi
  local sel=""
  for a in $AGENT_CATALOG; do
    printf "  Wire up %s? [Y/n] " "$a"
    read -r r
    case "$r" in ""|y|Y|yes|YES) sel="$sel $a";; *) skip "$a skipped";; esac
  done
  echo "$sel"
}

info "Knocode installer"
AGENT_SEL="$(select_agents)"
if [ -n "$AGENT_SEL" ]; then info "Agent integrations:$(echo "$AGENT_SEL")"; else info "Agent integrations: none"; fi

# 0a. Stop any running daemon/CLI up front - later steps REPLACE binaries (~/.knocode/bin)
# and a locked exe would fail the copy. The fresh daemon is restarted at the end (step 4).
for p in knocode-daemon knocode; do
  if pgrep -x "$p" >/dev/null 2>&1; then pkill -x "$p" 2>/dev/null || true; ok "stopped $p"; fi
done

# No Rust needed: knocode ships prebuilt (target/release) and the installer does not compile.
# Source builds use scripts/compile.sh (or CI).
if ! command -v node >/dev/null 2>&1; then
  if [ "$SKIP_PREREQS" = true ]; then warn "node not found - install Node >=20 https://nodejs.org (or re-run without --skip-prereqs)"
  else
    info "Installing Node.js (LTS)..."
    if command -v apt-get >/dev/null 2>&1; then sudo apt-get update -qq && sudo apt-get install -y nodejs npm 2>/dev/null && ok "node $(node --version)" || warn "node apt install failed - install manually: https://nodejs.org"
    elif command -v brew >/dev/null 2>&1; then brew install node 2>/dev/null && ok "node $(node --version)" || warn "node brew install failed"
    elif command -v dnf >/dev/null 2>&1; then sudo dnf install -y nodejs npm 2>/dev/null && ok "node $(node --version)" || warn "node dnf install failed"
    else warn "no package manager for node - install manually: https://nodejs.org"; fi
  fi
else ok "node $(node --version)"; fi
if ! command -v python3 >/dev/null 2>&1 && ! command -v python >/dev/null 2>&1; then
  info "python3 not found - attempting install..."
  if command -v apt-get >/dev/null 2>&1; then sudo apt-get update -qq && sudo apt-get install -y python3 python3-pip 2>/dev/null && ok "python3 $(python3 --version)" || warn "python3 apt install failed - install manually: https://www.python.org/downloads/"
  elif command -v brew >/dev/null 2>&1; then brew install python@3.13 2>/dev/null && ok "python3 $(python3 --version)" || warn "python3 brew install failed"
  elif command -v dnf >/dev/null 2>&1; then sudo dnf install -y python3 python3-pip 2>/dev/null && ok "python3 $(python3 --version)" || warn "python3 dnf install failed"
  else warn "python3 not found - install Python 3.11+ https://www.python.org/downloads/"; fi
else command -v python3 >/dev/null 2>&1 && ok "python3 $(python3 --version)" || ok "python $(python --version)"; fi
if ! command -v git >/dev/null 2>&1; then
  if [ "$SKIP_PREREQS" = true ]; then echo "git not found"; exit 1; fi
  info "Installing git..."
  if command -v apt-get >/dev/null 2>&1; then sudo apt-get update -qq && sudo apt-get install -y git 2>/dev/null && ok "$(git --version)" || { echo "git install failed"; exit 1; }
  elif command -v brew >/dev/null 2>&1; then brew install git 2>/dev/null && ok "$(git --version)" || { echo "git install failed"; exit 1; }
  elif command -v dnf >/dev/null 2>&1; then sudo dnf install -y git 2>/dev/null && ok "$(git --version)" || { echo "git install failed"; exit 1; }
  else echo "git not found - install manually: https://git-scm.com"; exit 1; fi
else ok "$(git --version)"; fi

info "Installing external tools..."
   # RTK - download prebuilt release for this platform -> ~/.knocode/bin/rtk (NO COMPILE). Unified bin.
   RTK_BIN="$HOME/.knocode/bin/rtk"
   if [ -f "$HOME/bin/rtk" ] && [ ! -f "$RTK_BIN" ]; then mkdir -p "$(dirname "$RTK_BIN")"; cp -f "$HOME/bin/rtk" "$RTK_BIN" 2>/dev/null && chmod +x "$RTK_BIN" 2>/dev/null && ok "migrated legacy ~/bin/rtk -> $RTK_BIN" || true; fi
   if command -v rtk >/dev/null 2>&1; then ok "rtk $(rtk --version 2>/dev/null | head -1)"
   elif [ -f "$RTK_BIN" ]; then ok "rtk binary at $RTK_BIN"
   else
     RTK_OS="$(uname -s 2>/dev/null | tr "[:upper:]" "[:lower:]")"; RTK_ARCH="$(uname -m 2>/dev/null | tr "[:upper:]" "[:lower:]")"
     case "$RTK_OS:$RTK_ARCH" in
       linux:x86_64|linux:amd64) RTK_ASSET="rtk-x86_64-unknown-linux-musl.tar.gz";;
       linux:aarch64|linux:arm64) RTK_ASSET="rtk-aarch64-unknown-linux-gnu.tar.gz";;
       darwin:x86_64) RTK_ASSET="rtk-x86_64-apple-darwin.tar.gz";;
       darwin:aarch64|darwin:arm64) RTK_ASSET="rtk-aarch64-apple-darwin.tar.gz";;
       *) RTK_ASSET="";;
     esac
     if [ -z "$RTK_ASSET" ]; then warn "rtk: unsupported platform ($RTK_OS/$RTK_ARCH) - install manually from https://github.com/rtk-ai/rtk/releases"
     else
       RTK_URL="https://github.com/rtk-ai/rtk/releases/latest/download/$RTK_ASSET"
       RTK_TMP="$(mktemp -d 2>/dev/null || echo "$HOME/.cache/tmp/rtk_dl")"; mkdir -p "$RTK_TMP"
       info "  downloading rtk release ($RTK_ASSET)..."
       if { command -v curl >/dev/null 2>&1 && curl -fsSL "$RTK_URL" -o "$RTK_TMP/$RTK_ASSET"; } || { command -v wget >/dev/null 2>&1 && wget -q "$RTK_URL" -O "$RTK_TMP/$RTK_ASSET"; }; then
         tar -xzf "$RTK_TMP/$RTK_ASSET" -C "$RTK_TMP" 2>/dev/null
         RTK_SRC="$(find "$RTK_TMP" -name rtk -type f 2>/dev/null | head -1 || true)"
         if [ -n "$RTK_SRC" ] && [ -f "$RTK_SRC" ]; then
           mkdir -p "$(dirname "$RTK_BIN")"
           cp -f "$RTK_SRC" "$RTK_BIN" 2>/dev/null && chmod +x "$RTK_BIN" 2>/dev/null && ok "rtk installed to $RTK_BIN (from GitHub release)" || warn "rtk copy failed"
         else warn "rtk release archive did not contain the rtk binary"
         fi
       else warn "rtk download failed - install manually from https://github.com/rtk-ai/rtk/releases"
       fi
       rm -rf "$RTK_TMP" 2>/dev/null
     fi
   fi


# 1. Use prebuilt knocode (no compile/test - use repository binary)
if [ "$SKIP_BUILD" = true ]; then info "Skipping build check (--skip-build)"; fi
info "Checking prebuilt knocode..."
if [ -f "$ROOT/target/release/knocode" ] || [ -f "$ROOT/target/release/knocode.exe" ]; then ok "knocode at target/release/knocode(.exe)"; else warn "knocode binary not found at target/release/knocode - build manually: cargo build --release"; echo "prebuilt knocode missing - expected at target/release/knocode" >&2; exit 1; fi
if [ -f "$ROOT/target/release/knocode-daemon" ] || [ -f "$ROOT/target/release/knocode-daemon.exe" ]; then ok "knocode-daemon at target/release/knocode-daemon(.exe)"; else warn "knocode-daemon not found at target/release/knocode-daemon"; fi

# 1b. TASK-037: ship binaries to ~/.knocode/bin + persist on PATH, so knocode keeps working
# from any directory/shell even if this repo checkout is moved or cleaned. Idempotent re-run.
BIN_DIR="$HOME/.knocode/bin"
mkdir -p "$BIN_DIR"
SRC_CLI="$ROOT/target/release/knocode";   [ -f "$SRC_CLI" ]   || SRC_CLI="$ROOT/target/release/knocode.exe"
SRC_DAEMON="$ROOT/target/release/knocode-daemon"; [ -f "$SRC_DAEMON" ] || SRC_DAEMON="$ROOT/target/release/knocode-daemon.exe"
INSTALLED_CLI="$BIN_DIR/knocode"
if [ -f "$SRC_CLI" ]; then cp -f "$SRC_CLI" "$INSTALLED_CLI" 2>/dev/null && chmod +x "$INSTALLED_CLI" && ok "knocode installed to $INSTALLED_CLI" || { warn "failed to copy knocode to $BIN_DIR"; INSTALLED_CLI="$SRC_CLI"; }
else warn "no knocode binary to install (expected $ROOT/target/release/knocode)"; INSTALLED_CLI="$SRC_CLI"; fi
INSTALLED_DAEMON="$BIN_DIR/knocode-daemon"
if [ -f "$SRC_DAEMON" ]; then cp -f "$SRC_DAEMON" "$INSTALLED_DAEMON" 2>/dev/null && chmod +x "$INSTALLED_DAEMON" && ok "knocode-daemon installed to $INSTALLED_DAEMON" || { warn "failed to copy knocode-daemon to $BIN_DIR"; INSTALLED_DAEMON="$SRC_DAEMON"; }
else INSTALLED_DAEMON="$SRC_DAEMON"; fi
# Persist on PATH: idempotent append to ~/.profile and ~/.bashrc with a marker comment
for rc in "$HOME/.profile" "$HOME/.bashrc"; do
  if [ -f "$rc" ]; then
    grep -qs "KNOCODE_BIN_PATH" "$rc" || printf '\n# KNOCODE_BIN_PATH: knocode AI runtime CLI + daemon\nexport PATH="$HOME/.knocode/bin:$PATH"\n' >> "$rc" && ok "PATH entry ensured in $rc"
  fi
done
case ":$PATH:" in *":$BIN_DIR:"*) ;; *) export PATH="$BIN_DIR:$PATH" ;; esac

# 2. Verify installation (doctor)
# NOTE: `knocode init` / `knocode index` are NOT run here on purpose - they bootstrap the
# repository they run IN (per-repo .knocode/ + index), which is meaningless for the knocode
# source checkout itself. Run them inside each repo you want analyzed.
info "Verifying installation (doctor)..."
"$INSTALLED_CLI" doctor

# =====================================================================================
# 3. Agent integrations (OpenCode / Copilot) - selected above
# =====================================================================================
if [ -n "$AGENT_SEL" ]; then
  OC_GLOBAL="$HOME/.config/opencode"
  MCP_DIST="$ROOT/packages/knocode-mcp/dist/index.js"
  WANT_MCP=false
  for a in $AGENT_SEL; do case "$a" in copilot) WANT_MCP=true;; esac; done

  # --- shared MCP server build (Copilot uses knocode-mcp) ---
  if [ "$WANT_MCP" = true ]; then
    if [ -d "$ROOT/packages/knocode-mcp" ]; then
      if [ ! -f "$MCP_DIST" ]; then
        if command -v npm >/dev/null 2>&1; then
          info "Building knocode-mcp MCP server..."
          (cd "$ROOT/packages/knocode-mcp" && npm install --silent 2>/dev/null && npm run build --silent 2>/dev/null && ok "knocode-mcp built to packages/knocode-mcp/dist") || warn "knocode-mcp build failed - run: cd packages/knocode-mcp && npm install && npm run build"
        else warn "npm not found - cannot build knocode-mcp (MCP agents need Node.js)"; fi
      else ok "knocode-mcp dist at packages/knocode-mcp/dist/index.js"; fi
    else warn "packages/knocode-mcp not found - skipping MCP server build"; fi
  fi

  # --- OpenCode: global plugin + skill (~/.config/opencode) ---
  if echo "$AGENT_SEL" | grep -qw opencode; then
    OC_GLOBAL_CFG="$OC_GLOBAL/opencode.jsonc"
    info "Configuring opencode plugin (global ~/.config/opencode)..."
    mkdir -p "$OC_GLOBAL"
    cat > "$OC_GLOBAL_CFG" <<EOF
{
    "\$schema": "https://opencode.ai/config.json",
    "plugin": ["opencode-knocode"]
}
EOF
    ok "opencode plugin GLOBAL at $OC_GLOBAL_CFG (plugin: opencode-knocode, MCPs used internally by daemon)"
    # Remove legacy global path plugin (now npm)
    GLOBAL_PLUGIN="$HOME/.config/opencode/plugins/knocode.ts"
    if [ -f "$GLOBAL_PLUGIN" ]; then rm -f "$GLOBAL_PLUGIN" 2>/dev/null && info "Removed legacy global path plugin knocode.ts" || true; fi
    LOCAL_PLUGIN="$ROOT/.opencode/plugins/knocode.ts"
    if [ -f "$LOCAL_PLUGIN" ]; then rm -f "$LOCAL_PLUGIN" 2>/dev/null && info "Removed legacy local path plugin .opencode/plugins/knocode.ts" || true; fi
    # Migrate: remove per-project opencode config/deps (plugin is global now)
    for f in "$ROOT/.opencode/opencode.jsonc" "$ROOT/.opencode/opencode.json" "$ROOT/.opencode/package.json" "$ROOT/.opencode/package-lock.json"; do
      if [ -f "$f" ]; then rm -f "$f" 2>/dev/null && info "Removed legacy project $(basename "$f") (MCPs/plugin are global now)" || true; fi
    done
    # Ensure npm plugin is built
    if [ -d "$ROOT/packages/opencode-knocode" ]; then
      if [ ! -f "$ROOT/packages/opencode-knocode/dist/index.js" ]; then
        if command -v npm >/dev/null 2>&1; then
          info "Building opencode-knocode npm package..."
          (cd "$ROOT/packages/opencode-knocode" && npm install --silent 2>/dev/null && npm run build --silent 2>/dev/null && ok "opencode-knocode built to packages/opencode-knocode/dist") || warn "opencode-knocode build failed - run: cd packages/opencode-knocode && npm install && npm run build"
        else warn "npm not found - cannot build opencode-knocode (install Node.js 18+)"; fi
      else ok "opencode-knocode dist at packages/opencode-knocode/dist/index.js"; fi
      # Install npm plugin GLOBALLY (~/.config/opencode/node_modules) via file: reference to this repo
      if command -v npm >/dev/null 2>&1; then
        info "Installing opencode-knocode globally (~/.config/opencode)..."
        PKG_JSON="$OC_GLOBAL/package.json"
        PLUGIN_REF="file:$ROOT/packages/opencode-knocode"
        if [ ! -f "$PKG_JSON" ]; then
          printf '%s\n' '{' '  "dependencies": {' '    "@opencode-ai/plugin": "1.18.22",' "    \"opencode-knocode\": \"$PLUGIN_REF\"" '  }' '}' > "$PKG_JSON"
        elif command -v node >/dev/null 2>&1; then
          PKG_JSON_PATH="$PKG_JSON" PLUGIN_REF="$PLUGIN_REF" node -e "const fs=require('fs');const p=process.env.PKG_JSON_PATH;let j={};try{j=JSON.parse(fs.readFileSync(p,'utf8'))}catch(e){};j.dependencies=j.dependencies||{};j.dependencies['opencode-knocode']=process.env.PLUGIN_REF;j.dependencies['@opencode-ai/plugin']=j.dependencies['@opencode-ai/plugin']||'1.18.22';fs.writeFileSync(p,JSON.stringify(j,null,2))" 2>/dev/null || true
        fi
        (cd "$OC_GLOBAL" && npm install --silent 2>/dev/null && [ -f node_modules/opencode-knocode/dist/index.js ] && ok "opencode-knocode installed to ~/.config/opencode/node_modules (global)") || warn "opencode-knocode npm install failed - try: cd ~/.config/opencode && npm install"
      else warn "npm not found - skipping global opencode plugin install (install Node.js 18+)"; fi
    else warn "packages/opencode-knocode not found - skipping npm plugin install"; fi
    # Knocode agent skill (opencode - agent-native discovery; per-agent: opencode is the only supported agent for now)
    OC_SKILL_SRC="$ROOT/.knocode/skills/knocode"
    if [ -f "$OC_SKILL_SRC/SKILL.md" ]; then
      mkdir -p "$OC_GLOBAL/skills" && cp -rf "$OC_SKILL_SRC" "$OC_GLOBAL/skills/" 2>/dev/null && ok "knocode skill installed to $OC_GLOBAL/skills/knocode (opencode agent-native)" || warn "knocode skill copy failed (source: $OC_SKILL_SRC)"
    else warn ".knocode/skills/knocode not found - skipping agent skill install"; fi
    info "Restart opencode to load global plugin 'opencode-knocode' (hooks: chat.message + message.updated + tool.execute.before, daemon http://127.0.0.1:9527). Plugin loads in EVERY project (global ~/.config/opencode)."
  fi

  # --- Copilot (VS Code): user-level mcp.json (~/.config/Code/User/mcp.json) ---
  if echo "$AGENT_SEL" | grep -qw copilot; then
    CODE_USER_DIR="$HOME/.config/Code/User"; VSCODE_MCP="$CODE_USER_DIR/mcp.json"
    mkdir -p "$CODE_USER_DIR"
    if [ ! -f "$VSCODE_MCP" ]; then
      printf '{\n  "servers": {\n    "knocode": { "command": "node", "args": ["%s"], "env": { "KNOCODE_DAEMON_URL": "http://127.0.0.1:9527" } }\n  }\n}\n' "$MCP_DIST" > "$VSCODE_MCP"
      ok "VS Code Copilot MCP at $VSCODE_MCP (user scope)"
    else
      if command -v node >/dev/null 2>&1; then
        VSCODE_MCP_PATH="$VSCODE_MCP" MCP_DIST="$MCP_DIST" node -e "const fs=require('fs');const p=process.env.VSCODE_MCP_PATH;let j={};try{j=JSON.parse(fs.readFileSync(p,'utf8'))}catch(e){};j.servers=j.servers||{};j.servers.knocode={command:'node',args:[process.env.MCP_DIST],env:{KNOCODE_DAEMON_URL:'http://127.0.0.1:9527'}};fs.writeFileSync(p,JSON.stringify(j,null,2))" 2>/dev/null && ok "VS Code Copilot MCP updated at $VSCODE_MCP" || skip "VS Code mcp.json exists but could not merge knocode into $VSCODE_MCP"
      else skip "VS Code mcp.json exists but could not merge knocode (node missing)"; fi
    fi
  fi

  # --- Copilot Agent Plugin (hooks: SessionStart/PreToolUse/PostToolUse) ---
  # Deploy to ~/.knocode/copilot-plugin (repo-independent, survives repo moves).
  if echo "$AGENT_SEL" | grep -qw copilot; then
    PLUGIN_SRC="$ROOT/packages/knocode-copilot-plugin"
    PLUGIN_DST="$HOME/.knocode/copilot-plugin"
    if [ -f "$PLUGIN_SRC/plugin.json" ]; then
      rm -rf "$PLUGIN_DST" 2>/dev/null || true
      mkdir -p "$PLUGIN_DST"
      if cp -r "$PLUGIN_SRC/." "$PLUGIN_DST/" 2>/dev/null; then
        ok "Copilot Agent Plugin deployed to $PLUGIN_DST (hooks + MCP)"
      else
        warn "failed to deploy Copilot Agent Plugin to $PLUGIN_DST"
      fi
    else
      warn "packages/knocode-copilot-plugin not found - skipping Agent Plugin deploy"
    fi

    # --- @knocode chat participant extension (VSIX via `code` CLI) ---
    EXT_DIR="$ROOT/packages/vscode-copilot-knocode"
    if [ -f "$EXT_DIR/package.json" ]; then
      if command -v npm >/dev/null 2>&1 && [ ! -f "$EXT_DIR/dist/extension.js" ]; then
        info "Building vscode-copilot-knocode extension..."
        (cd "$EXT_DIR" && npm install --silent 2>/dev/null && npm run build --silent 2>/dev/null) || warn "vscode-copilot-knocode build failed"
      fi
      if [ -z "$(ls "$EXT_DIR"/*.vsix 2>/dev/null)" ] && command -v npm >/dev/null 2>&1; then
        (cd "$EXT_DIR" && npx --yes @vscode/vsce package --no-dependencies >/dev/null 2>&1) || true
      fi
      VSIX="$(ls "$EXT_DIR"/*.vsix 2>/dev/null | head -n 1 || true)"
      if [ -n "$VSIX" ]; then
        if command -v code >/dev/null 2>&1; then
          info "Installing @knocode VS Code extension (code --install-extension)..."
          if code --install-extension "$VSIX" --force >/dev/null 2>&1; then
            ok "@knocode extension installed from $(basename "$VSIX") - reload VS Code to activate"
          else
            warn "code CLI install failed - install manually: code --install-extension $VSIX"
          fi
        else
          warn "VS Code 'code' CLI not on PATH - install manually: code --install-extension $VSIX"
        fi
      else
        warn "vscode-copilot-knocode VSIX not built - run: cd packages/vscode-copilot-knocode && npx @vscode/vsce package"
      fi
    else
      warn "packages/vscode-copilot-knocode not found - skipping @knocode extension install"
    fi
  fi
fi

# 4. Start daemon - knocode must be in RUNNING state after installation
# TASK-037: launch from ~/.knocode/bin (installed copy), repo-independent working dir.
daemon_health() { curl -s -o /dev/null -m 2 http://127.0.0.1:9527/health; }
DAEMON_UP=no
if command -v curl >/dev/null 2>&1 && daemon_health; then
  DAEMON_UP=yes
  ok "knocode daemon already running at http://127.0.0.1:9527 (status: running)"
elif [ ! -x "$INSTALLED_DAEMON" ]; then
  warn "knocode-daemon binary not found at $INSTALLED_DAEMON - build first (cargo build --release) then re-run installer or start manually"
else
  # Stale processes (holding old binary/port but not answering /health) - stop them before restart
  pkill -f knocode-daemon >/dev/null 2>&1 || true
  info "Starting knocode daemon..."
  mkdir -p "$HOME/.knocode"
  (cd "$HOME/.knocode" && nohup "$INSTALLED_DAEMON" >/dev/null 2>&1 &)
  if command -v curl >/dev/null 2>&1; then
    for _ in $(seq 1 40); do
      sleep 0.5
      if daemon_health; then DAEMON_UP=yes; break; fi
    done
  else
    sleep 3
    if pgrep -f knocode-daemon >/dev/null 2>&1; then DAEMON_UP=yes; fi
  fi
  if [ "$DAEMON_UP" = yes ]; then ok "knocode daemon RUNNING (http://127.0.0.1:9527, from $INSTALLED_DAEMON)"; else warn "daemon not responding on :9527 within 20s - start manually: $INSTALLED_DAEMON"; fi
fi

info "Done - daemon: $(if [ "$DAEMON_UP" = yes ]; then echo 'RUNNING at http://127.0.0.1:9527'; else echo "NOT running (start: $INSTALLED_DAEMON)"; fi) | agents: $(if [ -n "$AGENT_SEL" ]; then echo "$AGENT_SEL"; else echo none; fi) | knocode doctor"
info "Docs: docs/*.md | knocode doctor"
