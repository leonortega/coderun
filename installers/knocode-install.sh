#!/usr/bin/env bash
# Knocode end-user installer (Linux x64 / macOS arm64) - installs a prebuilt GitHub release
#
# Downloads the matching prebuilt archive from the latest GitHub Release (or pinned
# via --version) and installs knocode + knocode-daemon into ~/.knocode/bin, then
# ensures that directory is on PATH.
#
# Prerequisites are installed automatically when missing (unless --skip-prereqs):
#   - Git - required by the runtime (commit-mode repo watching).
#   - Python 3.11+ - required by the runtime.
#   - Node.js LTS - required only when agent integrations are selected.
#   - RTK (prebuilt from GitHub releases) - optional external tool.
#
# Agent integrations (OpenCode / Codex / Copilot / Cursor) are optional and
# selected interactively. They use the integration bundles shipped inside the
# release archive - no npm registry needed.
#
# One-liner:
#   curl -fsSL https://leonortega.github.io/knocode/install.sh | bash
#
# Pinned version:
#   curl -fsSL https://leonortega.github.io/knocode/install.sh | bash -s -- --version 0.9.7
set -euo pipefail

REPO="leonortega/knocode"
VERSION=""
AGENTS=""
ALL_AGENTS=false
NO_AGENTS=false
SKIP_PREREQS=false

for arg in "$@"; do case "$arg" in
  --version) VERSION="$2"; shift;;
  --version=*) VERSION="${arg#--version=}";;
  --agents) AGENTS="$2"; shift;;
  --agents=*) AGENTS="${arg#--agents=}";;
  --all-agents) ALL_AGENTS=true;;
  --no-agents) NO_AGENTS=true;;
  --skip-prereqs) SKIP_PREREQS=true;;
  -h|--help) echo "Usage: $0 [--version X.Y.Z] [--agents a,b,c|--all-agents|--no-agents] [--skip-prereqs]"; exit 0;;
esac; done

info() { echo -e "\033[36m[knocode]\033[0m $*"; }
ok()   { echo -e "  \033[32m[OK]\033[0m $*"; }
warn() { echo -e "  \033[33m[WARN]\033[0m $*"; }
skip() { echo -e "  \033[90m[SKIP]\033[0m $*"; }
fail() { echo -e "  \033[31m[FAIL]\033[0m $*" >&2; exit 1; }

# ── Agent catalog & selection ─────────────────────────────────────────────
AGENT_CATALOG="opencode codex copilot cursor"
select_agents() {
  if [ "$NO_AGENTS" = true ]; then echo ""; return; fi
  if [ -n "$AGENTS" ]; then
    local sel=""
    IFS=',' read -ra parts <<< "$AGENTS"
    for a in "${parts[@]}"; do
      a="$(echo "$a" | tr '[:upper:]' '[:lower:]' | xargs)"
      case " $AGENT_CATALOG " in *" $a "*) sel="$sel $a";; *) warn "unknown agent '$a' - valid: opencode, codex, copilot, cursor";; esac
    done
    if [ -z "$sel" ]; then fail "no valid agents in --agents ('$AGENTS')"; fi
    echo "$sel"; return
  fi
  if [ "$ALL_AGENTS" = true ]; then echo "$AGENT_CATALOG"; return; fi
  # Interactive multi-select when stdin is a terminal; default to NONE otherwise
  if [ ! -t 0 ]; then
    info "non-interactive run - no agent integrations installed (use --agents opencode,cursor or --all-agents to change)"
    echo ""; return
  fi
  local sel=""
  for a in $AGENT_CATALOG; do
    printf "  Wire up %s? [y/N] " "$a"
    read -r r
    case "$r" in y|Y|yes|YES) sel="$sel $a";; *) skip "$a skipped";; esac
  done
  echo "$sel"
}

info "Knocode installer (prebuilt release)"
AGENT_SEL="$(select_agents)"
if [ -n "$AGENT_SEL" ]; then info "Agent integrations:$AGENT_SEL"; else info "Agent integrations: none"; fi

# ── Architecture detection ────────────────────────────────────────────────
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
case "$OS:$ARCH" in
  linux:x86_64)  ASSET_SUFFIX="x86_64-unknown-linux-gnu";;
  linux:aarch64) ASSET_SUFFIX="aarch64-unknown-linux-gnu";;
  darwin:arm64|darwin:aarch64) ASSET_SUFFIX="aarch64-apple-darwin";;
  darwin:x86_64) ASSET_SUFFIX="x86_64-apple-darwin";;
  *) fail "unsupported platform $OS/$ARCH - knocode releases are built for Linux x64, macOS arm64/x64";;
esac

# ── Resolve version ───────────────────────────────────────────────────────
if [ -n "$VERSION" ]; then
  TAG="$(echo "$VERSION" | sed 's/^v//')"
  TAG="v${TAG#v}"
else
  info "Resolving latest release..."
  TAG="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed 's/.*"tag_name": *"//;s/".*//' || true)"
  if [ -z "$TAG" ]; then fail "could not resolve latest release from https://api.github.com/repos/$REPO/releases/latest"; fi
fi
VER="${TAG#v}"
if ! echo "$VER" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+'; then fail "invalid release tag '$TAG'"; fi
info "Installing knocode $VER"

# ── Stop running daemon/CLI ───────────────────────────────────────────────
for p in knocode-daemon knocode; do
  if pgrep -x "$p" >/dev/null 2>&1; then pkill -x "$p" 2>/dev/null || true; ok "stopped $p"; fi
done

# ── Download and extract ──────────────────────────────────────────────────
ASSET="knocode-${VER}-${ASSET_SUFFIX}.zip"
URL="https://github.com/$REPO/releases/download/$TAG/$ASSET"
TMP="$(mktemp -d 2>/dev/null || echo "$HOME/.cache/tmp/knocode_install")"
mkdir -p "$TMP"
intsDst=""

cleanup() { rm -rf "$TMP" 2>/dev/null || true; }
trap cleanup EXIT

info "Downloading $URL"
if ! curl -fsSL "$URL" -o "$TMP/$ASSET" 2>/dev/null; then
  fail "download failed: $URL"
fi

# Verify sha256 if sidecar exists (fail-open for older releases)
if curl -fsSL "$URL.sha256" -o "$TMP/$ASSET.sha256" 2>/dev/null; then
  expected="$(awk '{print $1}' "$TMP/$ASSET.sha256" | tr '[:upper:]' '[:lower:]')"
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$TMP/$ASSET" | awk '{print $1}')"
  elif command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$TMP/$ASSET" | awk '{print $1}')"
  else
    actual=""
  fi
  if [ -n "$actual" ] && [ "$actual" != "$expected" ]; then
    fail "checksum mismatch for $ASSET (expected $expected, got $actual)"
  fi
  if [ -n "$actual" ]; then ok "sha256 verified ($expected)"; fi
else
  warn "sha256 sidecar unavailable - skipping verification"
fi

mkdir -p "$TMP/extract"
unzip -qo "$TMP/$ASSET" -d "$TMP/extract" 2>/dev/null || fail "failed to extract $ASSET"

CLI_SRC="$(find "$TMP/extract" -name 'knocode' -type f 2>/dev/null | head -1)"
DAEMON_SRC="$(find "$TMP/extract" -name 'knocode-daemon' -type f 2>/dev/null | head -1)"
if [ -z "$CLI_SRC" ] || [ ! -f "$CLI_SRC" ]; then fail "knocode binary not found in $ASSET (broken release archive)"; fi

# ── Install binaries to ~/.knocode/bin ────────────────────────────────────
BIN_DIR="$HOME/.knocode/bin"
mkdir -p "$BIN_DIR"
INSTALLED_CLI="$BIN_DIR/knocode"
cp -f "$CLI_SRC" "$INSTALLED_CLI" && chmod +x "$INSTALLED_CLI"
ok "knocode $VER installed to $INSTALLED_CLI"

INSTALLED_DAEMON="$BIN_DIR/knocode-daemon"
if [ -n "$DAEMON_SRC" ] && [ -f "$DAEMON_SRC" ]; then
  cp -f "$DAEMON_SRC" "$INSTALLED_DAEMON" && chmod +x "$INSTALLED_DAEMON"
  ok "knocode-daemon installed to $INSTALLED_DAEMON"
else
  warn "knocode-daemon not found in $ASSET - daemon features unavailable"
fi

# Install bundled agent integration packages
INTS_SRC="$TMP/extract/integrations"
if [ -d "$INTS_SRC" ]; then
  intsDst="$HOME/.knocode/integrations"
  mkdir -p "$intsDst"
  cp -rf "$INTS_SRC/"* "$intsDst/" 2>/dev/null
  ok "agent integration bundles installed to $intsDst"
else
  warn "no bundled integrations in $ASSET - agent wiring will be unavailable"
fi

# Install the knocode agent skill (opencode — agent-native discovery)
SKILL_SRC="$TMP/extract/skills/knocode"
if [ -f "$SKILL_SRC/SKILL.md" ]; then
  OC_SKILL_DST="$HOME/.config/opencode/skills/knocode"
  mkdir -p "$(dirname "$OC_SKILL_DST")"
  cp -rf "$SKILL_SRC" "$OC_SKILL_DST"
  ok "knocode skill installed to $OC_SKILL_DST (opencode agent-native)"
else
  warn "knocode skill not found in $ASSET - skipping skill install"
fi

# ── Persist on PATH ───────────────────────────────────────────────────────
case ":$PATH:" in *":$BIN_DIR:"*) ;; *) export PATH="$BIN_DIR:$PATH" ;; esac
for rc in "$HOME/.profile" "$HOME/.bashrc" "$HOME/.zshrc"; do
  if [ -f "$rc" ]; then
    grep -qs "KNOCODE_BIN_PATH" "$rc" || printf '\n# KNOCODE_BIN_PATH: knocode AI runtime CLI + daemon\nexport PATH="$HOME/.knocode/bin:$PATH"\n' >> "$rc" && ok "PATH entry ensured in $rc"
  fi
done

# ── Verify binaries ───────────────────────────────────────────────────────
info "Verifying installation..."
if "$INSTALLED_CLI" --version 2>/dev/null; then
  ok "installed to $INSTALLED_CLI"
else
  warn "knocode failed to run"
fi

# ── Prerequisites ─────────────────────────────────────────────────────────
if [ "$SKIP_PREREQS" = true ]; then
  info "Skipping prerequisite installs (--skip-prereqs)"
else
  # Git
  if ! command -v git >/dev/null 2>&1; then
    info "Installing git..."
    if command -v apt-get >/dev/null 2>&1; then sudo apt-get update -qq && sudo apt-get install -y git 2>/dev/null && ok "$(git --version)" || warn "git install failed - install manually: https://git-scm.com"
    elif command -v brew >/dev/null 2>&1; then brew install git 2>/dev/null && ok "$(git --version)" || warn "git install failed"
    elif command -v dnf >/dev/null 2>&1; then sudo dnf install -y git 2>/dev/null && ok "$(git --version)" || warn "git install failed"
    elif command -v pacman >/dev/null 2>&1; then sudo pacman -S --noconfirm git 2>/dev/null && ok "$(git --version)" || warn "git install failed"
    else warn "git not found - install manually: https://git-scm.com"; fi
  else ok "$(git --version)"; fi

  # Python 3.11+
  if ! command -v python3 >/dev/null 2>&1 && ! command -v python >/dev/null 2>&1; then
    info "python3 not found - attempting install..."
    if command -v apt-get >/dev/null 2>&1; then sudo apt-get update -qq && sudo apt-get install -y python3 python3-pip 2>/dev/null && ok "python3 $(python3 --version)" || warn "python3 install failed - install manually: https://www.python.org/downloads/"
    elif command -v brew >/dev/null 2>&1; then brew install python@3.13 2>/dev/null && ok "python3 $(python3 --version)" || warn "python3 install failed"
    elif command -v dnf >/dev/null 2>&1; then sudo dnf install -y python3 python3-pip 2>/dev/null && ok "python3 $(python3 --version)" || warn "python3 install failed"
    elif command -v pacman >/dev/null 2>&1; then sudo pacman -S --noconfirm python python-pip 2>/dev/null && ok "python3 $(python3 --version)" || warn "python3 install failed"
    else warn "python3 not found - install manually: https://www.python.org/downloads/"; fi
  else
    PYCMD="$(command -v python3 2>/dev/null || command -v python 2>/dev/null)"
    PYVER="$($PYCMD --version 2>&1 | grep -oE '[0-9]+\.[0-9]+')"
    PYMAJOR="${PYVER%%.*}"
    if [ "$PYMAJOR" -ge 3 ]; then ok "python $PYVER"
    else warn "python found but version < 3.11 - install manually: https://www.python.org/downloads/"; fi
  fi

  # Node.js LTS (required for agent integrations)
  if ! command -v node >/dev/null 2>&1; then
    info "node not found - attempting install..."
    if command -v apt-get >/dev/null 2>&1; then sudo apt-get update -qq && sudo apt-get install -y nodejs npm 2>/dev/null && ok "node $(node --version)" || warn "node install failed - install manually: https://nodejs.org"
    elif command -v brew >/dev/null 2>&1; then brew install node 2>/dev/null && ok "node $(node --version)" || warn "node install failed"
    elif command -v dnf >/dev/null 2>&1; then sudo dnf install -y nodejs npm 2>/dev/null && ok "node $(node --version)" || warn "node install failed"
    elif command -v pacman >/dev/null 2>&1; then sudo pacman -S --noconfirm nodejs npm 2>/dev/null && ok "node $(node --version)" || warn "node install failed"
    else warn "no package manager for node - install manually: https://nodejs.org"; fi
  else ok "node $(node --version)"; fi
fi

# ── RTK (optional external tool) ──────────────────────────────────────────
RTK_BIN="$HOME/.knocode/bin/rtk"
if command -v rtk >/dev/null 2>&1; then
  ok "rtk $(rtk --version 2>/dev/null | head -1)"
elif [ -f "$RTK_BIN" ]; then
  ok "rtk binary at $RTK_BIN"
else
  RTK_ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
  case "$OS:$RTK_ARCH" in
    linux:x86_64|linux:amd64) RTK_ASSET="rtk-x86_64-unknown-linux-musl.tar.gz";;
    linux:aarch64|linux:arm64) RTK_ASSET="rtk-aarch64-unknown-linux-gnu.tar.gz";;
    darwin:x86_64) RTK_ASSET="rtk-x86_64-apple-darwin.tar.gz";;
    darwin:aarch64|darwin:arm64) RTK_ASSET="rtk-aarch64-apple-darwin.tar.gz";;
    *) RTK_ASSET="";;
  esac
  if [ -z "$RTK_ASSET" ]; then
    warn "rtk: unsupported platform ($OS/$RTK_ARCH) - install manually from https://github.com/rtk-ai/rtk/releases"
  else
    RTK_URL="https://github.com/rtk-ai/rtk/releases/latest/download/$RTK_ASSET"
    RTK_TMP="$(mktemp -d 2>/dev/null || echo "$HOME/.cache/tmp/rtk_dl")"
    mkdir -p "$RTK_TMP"
    info "  downloading rtk release ($RTK_ASSET)..."
    if curl -fsSL "$RTK_URL" -o "$RTK_TMP/$RTK_ASSET" 2>/dev/null; then
      tar -xzf "$RTK_TMP/$RTK_ASSET" -C "$RTK_TMP" 2>/dev/null
      RTK_SRC="$(find "$RTK_TMP" -name rtk -type f 2>/dev/null | head -1 || true)"
      if [ -n "$RTK_SRC" ] && [ -f "$RTK_SRC" ]; then
        mkdir -p "$(dirname "$RTK_BIN")"
        cp -f "$RTK_SRC" "$RTK_BIN" && chmod +x "$RTK_BIN"
        ok "rtk installed to $RTK_BIN (from GitHub release)"
      else
        warn "rtk release archive did not contain the rtk binary"
      fi
    else
      warn "rtk download failed - install manually from https://github.com/rtk-ai/rtk/releases"
    fi
    rm -rf "$RTK_TMP" 2>/dev/null
  fi
fi

# ── Agent integrations ────────────────────────────────────────────────────
if [ -n "$AGENT_SEL" ]; then
  info "Wiring agent integrations:$AGENT_SEL"

  if [ -z "$intsDst" ] || [ ! -d "$intsDst" ]; then
    warn "integration bundles not installed - agent wiring skipped"
  else
    # Node.js check (required for all agent integrations)
    if ! command -v node >/dev/null 2>&1; then
      if [ "$SKIP_PREREQS" = true ]; then
        warn "Node.js is required for agent integrations and --skip-prereqs was set - agents skipped"
        AGENT_SEL=""
      else
        warn "Node.js not found - agent integrations skipped (install Node from https://nodejs.org)"
        AGENT_SEL=""
      fi
    fi

    # --- OpenCode ---
    if echo "$AGENT_SEL" | grep -qw opencode; then
      OC_GLOBAL="$HOME/.config/opencode"
      PLUGIN_SRC="$intsDst/opencode-knocode"
      if [ -f "$PLUGIN_SRC/dist/index.js" ]; then
        mkdir -p "$OC_GLOBAL/node_modules"
        cp -rf "$PLUGIN_SRC" "$OC_GLOBAL/node_modules/"
        OC_CFG="$OC_GLOBAL/opencode.jsonc"
        if [ ! -f "$OC_CFG" ] || ! grep -q "opencode-knocode" "$OC_CFG" 2>/dev/null; then
          cat > "$OC_CFG" <<'OCEOF'
{
    "$schema": "https://opencode.ai/config.json",
    "plugin": ["opencode-knocode"]
}
OCEOF
        fi
        ok "opencode plugin installed (bundled opencode-knocode)"
        info "Restart opencode to load the plugin (daemon http://127.0.0.1:9527)"
      else
        warn "bundled opencode-knocode has no dist/index.js"
      fi
    fi

    # --- Shared MCP server descriptor ---
    MCP_DIST="$intsDst/knocode-mcp/dist/index.js"
    if [ ! -f "$MCP_DIST" ]; then
      warn "bundled knocode-mcp has no dist/index.js - MCP agents skipped"
    fi

    # --- Codex ---
    if echo "$AGENT_SEL" | grep -qw codex && [ -f "$MCP_DIST" ]; then
      CODEX_DIR="$HOME/.codex"
      CODEX_CFG="$CODEX_DIR/config.toml"
      mkdir -p "$CODEX_DIR"
      if [ ! -f "$CODEX_CFG" ] || ! grep -q "mcp_servers.knocode" "$CODEX_CFG" 2>/dev/null; then
        printf '\n[mcp_servers.knocode]\ncommand = "node"\nargs = ["%s"]\n' "$MCP_DIST" >> "$CODEX_CFG"
        ok "Codex MCP config at $CODEX_CFG"
      else
        ok "Codex MCP already configured at $CODEX_CFG"
      fi
    fi

    # --- Copilot (VS Code) ---
    if echo "$AGENT_SEL" | grep -qw copilot && [ -f "$MCP_DIST" ]; then
      CODE_USER_DIR="$HOME/.config/Code/User"
      VSCODE_MCP="$CODE_USER_DIR/mcp.json"
      mkdir -p "$CODE_USER_DIR"
      if [ ! -f "$VSCODE_MCP" ]; then
        cat > "$VSCODE_MCP" <<MCPJSON
{
  "servers": {
    "knocode": { "command": "node", "args": ["$MCP_DIST"], "env": { "KNOCODE_DAEMON_URL": "http://127.0.0.1:9527" } }
  }
}
MCPJSON
        ok "VS Code Copilot MCP at $VSCODE_MCP"
      else
        if command -v node >/dev/null 2>&1; then
          node -e "
            const fs=require('fs');const p=process.argv[1];
            let j={};try{j=JSON.parse(fs.readFileSync(p,'utf8'))}catch(e){};
            j.servers=j.servers||{};
            j.servers.knocode={command:'node',args:[process.argv[2]],env:{KNOCODE_DAEMON_URL:'http://127.0.0.1:9527'}};
            fs.writeFileSync(p,JSON.stringify(j,null,2));
          " "$VSCODE_MCP" "$MCP_DIST" 2>/dev/null && ok "VS Code Copilot MCP updated at $VSCODE_MCP" || skip "VS Code mcp.json exists but could not merge knocode"
        else
          skip "VS Code mcp.json exists but could not merge knocode (node missing)"
        fi
      fi
    fi

    # --- Cursor ---
    if echo "$AGENT_SEL" | grep -qw cursor && [ -f "$MCP_DIST" ]; then
      CURSOR_DIR="$HOME/.cursor"
      CURSOR_MCP="$CURSOR_DIR/mcp.json"
      mkdir -p "$CURSOR_DIR"
      if [ ! -f "$CURSOR_MCP" ]; then
        cat > "$CURSOR_MCP" <<MCPJSON
{
  "mcpServers": {
    "knocode": { "command": "node", "args": ["$MCP_DIST"], "env": { "KNOCODE_DAEMON_URL": "http://127.0.0.1:9527" } }
  }
}
MCPJSON
        ok "Cursor MCP at $CURSOR_MCP"
      else
        if command -v node >/dev/null 2>&1; then
          node -e "
            const fs=require('fs');const p=process.argv[1];
            let j={};try{j=JSON.parse(fs.readFileSync(p,'utf8'))}catch(e){};
            j.mcpServers=j.mcpServers||{};
            j.mcpServers.knocode={command:'node',args:[process.argv[2]],env:{KNOCODE_DAEMON_URL:'http://127.0.0.1:9527'}};
            fs.writeFileSync(p,JSON.stringify(j,null,2));
          " "$CURSOR_MCP" "$MCP_DIST" 2>/dev/null && ok "Cursor MCP updated at $CURSOR_MCP" || skip "Cursor mcp.json exists but could not merge knocode"
        else
          skip "Cursor mcp.json exists but could not merge knocode (node missing)"
        fi
      fi
    fi

    if [ -n "$AGENT_SEL" ]; then
      info "Agent integrations wired:$AGENT_SEL"
    fi
  fi
fi

# ── Start daemon ──────────────────────────────────────────────────────────
daemon_health() { curl -s -o /dev/null -m 2 http://127.0.0.1:9527/health; }
DAEMON_UP=no
if command -v curl >/dev/null 2>&1 && daemon_health; then
  DAEMON_UP=yes
  ok "knocode daemon already running at http://127.0.0.1:9527"
elif [ ! -x "$INSTALLED_DAEMON" ]; then
  warn "knocode-daemon not found at $INSTALLED_DAEMON - start manually"
else
  # Stop stale processes
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
  if [ "$DAEMON_UP" = yes ]; then
    ok "knocode daemon RUNNING (http://127.0.0.1:9527)"
  else
    warn "daemon not responding on :9527 within 20s - start manually: $INSTALLED_DAEMON"
  fi
fi

info "Done - daemon: $(if [ "$DAEMON_UP" = yes ]; then echo 'RUNNING at http://127.0.0.1:9527'; else echo "NOT running (start: $INSTALLED_DAEMON)"; fi) | agents: $(if [ -n "$AGENT_SEL" ]; then echo "$AGENT_SEL"; else echo none; fi)"
info "Next steps: open a new terminal, run 'knocode init' inside a project."
info "Docs: https://github.com/$REPO#readme"
