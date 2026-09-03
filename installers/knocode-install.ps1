#Requires -Version 5.1
<#
.SYNOPSIS
  Knocode end-user installer (Windows x64) - installs a prebuilt GitHub release.

.DESCRIPTION
  Downloads knocode-<ver>-x86_64-pc-windows-msvc.zip from the matching GitHub
  Release (latest by default, or pinned via -Version) and installs
  knocode.exe + knocode-daemon.exe into %USERPROFILE%\.knocode\bin, then ensures
  that directory is on the USER PATH (idempotent).

  This is the LEAN end-user installer for prebuilt releases. It does not install
  Rust, Python, Node, or eval tooling and it does not build from source. For a
  full developer environment from a source checkout, use scripts/install.ps1.

  Latest release (one-liner):
    powershell -ExecutionPolicy Bypass -c "irm https://github.com/leonortega/knocode/releases/latest/download/knocode-install.ps1 | iex"

  Pinned version (download the script and pass -Version):
    powershell -ExecutionPolicy Bypass -File knocode-install.ps1 -Version 0.9.0

.PARAMETER Version
  Release version to install, e.g. "0.9.0" (leading "v" is optional).
  Defaults to the latest GitHub release.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File knocode-install.ps1
  powershell -ExecutionPolicy Bypass -File knocode-install.ps1 -Version 0.9.0
#>
param([string]$Version = "")

$ErrorActionPreference = "Stop"
$Repo = "leonortega/knocode"

function Write-Step($m) { Write-Host "[knocode] $m" -ForegroundColor Cyan }
function Write-Ok($m) { Write-Host "  [OK] $m" -ForegroundColor Green }
function Write-Warn($m) { Write-Host "  [WARN] $m" -ForegroundColor Yellow }
function Fail($m) { Write-Host "  [FAIL] $m" -ForegroundColor Red; throw $m }

Write-Step "Knocode installer (prebuilt release)"

# 1. Resolve the release tag (default: latest from the GitHub API)
$tag = ""
if ($Version -ne "") {
    $tag = if ($Version.StartsWith("v")) { $Version } else { "v$Version" }
}
else {
    Write-Step "Resolving latest release..."
    try {
        $rel = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers @{ "User-Agent" = "knocode-installer" } -UseBasicParsing
        $tag = $rel.tag_name
    }
    catch { Fail "could not resolve latest release from https://api.github.com/repos/$Repo/releases/latest ($($_.Exception.Message))" }
}
$ver = $tag.TrimStart("v")
if ($ver -notmatch "^\d+\.\d+\.\d+") { Fail "invalid release tag '$tag'" }
Write-Step "Installing knocode $ver"

# 2. Architecture guard - releases are built for Windows x64 only
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -eq "x86") {
    # 32-bit process on 64-bit Windows (WOW64)
    if ($env:PROCESSOR_ARCHITEW6432 -eq "AMD64") { $arch = "AMD64" }
}
if ($arch -ne "AMD64") { Fail "unsupported architecture '$arch' - knocode releases are built for Windows x64 (AMD64) only" }

# 3. Stop running daemon/CLI up front - later steps REPLACE the binaries and a
#    locked exe would fail the copy.
foreach ($procName in @("knocode-daemon", "knocode")) {
    Get-Process -Name $procName -ErrorAction SilentlyContinue | ForEach-Object {
        try { Stop-Process -Id $_.Id -Force -ErrorAction Stop; Write-Step "stopped $procName PID $($_.Id)" } catch { }
    }
}

# 4. Download the release archive, extract it, and install into %USERPROFILE%\.knocode\bin
$asset = "knocode-$ver-x86_64-pc-windows-msvc.zip"
$url = "https://github.com/$Repo/releases/download/$tag/$asset"
Write-Step "Downloading $url"
$tmp = Join-Path $env:TEMP ("knocode_install_" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
try {
    try { [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12 } catch { }
    $zip = Join-Path $tmp $asset
    Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
    if (-not (Test-Path -LiteralPath $zip)) { Fail "download failed: $url" }

    $extract = Join-Path $tmp "x"
    Expand-Archive -LiteralPath $zip -DestinationPath $extract -Force
    $cliSrc = Get-ChildItem -LiteralPath $extract -Recurse -Filter "knocode.exe" | Select-Object -First 1
    if (-not $cliSrc) { Fail "knocode.exe not found in $asset (broken release archive)" }
    $daemonSrc = Get-ChildItem -LiteralPath $extract -Recurse -Filter "knocode-daemon.exe" | Select-Object -First 1

    # 5. Copy the binaries into place
    $binDir = Join-Path $env:USERPROFILE ".knocode\bin"
    New-Item -ItemType Directory -Force -Path $binDir | Out-Null
    $installedCli = Join-Path $binDir "knocode.exe"
    Copy-Item -LiteralPath $cliSrc.FullName -Destination $installedCli -Force
    Write-Ok "knocode.exe $ver installed to $installedCli"
    if ($daemonSrc) {
        Copy-Item -LiteralPath $daemonSrc.FullName -Destination (Join-Path $binDir "knocode-daemon.exe") -Force
        Write-Ok "knocode-daemon.exe installed to $binDir\knocode-daemon.exe"
    }
    else { Write-Warn "knocode-daemon.exe missing from $asset - daemon features unavailable" }
}
finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

# 6. Ensure $binDir is on the USER PATH (HKCU Environment) - append only when missing
$binDir = Join-Path $env:USERPROFILE ".knocode\bin"
$installedCli = Join-Path $binDir "knocode.exe"
try {
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($null -eq $userPath) { $userPath = "" }
    $entries = $userPath -split ";" | Where-Object { $_ -ne "" }
    if ($entries -notcontains $binDir) {
        [Environment]::SetEnvironmentVariable("Path", (($entries + $binDir) -join ";"), "User")
        Write-Step "Added $binDir to USER PATH - open a new terminal before using 'knocode'"
    }
    else { Write-Ok "$binDir already on USER PATH" }
}
catch { Write-Warn "could not persist USER PATH: $($_.Exception.Message) - add $binDir manually" }
if (($env:Path -split ";") -notcontains $binDir) { $env:Path = "$binDir;$env:Path" }

# 7. Verify
Write-Step "Verifying installation..."
try {
    & $installedCli --version
    Write-Ok "installed to $installedCli"
}
catch { Write-Warn "knocode.exe failed to run: $($_.Exception.Message)" }

Write-Step "Next steps: open a new terminal, then run 'knocode init' inside a project and 'knocode serve'."
Write-Step "Docs: https://github.com/$Repo#readme"
