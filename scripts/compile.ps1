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
Info "cargo build $($buildArgs -join ' ') ..."
& cargo build @buildArgs
if ($LASTEXITCODE -ne 0) { Fail "cargo build failed" }
if ($NoRelease) {
  Ok "target/debug/coderun.exe + coderun-daemon.exe"
} else {
  Ok "target/release/coderun.exe + coderun-daemon.exe"
}

# Tests
if ($SkipTests) {
  Info "Skipping tests (--SkipTests)"
} else {
  $testArgs = @("--workspace", "--quiet")
  if ($Features) { $testArgs += "--features"; $testArgs += $Features }
  Info "cargo test $($testArgs -join ' ') ..."
  & cargo test @testArgs
  if ($LASTEXITCODE -ne 0) { Warn "cargo test had failures - see above" } else { Ok "tests passing (add -Features extended-languages for go,java,c,cpp)" }
}

Info "Compile done. Next: powershell -ExecutionPolicy Bypass -File scripts/install.ps1  (uses pre-compiled binary, no rebuild)"
