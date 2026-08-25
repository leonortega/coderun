#!/usr/bin/env bash
# Coderun compile script (Unix: Linux/macOS, bash)
# Builds coderun binaries from source. Installer assumes pre-compiled and does NOT build.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
RELEASE=true; SKIP_TESTS=false; FEATURES=""
for arg in "$@"; do case "$arg" in
  --debug) RELEASE=false ;;
  --skip-tests) SKIP_TESTS=true ;;
  --features) FEATURES="$2"; shift ;;
  --features=*) FEATURES="${arg#--features=}" ;;
  -h|--help) echo "Usage: $0 [--debug] [--skip-tests] [--features FEATS]"; echo "  --debug        Build debug (default: release)"; echo "  --skip-tests   Skip cargo test"; echo "  --features     Cargo features (e.g. extended-languages)"; exit 0 ;;
esac; done
info(){ echo -e "\033[36m[coderun]\033[0m $*"; } ; ok(){ echo -e "  \033[32m[OK]\033[0m $*"; } ; warn(){ echo -e "  \033[33m[WARN]\033[0m $*"; }
command -v cargo >/dev/null || { echo "cargo not found"; exit 1; }
command -v rustc >/dev/null || { echo "rustc not found"; exit 1; }
info "Compiling coderun ($([ "$RELEASE" = true ] && echo release || echo debug)) from $ROOT ..."
if [ -n "$FEATURES" ]; then info "Features: $FEATURES"; fi
if [ "$RELEASE" = true ]; then cargo build --release ${FEATURES:+--features "$FEATURES"}; ok "target/release/coderun + coderun-daemon"; else cargo build ${FEATURES:+--features "$FEATURES"}; ok "target/debug/coderun + coderun-daemon"; fi
if [ "$SKIP_TESTS" = true ]; then info "Skipping tests (--skip-tests)"; else info "cargo test --workspace --quiet ..."; cargo test --workspace --quiet ${FEATURES:+--features "$FEATURES"} && ok "tests passing" || warn "cargo test had failures"; fi
info "Compile done. Next: bash scripts/install.sh  (uses pre-compiled binary, no rebuild)"
