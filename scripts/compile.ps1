#Requires -Version 5.1
<#
.SYNOPSIS
  Coderun compile script (Windows PowerShell 5.1)
  Builds coderun binaries from source. Installer assumes pre-compiled binaries and does NOT build.

.DESCRIPTION
  Compiles coderun and coderun-daemon in release mode and runs workspace tests.
  Use this when you need to (re)build after source changes. The installer (scripts/install.ps1)
  intentionally does not compile and will use existing target/release/coderun.exe if present.

.PARAMETER Release
  Build in --release mode (default). Use -NoRelease for debug build.

.PARAMETER SkipTests
  Skip cargo test after build.

.PARAMETER Features
  Cargo features to enable (default: none). Use "extended-languages" for go,java,c,cpp parsers.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/compile.ps1
  powershell -ExecutionPolicy Bypass -File scripts/compile.ps1 -SkipTests
  powershell -ExecutionPolicy Bypass -File scripts/compile.ps1 -Features extended-languages
#>
param(
  [switch]$NoRelease,
  [switch]$SkipTests,
  [string]$Features = ""
)

$ErrorActionPreference = "Stop"
$Root = (Resolve-Path "$PSScriptRoot\..").Path
Set-Location $Root

function Info($m) { Write-Host "[coderun] $m" -ForegroundColor Cyan }
function Ok($m) { Write-Host "  [OK] $m" -ForegroundColor Green }
function Warn($m) { Write-Host "  [WARN] $m" -ForegroundColor Yellow }
function Fail($m) { Write-Host "  [FAIL] $m" -ForegroundColor Red; throw $m }

$mode = if ($NoRelease) { "debug" } else { "release" }
$buildArgs = @()
if (-not $NoRelease) { $buildArgs += "--release" }
if ($Features) { $buildArgs += "--features"; $buildArgs += $Features }

Info "Compiling coderun ($mode) from $Root ..."
if ($Features) { Info "Features: $Features" }

# Ensure Rust is available
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { Fail "cargo not found - install Rust https://rustup.rs" }
if (-not (Get-Command rustc -ErrorAction SilentlyContinue)) { Fail "rustc not found" }
Ok "rustc $(rustc --version)"
Ok "cargo $(cargo --version)"

# Build
# Stop a running daemon/cli first - a locked target/*/coderun-daemon.exe makes cargo's
# relink fail with "Access denied" (os error 5). Restart is up to the caller (install.ps1).
$running = Get-Process -Name coderun-daemon, coderun -ErrorAction SilentlyContinue
if ($running) {
  Warn "stopping running coderun processes (lock target binaries): $($running.Id -join ', ')"
  $running | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Milliseconds 800
}

Info "cargo build $($buildArgs -join ' ') ..."
& cargo build @buildArgs
if ($LASTEXITCODE -ne 0) { Fail "cargo build failed" }

# Ensure binaries end up in $Root/target/release/ (or debug/) so the installer can find them.
# Cargo may use a global target dir (e.g. ~/.cargo/target) when CARGO_TARGET_DIR or
# [build] target is set in .cargo/config.toml. Detect via cargo metadata and copy if needed.
$expectedDir = Join-Path $Root "target\$mode"
if ($NoRelease) { $expectedDir = Join-Path $Root "target\debug" }
$coderunExpected = Join-Path $expectedDir "coderun.exe"
if (-not (Test-Path $coderunExpected)) {
  try {
    $metaJson = & cargo metadata --no-deps --format-version 1 2>$null | Out-String
    if ($LASTEXITCODE -eq 0 -and $metaJson) {
      $cargoTargetDir = ($metaJson | ConvertFrom-Json).target_directory
      if ($cargoTargetDir -and (Test-Path $cargoTargetDir)) {
        $cargoModeDir = Join-Path $cargoTargetDir $mode
        $srcCoderun = Join-Path $cargoModeDir "coderun.exe"
        $srcDaemon = Join-Path $cargoModeDir "coderun-daemon.exe"
        if (Test-Path $srcCoderun) {
          New-Item -ItemType Directory -Force -Path $expectedDir | Out-Null
          Copy-Item -LiteralPath $srcCoderun -Destination $expectedDir -Force
          if (Test-Path $srcDaemon) { Copy-Item -LiteralPath $srcDaemon -Destination $expectedDir -Force }
          Info "Copied binaries from cargo target dir ($cargoModeDir) -> $expectedDir"
        }
      }
    }
  } catch {}
}

if ($NoRelease) {
  Ok "target/debug/coderun.exe + coderun-daemon.exe"
} else {
  Ok "target/release/coderun.exe + coderun-daemon.exe"
}

# Tests
# NOTE: the suite is fully HERMETIC - no test requires a live daemon, engram server,
# LiteLLM, or any network service. `cargo test --workspace` passes on a cold machine.
if ($SkipTests) {
  Info "Skipping tests (--SkipTests)"
} else {
  $testArgs = @("--workspace", "--quiet")
  if ($Features) { $testArgs += "--features"; $testArgs += $Features }
  Info "cargo test $($testArgs -join ' ') ..."
  & cargo test @testArgs
  if ($LASTEXITCODE -ne 0) { Warn "cargo test had failures - see above" } else { Ok "tests passing (add -Features extended-languages for go,java,c,cpp)" }
}

# --- opencode-coderun npm plugin ---
$npmCmd = Get-Command npm -ErrorAction SilentlyContinue
$pluginDir = Join-Path $Root "packages/opencode-coderun"
if ($npmCmd) {
  if (Test-Path $pluginDir) {
    Info "Building npm plugin packages/opencode-coderun ..."
    Push-Location $pluginDir
    try {
      $hasLock = Test-Path (Join-Path $pluginDir "package-lock.json")
      if ($hasLock) {
        & npm ci --silent
      } else {
        & npm install --silent
      }
      if ($LASTEXITCODE -ne 0) { Warn "npm install failed for opencode-coderun - see above" }
      else {
        & npm run build --silent
        if ($LASTEXITCODE -ne 0) { Warn "opencode-coderun build failed" }
        else {
          Ok "opencode-coderun dist built"
          if ($SkipTests) {
            Info "Skipping opencode-coderun tests (--SkipTests)"
          } else {
            & npm test --silent
            if ($LASTEXITCODE -ne 0) { Warn "opencode-coderun tests had failures - see above" } else { Ok "opencode-coderun tests passing" }
          }
        }
      }
    } finally { Pop-Location }
  } else {
    Warn "packages/opencode-coderun not found - skipping npm build"
  }
} else {
  Warn "npm not found - skipping opencode-coderun build (install Node.js 18+)"
}

Info "Compile done. Next: powershell -ExecutionPolicy Bypass -File scripts/install.ps1  (uses pre-compiled binary, no rebuild)"
