#Requires -Version 5.1
<#
.SYNOPSIS
  Coderun v0.5.0 first-class installer (Windows PowerShell 5.1)
  Installs ALL stack tools as first-class (no optional except LSP, no Temporal) + builds coderun + opencode plugin.
  Idempotent — re-run to update.

.DESCRIPTION
  Tools: Rust 1.75, Node >=20, Python+pip, Git, SQLite(bundled), tree-sitter/ripgrep/tantivy/tiktoken embedded,
         ast-grep, engram MCP, FlashRank ort, codebase-memory-mcp, LiteLLM, RTK, MkDocs, analyzers (clippy/eslint), promptfoo, DBOS sidecar

.PARAMETER SkipBuild
  Skip cargo build --release

.PARAMETER SkipExternal
  Skip external tool installs (only build + opencode plugin)

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/install.ps1
  powershell -ExecutionPolicy Bypass -File scripts/install.ps1 -SkipExternal
#>
param([switch]$SkipBuild, [switch]$SkipExternal)

$ErrorActionPreference = "Stop"
$Root = (Resolve-Path "$PSScriptRoot\..").Path
Set-Location $Root

function Test-Cmd($cmd) { $null -ne (Get-Command $cmd -ErrorAction SilentlyContinue) }
function Info($m) { Write-Host "[coderun] $m" -ForegroundColor Cyan }
function Ok($m) { Write-Host "  ✓ $m" -ForegroundColor Green }
function Warn($m) { Write-Host "  ⚠ $m" -ForegroundColor Yellow }
function Fail($m) { Write-Host "  ✗ $m" -ForegroundColor Red; throw $m }

Info "Coderun v0.5.0 installer — $Root"

# 0. Prereqs
if (-not (Test-Cmd rustc)) {
  Info "Installing Rust 1.75 via rustup..."
  Invoke-WebRequest -Uri https://win.rustup.rs/x86_64 -OutFile "$env:TEMP\rustup-init.exe"
  & "$env:TEMP\rustup-init.exe" -y --default-toolchain 1.75
  $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
}
if (-not (Test-Cmd rustc)) { Fail "rustc not found after install" } else { Ok "rustc $(rustc --version)" }

if (-not (Test-Cmd node)) { Warn "node not found — install Node >=20 https://nodejs.org then re-run"; } else { Ok "node $(node --version)" }
if (-not (Test-Cmd python) -and -not (Test-Cmd python3)) { Warn "python not found — install Python 3.11+ for litellm/mkdocs" } else { Ok "python $((python --version 2>&1) -join ' ')" }
if (-not (Test-Cmd git)) { Fail "git not found" } else { Ok "git $(git --version)" }

if ($SkipExternal) { Info "Skipping external tools (--SkipExternal)" }
else {
  Info "Installing first-class external tools..."

  # ast-grep
  if (Test-Cmd sg) { Ok "ast-grep $(sg --version)" } elseif (Test-Cmd cargo) {
    Info "  ast-grep via cargo (sg-core)..."
    try { cargo install ast-grep --locked 2>&1 | Out-Null; Ok "ast-grep installed" } catch { Warn "ast-grep cargo install failed — heuristic fallback will WARN" }
  }

  # engram (Gentleman-Programming/engram) — clone and build
  if (Test-Path "$Root\..\engram") { Ok "engram clone exists at ..\engram" }
  else {
    Info "  engram: git clone + cargo run -- --port 9090 (manual start still needed)"
    try { git clone https://github.com/Gentleman-Programming/engram "$Root\..\engram" 2>&1 | Out-Null; Ok "engram cloned" } catch { Warn "engram clone failed — local LIKE fallback" }
  }

  # FlashRank ort model
  $modelDir = "$env:USERPROFILE\.coderun\models"
  New-Item -ItemType Directory -Force -Path $modelDir | Out-Null
  $modelPath = "$modelDir\flashrank.onnx"
  if (Test-Path $modelPath) { Ok "FlashRank model $modelPath" }
  else { Warn "FlashRank model not found at $modelPath — download rank-T5-flan int8 ONNX manually; TF-IDF fallback until then" }

  # codebase-memory-mcp
  if (Test-Cmd npx) {
    try { npm list -g codebase-memory-mcp 2>&1 | Out-Null; if ($LASTEXITCODE -ne 0) { npm i -g codebase-memory-mcp 2>&1 | Out-Null }; Ok "codebase-memory-mcp $(npm list -g codebase-memory-mcp 2>&1 | Select-String codebase-memory-mcp)" } catch { Warn "codebase-memory-mcp npm install failed" }
  }

  # LiteLLM proxy
  if (Test-Cmd pip) { try { pip show litellm 2>&1 | Out-Null; if ($LASTEXITCODE -ne 0) { pip install "litellm[proxy]" 2>&1 | Out-Null }; Ok "litellm pip" } catch { Warn "litellm pip install failed" } }

  # RTK
  if (Test-Cmd rtk) { Ok "rtk $(rtk --version 2>&1 | Select-Object -First 1)" }
  else { try { cargo install --git https://github.com/rtk-ai/rtk 2>&1 | Out-Null; Ok "rtk installed via cargo git" } catch { Warn "rtk cargo git install failed — built-ins fallback" } }

  # MkDocs
  if (Test-Cmd mkdocs) { Ok "mkdocs $(mkdocs --version)" } else { try { pip install mkdocs mkdocs-material pymdownx 2>&1 | Out-Null; Ok "mkdocs pip" } catch { Warn "mkdocs pip failed" } }

  # analyzers
  try { rustup component add clippy 2>&1 | Out-Null; Ok "clippy" } catch {}
  if (Test-Cmd npm) { try { npm list -g eslint 2>&1 | Out-Null; if ($LASTEXITCODE -ne 0) { npm i -g eslint 2>&1 | Out-Null }; Ok "eslint" } catch {} }

  # promptfoo
  if (Test-Cmd promptfoo) { Ok "promptfoo $(promptfoo --version)" } else { try { npm i -g promptfoo 2>&1 | Out-Null; Ok "promptfoo" } catch { Warn "promptfoo npm install failed" } }

  # DBOS sidecar deps
  if (Test-Path "$Root\workflow\dbos\package.json") {
    Push-Location "$Root\workflow\dbos"
    try { npm install 2>&1 | Out-Null; npx tsc --noEmit 2>&1 | Out-Null; Ok "DBOS sidecar deps" } catch { Warn "DBOS npm install failed" }
    Pop-Location
  }
}

# 1. Build coderun
if ($SkipBuild) { Info "Skipping build (--SkipBuild)" }
else {
  Info "Building coderun --release (0.5.0, 192 tests)..."
  cargo build --release; if ($LASTEXITCODE -ne 0) { Fail "cargo build failed" }
  Ok "target/release/coderun.exe + coderun-daemon.exe"
  cargo test --workspace --quiet
  if ($LASTEXITCODE -ne 0) { Warn "cargo test had failures — see above" } else { Ok "192 tests passing" }
}

# 2. init + index + doctor
Info "Initializing repo..."
& ".\target\release\coderun.exe" init 2>&1 | Out-Null
& ".\target\release\coderun.exe" index 2>&1 | Out-Null
& ".\target\release\coderun.exe" doctor

# 3. opencode plugin
Info "Installing opencode plugin..."
$src = "$Root\.opencode\plugins\coderun.ts"
if (-not (Test-Path $src)) { Warn "plugin source not found at $src" }
else {
  $dest1 = "$Root\.opencode\plugins"
  New-Item -ItemType Directory -Force -Path $dest1 | Out-Null
  Copy-Item $src $dest1 -Force; Ok "copied to $dest1"
  # global fallback
  $globalDest = "$env:USERPROFILE\.config\opencode\plugins"
  New-Item -ItemType Directory -Force -Path $globalDest | Out-Null
  Copy-Item $src $globalDest -Force; Ok "copied to $globalDest (global)"
  Info "Restart opencode to load plugin (hooks: chat.message + tool.execute.before, UDS /tmp/coderun.sock + MessagePack, 30s fail-open)"
}

Info "Done — next: coderun serve  |  coderun preview 'add auth'  |  coderun workflow start 'refactor' --require-approval  |  curl http://127.0.0.1:9527/metrics"
Info "Docs: mkdocs serve  |  promptfoo eval --config eval/promptfooconfig.yaml"
