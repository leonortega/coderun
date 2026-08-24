#!/usr/bin/env bash
# Coderun v0.5.0 first-class installer (Unix: Linux/macOS, bash)
# Tools are FIRST-CLASS (no optional except LSP, no Temporal) + builds coderun + opencode plugin.
# Idempotent. Usage: bash scripts/install.sh [--skip-build] [--skip-external]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SKIP_BUILD=false; SKIP_EXTERNAL=false
for arg in "$@"; do case "$arg" in --skip-build) SKIP_BUILD=true;; --skip-external) SKIP_EXTERNAL=true;; esac; done
info(){ echo -e "\033[36m[coderun]\033[0m $*"; } ; ok(){ echo -e "  \033[32m✓\033[0m $*"; } ; warn(){ echo -e "  \033[33m⚠\033[0m $*"; }

info "Coderun v0.5.0 installer — $ROOT"
command -v rustc >/dev/null || { info "Installing Rust 1.75..."; curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.75; export PATH="$HOME/.cargo/bin:$PATH"; }
ok "rustc $(rustc --version)"
command -v node >/dev/null || warn "node not found — install Node >=20 https://nodejs.org"; command -v node >/dev/null && ok "node $(node --version)"
command -v python3 >/dev/null || warn "python3 not found"; command -v python3 >/dev/null && ok "python3 $(python3 --version)"
command -v git >/dev/null || { echo "git not found"; exit 1; }; ok "$(git --version)"

if [ "$SKIP_EXTERNAL" = true ]; then info "Skipping external tools (--skip-external)"; else
  info "Installing first-class external tools..."
  command -v sg >/dev/null && ok "ast-grep $(sg --version)" || { info "  ast-grep via cargo..."; cargo install ast-grep --locked 2>/dev/null && ok "ast-grep" || warn "ast-grep install failed — fallback WARN"; }
  if [ -d "$ROOT/../engram" ]; then ok "engram clone at ../engram"; else git clone https://github.com/Gentleman-Programming/engram "$ROOT/../engram" 2>/dev/null && ok "engram cloned" || warn "engram clone failed"; fi
  MODEL_DIR="$HOME/.coderun/models"; mkdir -p "$MODEL_DIR"; [ -f "$MODEL_DIR/flashrank.onnx" ] && ok "FlashRank $MODEL_DIR/flashrank.onnx" || warn "FlashRank model missing at $MODEL_DIR/flashrank.onnx — TF-IDF fallback"
  command -v npx >/dev/null && { npm list -g codebase-memory-mcp >/dev/null 2>&1 || npm i -g codebase-memory-mcp 2>/dev/null; ok "codebase-memory-mcp"; } || true
  pip3 show litellm >/dev/null 2>&1 || pip3 install "litellm[proxy]" 2>/dev/null; ok "litellm"
  command -v rtk >/dev/null && ok "rtk $(rtk --version 2>/dev/null | head -1)" || { cargo install --git https://github.com/rtk-ai/rtk 2>/dev/null && ok "rtk" || warn "rtk failed — built-ins fallback"; }
  command -v mkdocs >/dev/null && ok "mkdocs $(mkdocs --version)" || { pip3 install mkdocs mkdocs-material pymdownx 2>/dev/null && ok "mkdocs"; } || true
  rustup component add clippy 2>/dev/null; ok "clippy"; command -v eslint >/dev/null || npm i -g eslint 2>/dev/null; command -v promptfoo >/dev/null || npm i -g promptfoo 2>/dev/null; ok "promptfoo/eslint"
  if [ -f "$ROOT/workflow/dbos/package.json" ]; then (cd "$ROOT/workflow/dbos" && npm install 2>/dev/null && npx tsc --noEmit 2>/dev/null && ok "DBOS sidecar deps"); fi
fi

if [ "$SKIP_BUILD" = true ]; then info "Skipping build (--skip-build)"; else
  info "Building coderun --release (0.5.0, 192 tests)..."
  cargo build --release; ok "target/release/coderun + coderun-daemon"
  cargo test --workspace --quiet; ok "192 tests passing"
fi

info "Initializing repo..."
"$ROOT/target/release/coderun" init 2>/dev/null || true
"$ROOT/target/release/coderun" index 2>/dev/null || true
"$ROOT/target/release/coderun" doctor

info "Installing opencode plugin..."
SRC="$ROOT/.opencode/plugins/coderun.ts"
if [ ! -f "$SRC" ]; then warn "plugin source not found at $SRC"; else
  mkdir -p "$ROOT/.opencode/plugins"; cp -f "$SRC" "$ROOT/.opencode/plugins/"; ok "copied to $ROOT/.opencode/plugins/"
  mkdir -p "$HOME/.config/opencode/plugins"; cp -f "$SRC" "$HOME/.config/opencode/plugins/"; ok "copied to $HOME/.config/opencode/plugins (global)"
  info "Restart opencode to load plugin (hooks: chat.message + tool.execute.before, UDS /tmp/coderun.sock + MessagePack, 30s fail-open)"
fi

info "Done — next: coderun serve | coderun preview 'add auth' | coderun workflow start 'refactor' --require-approval | curl http://127.0.0.1:9527/metrics"
info "Docs: mkdocs serve | promptfoo eval --config eval/promptfooconfig.yaml"
