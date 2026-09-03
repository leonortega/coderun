#!/usr/bin/env bash
# Knocode installer v0.8.0 minimal (Unix: Linux/macOS, bash)
# Minimal v1: Tree-sitter+Tantivy+SQLite+Git + ast-grep/RTK optional; promptfoo deferred per V1_MINIMAL_STACK_PLAN.md:2
# Idempotent. Usage: bash scripts/install.sh [--skip-build] [--skip-external] [--with-optional]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SKIP_BUILD=false; SKIP_EXTERNAL=false; WITH_OPTIONAL=false
for arg in "$@"; do case "$arg" in --skip-build) SKIP_BUILD=true;; --skip-external) SKIP_EXTERNAL=true;; --with-optional) WITH_OPTIONAL=true;; esac; done
info(){ echo -e "\033[36m[knocode]\033[0m $*"; } ; ok(){ echo -e "  \033[32m[OK]\033[0m $*"; } ; warn(){ echo -e "  \033[33m[WARN]\033[0m $*"; }

info "Knocode installer"

# 0a. Stop any running daemon/CLI up front - later steps REPLACE binaries (~/.knocode/bin)
# and a locked exe would fail the copy. The fresh daemon is restarted at the end (step 4).
for p in knocode-daemon knocode; do
  if pgrep -x "$p" >/dev/null 2>&1; then pkill -x "$p" 2>/dev/null || true; ok "stopped $p"; fi
done

# Rust/rustup kept for the clippy analyzer (RTK now ships prebuilt - no cargo build needed)
NEED_RUST=false
if ! command -v rustc >/dev/null 2>&1; then NEED_RUST=true; fi
if [ "$NEED_RUST" = true ]; then
  info "Installing Rust stable via rustup (for clippy analyzer)..."
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
  else warn "python3 not found - install Python 3.11+ https://www.python.org/downloads/ (required for promptfoo)"; fi
else command -v python3 >/dev/null 2>&1 && ok "python3 $(python3 --version)" || ok "python $(python --version)"; fi
command -v git >/dev/null || { echo "git not found"; exit 1; }; ok "$(git --version)"

if [ "$SKIP_EXTERNAL" = true ]; then info "Skipping external tools (--skip-external)"; else
  info "Installing first-class external tools..."
  command -v ast-grep >/dev/null && ok "ast-grep $(ast-grep --version)" || { info "  ast-grep via npm (@ast-grep/cli, prebuilt)..."; npm i -g @ast-grep/cli 2>/dev/null && ok "ast-grep installed" || warn "ast-grep npm install failed - fallback WARN"; }
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
   rustup component add clippy 2>/dev/null; ok "clippy"; command -v eslint >/dev/null || npm i -g eslint 2>/dev/null; command -v promptfoo >/dev/null || npm i -g promptfoo 2>/dev/null; if command -v promptfoo >/dev/null; then ok "promptfoo $(promptfoo --version 2>/dev/null | head -1)"; else warn "promptfoo install failed - try: npm i -g promptfoo"; fi; ok "eslint/promptfoo check"

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

# 3. opencode plugin (GLOBAL: ~/.config/opencode - loaded in EVERY project)
OC_GLOBAL="$HOME/.config/opencode"
OC_GLOBAL_CFG="$OC_GLOBAL/opencode.jsonc"
ROOT_OPENCODE="$ROOT/.opencode"   # legacy project dir - cleaned below
info "Configuring opencode plugin (global ~/.config/opencode)..."
mkdir -p "$OC_GLOBAL"
# Global config: plugin only
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
for f in "$ROOT_OPENCODE/opencode.jsonc" "$ROOT_OPENCODE/opencode.json" "$ROOT_OPENCODE/package.json" "$ROOT_OPENCODE/package-lock.json"; do
  if [ -f "$f" ]; then rm -f "$f" 2>/dev/null && info "Removed legacy project $(basename "$f") (MCPs/plugin are global now)" || true; fi
done
# Ensure npm plugin is built
if [ -d "$ROOT/packages/opencode-knocode" ]; then
  if [ ! -f "$ROOT/packages/opencode-knocode/dist/index.js" ]; then
    if command -v npm >/dev/null 2>&1; then
      info "Building opencode-knocode npm package..."
      (cd "$ROOT/packages/opencode-knocode" && npm install --silent 2>/dev/null && npm run build --silent 2>/dev/null && ok "opencode-knocode built to packages/opencode-knocode/dist") || warn "opencode-knocode build failed - run: cd packages/opencode-knocode && npm install && npm run build"
    else
      warn "npm not found - cannot build opencode-knocode (install Node.js 18+)"
    fi
  else
    ok "opencode-knocode dist at packages/opencode-knocode/dist/index.js"
  fi
  # Install npm plugin GLOBALLY (~/.config/opencode/node_modules) via file: reference to this repo
  if command -v npm >/dev/null 2>&1; then
    info "Installing opencode-knocode globally (~/.config/opencode)..."
    PKG_JSON="$OC_GLOBAL/package.json"
    PLUGIN_REF="file:$ROOT/packages/opencode-knocode"
    if [ ! -f "$PKG_JSON" ]; then
      printf '%s\n' '{' '  "dependencies": {' '    "@opencode-ai/plugin": "1.18.22",' "    \"opencode-knocode\": \"$PLUGIN_REF\"" '  }' '}' > "$PKG_JSON"
    elif command -v node >/dev/null 2>&1; then
      # Merge + always refresh the file: ref (repo may have moved)
      PKG_JSON_PATH="$PKG_JSON" PLUGIN_REF="$PLUGIN_REF" node -e "const fs=require('fs');const p=process.env.PKG_JSON_PATH;let j={};try{j=JSON.parse(fs.readFileSync(p,'utf8'))}catch(e){};j.dependencies=j.dependencies||{};j.dependencies['opencode-knocode']=process.env.PLUGIN_REF;j.dependencies['@opencode-ai/plugin']=j.dependencies['@opencode-ai/plugin']||'1.18.22';fs.writeFileSync(p,JSON.stringify(j,null,2))" 2>/dev/null || true
    fi
    (cd "$OC_GLOBAL" && npm install --silent 2>/dev/null && [ -f node_modules/opencode-knocode/dist/index.js ] && ok "opencode-knocode installed to ~/.config/opencode/node_modules (global)") || warn "opencode-knocode npm install failed - try: cd ~/.config/opencode && npm install"
  else
    warn "npm not found - skipping global opencode plugin install (install Node.js 18+)"
  fi
else
  warn "packages/opencode-knocode not found - skipping npm plugin install"
fi
# Knocode agent skill (opencode - agent-native discovery; per-agent: opencode is the only supported agent for now)
# Global ~/.config/opencode/skills/<name>/SKILL.md applies to EVERY project (same pattern as the plugin).
OC_SKILL_SRC="$ROOT/.opencode/skills/knocode"
if [ -f "$OC_SKILL_SRC/SKILL.md" ]; then
  mkdir -p "$OC_GLOBAL/skills" && cp -rf "$OC_SKILL_SRC" "$OC_GLOBAL/skills/" 2>/dev/null && ok "knocode skill installed to $OC_GLOBAL/skills/knocode (opencode agent-native)" || warn "knocode skill copy failed (source: $OC_SKILL_SRC)"
else
  warn ".opencode/skills/knocode not found - skipping agent skill install"
fi
info "Restart opencode to load global plugin 'opencode-knocode' (hooks: chat.message + message.updated + tool.execute.before, daemon http://127.0.0.1:9527). Plugin loads in EVERY project (global ~/.config/opencode)."

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

info "Done - daemon: $(if [ "$DAEMON_UP" = yes ]; then echo 'RUNNING at http://127.0.0.1:9527'; else echo "NOT running (start: $INSTALLED_DAEMON)"; fi) | knocode preview 'add auth' | curl http://127.0.0.1:9527/metrics | knocode doctor"
info "Docs: docs/*.md plain | promptfoo eval --config eval/promptfooconfig.yaml (optional: --with-optional) | knocode doctor"
