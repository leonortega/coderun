#Requires -Version 5.1
<#
.SYNOPSIS
  Coderun first-class installer (Windows PowerShell 5.1)
  Installs ALL stack tools as first-class + uses prebuilt coderun (no compile/test).
  Idempotent - re-run to update.

.DESCRIPTION
  Tools: Rust, Node >=20, Python+pip, Git, SQLite(bundled), tree-sitter/ripgrep/tantivy/tiktoken embedded,
         ast-grep, engram, LiteLLM, RTK, MkDocs, analyzers (clippy/eslint), promptfoo
  Prebuilt: target/release/coderun.exe + coderun-daemon.exe are used directly (no cargo build/test).

.PARAMETER SkipBuild
  Deprecated - build is always skipped (prebuilt binary at target/release/coderun.exe is used). Kept for compat.

.PARAMETER SkipExternal
  Skip external tool installs (only config + doctor)

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/install.ps1
  powershell -ExecutionPolicy Bypass -File scripts/install.ps1 -SkipExternal
#>
param([switch]$SkipBuild, [switch]$SkipExternal)

$ErrorActionPreference = "Stop"
# Always English in scripts (avoid localized ShouldProcess/WhatIf)
try { [System.Threading.Thread]::CurrentThread.CurrentUICulture = [System.Globalization.CultureInfo]::GetCultureInfo('en-US'); [System.Threading.Thread]::CurrentThread.CurrentCulture = [System.Globalization.CultureInfo]::GetCultureInfo('en-US') } catch {}
$Root = (Resolve-Path "$PSScriptRoot\..").Path
Set-Location $Root

function Test-Cmd($cmd) { $null -ne (Get-Command $cmd -ErrorAction SilentlyContinue) }
function Info($m) { Write-Host "[coderun] $m" -ForegroundColor Cyan }
function Ok($m) { Write-Host "  [OK] $m" -ForegroundColor Green }
function Warn($m) { Write-Host "  [WARN] $m" -ForegroundColor Yellow }
function Fail($m) { Write-Host "  [FAIL] $m" -ForegroundColor Red; throw $m }

Info "Coderun installer"

# 0a. Stop any running daemon/CLI up front - later steps REPLACE binaries (~\.coderun\bin)
# and a locked exe would fail the copy. The fresh daemon is restarted at the end (step 4).
foreach ($procName in @("coderun-daemon", "coderun")) {
  Get-Process -Name $procName -ErrorAction SilentlyContinue | ForEach-Object {
    try { Stop-Process -Id $_.Id -Force -ErrorAction Stop; Info "stopped $procName PID $($_.Id)" } catch {}
  }
}

# 0. Prereqs - Rust 1.85+ required for RTK (edition2024)
$needRustInstall = $false
$rustVerStr = ""
if (-not (Test-Cmd rustc)) { $needRustInstall = $true }
else {
  try {
    $rustVerStr = (rustc --version 2>&1 | Out-String).Trim()
    if ($rustVerStr -match "rustc 1\.(\d+)\.") {
      $minor = [int]$Matches[1]
      if ($minor -lt 85) { Info "Rust $rustVerStr too old for RTK (needs 1.85+ for edition2024), upgrading..."; $needRustInstall = $true }
    }
  } catch {}
}
if ($needRustInstall) {
  Info "Installing Rust stable via rustup..."
  try {
    if (Test-Cmd rustup) { rustup update stable 2>&1 | Out-Null; rustup default stable 2>&1 | Out-Null; Ok "rustc $(rustc --version) (updated)" }
    else {
      Invoke-WebRequest -Uri https://win.rustup.rs/x86_64 -OutFile "$env:TEMP\rustup-init.exe" -UseBasicParsing
      & "$env:TEMP\rustup-init.exe" -y --default-toolchain stable 2>&1 | Out-Null
      $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
    }
  } catch { Warn "rustup update failed: $_" }
}
if (-not (Test-Cmd rustc)) { Fail "rustc not found after install" } else { Ok "rustc $(rustc --version)" }

if (-not (Test-Cmd node)) { Warn "node not found - install Node >=20 https://nodejs.org then re-run"; } else { Ok "node $(node --version)" }
if (-not (Test-Cmd python) -and -not (Test-Cmd python3)) {
  Info "python not found - installing Python 3.13..."
  try {
    if (Get-Command winget -ErrorAction SilentlyContinue) {
      winget install --id Python.Python.3.13 -e --accept-package-agreements --accept-source-agreements --silent 2>&1 | Out-Null
      $pyPaths = @("$env:LOCALAPPDATA\Programs\Python\Python313\python.exe", "$env:LOCALAPPDATA\Programs\Python\Python313\Scripts\python.exe", "C:\Python313\python.exe")
      foreach ($p in $pyPaths) { if (Test-Path $p) { $env:Path = "$(Split-Path $p -Parent);$(Split-Path $p -Parent)\Scripts;$env:Path"; break } }
    } else {
      # Fallback: download official installer
      $pyUrl = "https://www.python.org/ftp/python/3.13.2/python-3.13.2-amd64.exe"
      $pyInst = "$env:TEMP\python-3.13.2-amd64.exe"
      Invoke-WebRequest -Uri $pyUrl -OutFile $pyInst -UseBasicParsing
      & $pyInst /quiet InstallAllUsers=0 PrependPath=1 Include_test=0 2>&1 | Out-Null
      Start-Sleep -Seconds 5
      $env:Path = "$env:LOCALAPPDATA\Programs\Python\Python313\Scripts;$env:LOCALAPPDATA\Programs\Python\Python313;$env:Path"
    }
    # Refresh command cache
    if (Test-Cmd python -or Test-Cmd python3) { Ok "python $((python --version 2>&1) -join ' ')" } else { Warn "python install attempted but python not on PATH - install manually: https://www.python.org/downloads/ (check 'Add to PATH')" }
  } catch { Warn "python auto-install failed - $_ (install manually: https://www.python.org/downloads/)" }
} else { Ok "python $((python --version 2>&1) -join ' ')" }
if (-not (Test-Cmd git)) { Fail "git not found" } else { Ok "git $(git --version)" }

if ($SkipExternal) { Info "Skipping external tools (--SkipExternal)" }
else {
  Info "Installing first-class external tools..."

  # ast-grep (npm @ast-grep/cli - PREBUILT, fast; provides ast-grep + sg commands)
  if (Test-Cmd ast-grep) { Ok "ast-grep $(ast-grep --version)" }
  elseif (Test-Cmd npm) {
    Info "  ast-grep via npm (@ast-grep/cli, prebuilt)..."
    try {
      & npm i -g "@ast-grep/cli" *>&1 | Out-Null
      $npmPrefix = $null; try { $npmPrefix = (& npm prefix -g 2>$null | Out-String).Trim() } catch {}
      if ($npmPrefix -and (Test-Path $npmPrefix)) { $env:Path = "$npmPrefix;$env:Path" }
      if ((Test-Cmd ast-grep) -or (Test-Cmd sg)) { Ok "ast-grep installed (npm @ast-grep/cli)" }
      else { Warn "ast-grep npm install did not put 'ast-grep'/'sg' on PATH - heuristic fallback will WARN" }
    } catch { Warn "ast-grep npm install failed - heuristic fallback will WARN" }
  }
  else { Warn "npm not found - cannot install ast-grep (heuristic fallback will WARN)" }

  # engram - local binary from .coderun/engram/*.zip (no git clone, no external repo)
  $engramBinPath = "$env:USERPROFILE\bin\engram.exe"
  $engramInstalled = $false
  if (Test-Cmd engram) { Ok "engram $(engram --version 2>&1 | Select-Object -First 1)"; $engramInstalled = $true }
  elseif (Test-Path $engramBinPath) { Ok "engram binary at $engramBinPath"; $engramInstalled = $true }
  if (-not $engramInstalled) {
    $repoEngramZip = Get-ChildItem -LiteralPath "$Root\.coderun\engram" -Filter "*.zip" -ErrorAction SilentlyContinue | Select-Object -First 1
    # also support already-extracted binary in repo
    $repoEngramExe = Get-ChildItem -LiteralPath "$Root\.coderun\engram" -Filter "engram.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($repoEngramExe -and (Test-Path $repoEngramExe.FullName)) {
      try {
        New-Item -ItemType Directory -Force -Path (Split-Path $engramBinPath -Parent) | Out-Null
        Copy-Item -LiteralPath $repoEngramExe.FullName -Destination $engramBinPath -Force
        Ok "engram installed to $engramBinPath (from .coderun\engram\engram.exe)"
        $engramInstalled = $true
      } catch { Warn "engram copy from repo failed: $_ - local LIKE fallback" }
    } elseif ($repoEngramZip) {
      try {
        New-Item -ItemType Directory -Force -Path (Split-Path $engramBinPath -Parent) | Out-Null
        $tmpExtract = Join-Path $env:TEMP "engram_extract"
        if (Test-Path $tmpExtract) { Remove-Item -LiteralPath $tmpExtract -Recurse -Force -ErrorAction SilentlyContinue }
        Expand-Archive -LiteralPath $repoEngramZip.FullName -DestinationPath $tmpExtract -Force
        $srcExe = Get-ChildItem -LiteralPath $tmpExtract -Recurse -Filter "engram.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($srcExe) {
          Copy-Item -LiteralPath $srcExe.FullName -Destination $engramBinPath -Force
          Ok "engram installed to $engramBinPath (from $($repoEngramZip.Name))"
          $engramInstalled = $true
        } else { Warn "engram zip did not contain engram.exe - local LIKE fallback" }
      } catch { Warn "engram install from zip failed: $_ - local LIKE fallback" }
    } else {
      Warn "engram zip not found at .coderun\engram\*.zip - local LIKE fallback"
    }
  }
  if (Test-Path "$Root\..\engram") { Ok "legacy engram clone at ..\engram (unused, local binary preferred - can be removed)" }
  # Show engram detail if installed
  if ($engramInstalled -and (Test-Path $engramBinPath)) {
    try { $engramVer = & $engramBinPath --version 2>&1 | Select-Object -First 1; Ok "engram detail: $engramVer at $engramBinPath" } catch {}
  }

  # FlashRank removed from v1 runtime per benchmark evaluation (see rerank.rs)

  # codebase-memory-mcp - extract from bundled zip or download from GitHub releases
  $cbmBinPath = "$env:USERPROFILE\bin\codebase-memory-mcp.exe"
  $cbmBinNoExt = "$env:USERPROFILE\bin\codebase-memory-mcp"
  $cbmInstalled = $false
  if (Test-Cmd codebase-memory-mcp) { Ok "codebase-memory-mcp $(codebase-memory-mcp --version 2>&1 | Select-Object -First 1)"; $cbmInstalled = $true }
  elseif (Test-Path $cbmBinPath) { Ok "codebase-memory-mcp binary at $cbmBinPath"; $cbmInstalled = $true }
  if (-not $cbmInstalled) {
    # Try bundled zip first
    $cbmZip = Get-ChildItem -LiteralPath "$Root\.coderun\codebase-memory" -Filter "*.zip" -ErrorAction SilentlyContinue | Select-Object -First 1
    $cbmExe = Get-ChildItem -LiteralPath "$Root\.coderun\codebase-memory" -Filter "codebase-memory-mcp.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($cbmExe -and (Test-Path $cbmExe.FullName)) {
      try {
        New-Item -ItemType Directory -Force -Path (Split-Path $cbmBinPath -Parent) | Out-Null
        Copy-Item -LiteralPath $cbmExe.FullName -Destination $cbmBinPath -Force
        Copy-Item -LiteralPath $cbmExe.FullName -Destination $cbmBinNoExt -Force
        Ok "codebase-memory-mcp installed (from .coderun\codebase-memory binary)"
        $cbmInstalled = $true
      } catch { Warn "codebase-memory-mcp copy failed: $_" }
    } elseif ($cbmZip) {
      try {
        New-Item -ItemType Directory -Force -Path (Split-Path $cbmBinPath -Parent) | Out-Null
        $tmpExtract = Join-Path $env:TEMP "cbm_extract"
        if (Test-Path $tmpExtract) { Remove-Item -LiteralPath $tmpExtract -Recurse -Force -ErrorAction SilentlyContinue }
        Expand-Archive -LiteralPath $cbmZip.FullName -DestinationPath $tmpExtract -Force
        $srcExe = Get-ChildItem -LiteralPath $tmpExtract -Recurse -Filter "codebase-memory-mcp.exe" | Select-Object -First 1
        if ($srcExe) {
          Copy-Item -LiteralPath $srcExe.FullName -Destination $cbmBinPath -Force
          Copy-Item -LiteralPath $srcExe.FullName -Destination $cbmBinNoExt -Force
          Ok "codebase-memory-mcp installed (from $($cbmZip.Name))"
          $cbmInstalled = $true
        } else { Warn "codebase-memory-mcp zip did not contain codebase-memory-mcp.exe" }
        Remove-Item -LiteralPath $tmpExtract -Recurse -Force -ErrorAction SilentlyContinue
      } catch { Warn "codebase-memory-mcp extraction failed: $_" }
    }
    # Fallback: download from GitHub releases
    if (-not $cbmInstalled) {
      try {
        Info "  Downloading codebase-memory-mcp from GitHub..."
        New-Item -ItemType Directory -Force -Path (Split-Path $cbmBinPath -Parent) | Out-Null
        $cbmUrl = "https://github.com/nicholasgasior/codebase-memory-mcp/releases/latest/download/codebase-memory-mcp-windows-amd64.zip"
        $cbmZipPath = Join-Path $env:TEMP "codebase-memory-mcp.zip"
        Invoke-WebRequest -Uri $cbmUrl -OutFile $cbmZipPath -UseBasicParsing
        $tmpExtract = Join-Path $env:TEMP "cbm_download_extract"
        if (Test-Path $tmpExtract) { Remove-Item -LiteralPath $tmpExtract -Recurse -Force -ErrorAction SilentlyContinue }
        Expand-Archive -LiteralPath $cbmZipPath -DestinationPath $tmpExtract -Force
        $srcExe = Get-ChildItem -LiteralPath $tmpExtract -Recurse -Filter "codebase-memory-mcp.exe" | Select-Object -First 1
        if ($srcExe) {
          Copy-Item -LiteralPath $srcExe.FullName -Destination $cbmBinPath -Force
          Copy-Item -LiteralPath $srcExe.FullName -Destination $cbmBinNoExt -Force
          Ok "codebase-memory-mcp downloaded and installed"
          $cbmInstalled = $true
        } else { Warn "codebase-memory-mcp download zip did not contain binary" }
        Remove-Item -LiteralPath $tmpExtract -Recurse -Force -ErrorAction SilentlyContinue
      } catch { Warn "codebase-memory-mcp download failed: $_" }
    }
    if (-not $cbmInstalled) { Warn "codebase-memory-mcp could not be installed" }
  }

  # LiteLLM proxy
  if (Test-Cmd pip) { try { pip show litellm 2>&1 | Out-Null; if ($LASTEXITCODE -ne 0) { pip install "litellm[proxy]" 2>&1 | Out-Null }; Ok "litellm pip" } catch { Warn "litellm pip install failed" } }

  # RTK - extract from .coderun\rtk\*.zip (Windows) -> ~\bin\rtk.exe (NO COMPILE). Cargo source build only as last resort.
  $rtkBinPath = "$env:USERPROFILE\bin\rtk.exe"
  if (Test-Cmd rtk) { Ok "rtk $(rtk --version 2>&1 | Select-Object -First 1)" }
  elseif (Test-Path $rtkBinPath) { $env:Path = "$(Split-Path $rtkBinPath -Parent);$env:Path"; Ok "rtk binary at $rtkBinPath" }
  else {
    $repoZip = Get-ChildItem -LiteralPath "$Root\.coderun\rtk" -Filter "*.zip" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($repoZip -and (Test-Path $repoZip.FullName)) {
      try {
        New-Item -ItemType Directory -Force -Path (Split-Path $rtkBinPath -Parent) | Out-Null
        $tmpExtract = Join-Path $env:TEMP "rtk_extract"
        if (Test-Path $tmpExtract) { Remove-Item -LiteralPath $tmpExtract -Recurse -Force -ErrorAction SilentlyContinue }
        Expand-Archive -LiteralPath $repoZip.FullName -DestinationPath $tmpExtract -Force
        $srcExe = Get-ChildItem -LiteralPath $tmpExtract -Recurse -Filter "rtk.exe" | Select-Object -First 1
        if ($srcExe) {
          Copy-Item -LiteralPath $srcExe.FullName -Destination $rtkBinPath -Force
          $env:Path = "$(Split-Path $rtkBinPath -Parent);$env:Path"
          Ok "rtk installed to $rtkBinPath (from $($repoZip.Name))"
        } else { Warn "rtk zip did not contain rtk.exe" }
        Remove-Item -LiteralPath $tmpExtract -Recurse -Force -ErrorAction SilentlyContinue
      } catch { Warn "rtk extraction failed: $_" }
    }
  }
  if (-not ((Test-Cmd rtk) -or (Test-Path $rtkBinPath))) {
    $rtkOk = $false
    # Long compile - stream output LIVE (silence looks like a freeze). EAP relaxed locally: under Stop, redirected cargo stderr becomes terminating
    $rtkPrevEap = $ErrorActionPreference; $ErrorActionPreference = "Continue"
    try {
      Info "  No prebuilt rtk found - building via cargo... FIRST BUILD TAKES SEVERAL MINUTES (not frozen)"
      # Ensure Rust is up to date for edition2024
      if (Test-Cmd rustup) { & rustup update stable; & rustup default stable }
      & cargo install --git https://github.com/rtk-ai/rtk --locked
      if ($LASTEXITCODE -eq 0 -and (Test-Cmd rtk)) { Ok "rtk $(rtk --version 2>&1 | Select-Object -First 1) (cargo git)"; $rtkOk = $true }
    } catch {}
    if (-not $rtkOk) {
      try { & cargo install rtk --locked; if ($LASTEXITCODE -eq 0 -and (Test-Cmd rtk)) { Ok "rtk $(rtk --version 2>&1 | Select-Object -First 1) (crates.io)"; $rtkOk = $true } } catch {}
    }
    $ErrorActionPreference = $rtkPrevEap
    if (-not $rtkOk) { Warn "rtk cargo install failed - built-ins fallback (or drop a prebuilt rtk.exe into .coderun\rtk\)" }
  }

  # MkDocs (docs) - pymdownx is provided by pymdown-extensions
  $mkdocsOk = $false
  $prevEA = $ErrorActionPreference; $ErrorActionPreference = "Continue"
  if (Test-Cmd mkdocs) { try { $v = mkdocs --version 2>&1 | Out-String; if ($v -match "mkdocs") { Ok "mkdocs $($v.Trim())"; $mkdocsOk = $true } } catch {} }
  if (-not $mkdocsOk) {
    try {
      $v = & python -m mkdocs --version 2>&1 | Out-String
      if ($LASTEXITCODE -eq 0 -and $v -match "mkdocs") { Ok "mkdocs $($v.Trim()) (python -m)"; $mkdocsOk = $true }
    } catch {}
  }
  if (-not $mkdocsOk) {
    Info "  Installing mkdocs (pymdown-extensions provides pymdownx)..."
    try { & python -m pip install --upgrade pip *>&1 | Out-Null } catch {}
    # WindowsApps pip can have corrupted mkdocs metadata (ValueError) - force reinstall
    try { & python -m pip uninstall -y mkdocs *>&1 | Out-Null } catch {}
    $ec = 1
    try {
      if (Test-Cmd python) { & python -m pip install --user --force-reinstall --no-cache mkdocs mkdocs-material pymdown-extensions *>&1 | Out-Null; $ec = $LASTEXITCODE }
      elseif (Test-Cmd pip) { pip install --user --force-reinstall --no-cache mkdocs mkdocs-material pymdown-extensions *>&1 | Out-Null; $ec = $LASTEXITCODE }
      elseif (Test-Cmd pip3) { pip3 install --user --force-reinstall --no-cache mkdocs mkdocs-material pymdown-extensions *>&1 | Out-Null; $ec = $LASTEXITCODE }
    } catch {}
    # Ensure Scripts on PATH for this session (WindowsApps + AppData)
    $scriptPaths = @("$env:APPDATA\Python\Scripts", "$env:LOCALAPPDATA\Programs\Python\Python313\Scripts", "$env:LOCALAPPDATA\Programs\Python\Python313\Scripts", "$env:LOCALAPPDATA\Packages\PythonSoftwareFoundation.Python.3.13_qbz5n2kfra8p0\LocalCache\local-packages\Python313\Scripts")
    foreach ($sp in $scriptPaths) {
      $resolved = $null
      try { $resolved = Resolve-Path -LiteralPath $sp -ErrorAction SilentlyContinue } catch {}
      if ($resolved) { $env:Path = "$($resolved.Path);$env:Path" }
      elseif (Test-Path $sp) { $env:Path = "$sp;$env:Path" }
    }
    # Also try wildcard for Packages
    foreach ($p in Get-ChildItem -LiteralPath "$env:LOCALAPPDATA\Packages" -Directory -ErrorAction SilentlyContinue | Where-Object Name -like "PythonSoftwareFoundation.Python.3.13*") {
      $cand = Join-Path $p.FullName "LocalCache\local-packages\Python313\Scripts"
      if (Test-Path $cand) { $env:Path = "$cand;$env:Path" }
    }
    # re-check after install
    if (Test-Cmd mkdocs) { try { $v = mkdocs --version 2>&1 | Out-String; if ($v -match "mkdocs") { Ok "mkdocs $($v.Trim())"; $mkdocsOk = $true } } catch {} }
    if (-not $mkdocsOk) {
      try { $v2 = & python -m mkdocs --version 2>&1 | Out-String; if ($LASTEXITCODE -eq 0 -and $v2 -match "mkdocs") { Ok "mkdocs $($v2.Trim()) (python -m)"; $mkdocsOk = $true } } catch {}
    }
    if (-not $mkdocsOk) {
      # Fallback: check via importlib (pip show can be corrupted on WindowsApps)
      try {
        $v3 = & python -c "import importlib.metadata; print(importlib.metadata.version('mkdocs'))" 2>&1 | Out-String
        $v3t = $v3.Trim()
        if ($LASTEXITCODE -eq 0 -and $v3t -and $v3t -ne "None" -and $v3t -match "^\d") { Ok "mkdocs $v3t (pip)"; $mkdocsOk = $true }
        elseif ($LASTEXITCODE -eq 0 -and $v3t) { Ok "mkdocs installed (pip)"; $mkdocsOk = $true }
      } catch {}
    }
    if (-not $mkdocsOk) { Warn "mkdocs install failed - try: python -m pip install --user --force-reinstall mkdocs mkdocs-material pymdown-extensions (ensure Python Scripts on PATH)" }
  }
  $ErrorActionPreference = $prevEA

  # analyzers
  try { rustup component add clippy 2>&1 | Out-Null; Ok "clippy" } catch {}
  if (Test-Cmd npm) { try { npm list -g eslint 2>&1 | Out-Null; if ($LASTEXITCODE -ne 0) { npm i -g eslint 2>&1 | Out-Null }; Ok "eslint" } catch {} }

  # promptfoo (eval) - suppress Node ExperimentalWarning
  $prevEA2 = $ErrorActionPreference; $ErrorActionPreference = "Continue"
  $env:NODE_NO_WARNINGS = "1"
  $promptOk = $false
  if (Test-Cmd promptfoo) {
    try {
      $raw = & promptfoo --version 2>&1 | Out-String
      $v = ($raw -split "`n" | Where-Object { $_ -match "^\s*\d+\.\d+" } | ForEach-Object { $_.Trim() } | Select-Object -First 1)
      if ($v) { Ok "promptfoo $v"; $promptOk = $true } else { Ok "promptfoo installed"; $promptOk = $true }
    } catch { Ok "promptfoo installed"; $promptOk = $true }
  }
  if (-not $promptOk) {
    try {
      Info "  Installing promptfoo..."
      npm i -g promptfoo *>&1 | Out-Null
      if (Test-Cmd promptfoo) {
        $raw = & promptfoo --version 2>&1 | Out-String
        $v = ($raw -split "`n" | Where-Object { $_ -match "^\s*\d+\.\d+" } | ForEach-Object { $_.Trim() } | Select-Object -First 1)
        if ($v) { Ok "promptfoo $v" } else { Ok "promptfoo installed" }
      } else { Info "  promptfoo not installed (optional - for eval: npm i -g promptfoo)" }
    } catch { Info "  promptfoo not installed (optional - for eval: npm i -g promptfoo) - $_" }
  }
  $ErrorActionPreference = $prevEA2



# 1. Use prebuilt coderun (no compile/test - use repository binary)
if ($SkipBuild) { Info "Skipping build check (--SkipBuild)" }
Info "Checking prebuilt coderun..."
$prebuilt = Join-Path $Root "target\release\coderun.exe"
$prebuiltDaemon = Join-Path $Root "target\release\coderun-daemon.exe"
if (Test-Path $prebuilt) { Ok "coderun at target/release/coderun.exe" } else { Warn "coderun binary not found at target/release/coderun.exe - build manually: cargo build --release"; Fail "prebuilt coderun.exe missing - expected at target/release/coderun.exe" }
if (Test-Path $prebuiltDaemon) { Ok "coderun-daemon at target/release/coderun-daemon.exe" } else { Warn "coderun-daemon not found at target/release/coderun-daemon.exe" }

# 1b. TASK-037: ship binaries to %USERPROFILE%\.coderun\bin + persist on the USER PATH,
# so `coderun --version` and the daemon keep working from ANY directory/shell even if this
# repo checkout is moved or cleaned (cargo clean / -RemoveRepo). Idempotent re-run.
$binDir = Join-Path $env:USERPROFILE ".coderun\bin"
$installedCli = Join-Path $binDir "coderun.exe"
$installedDaemon = Join-Path $binDir "coderun-daemon.exe"
try {
  New-Item -ItemType Directory -Force -Path $binDir | Out-Null
  Copy-Item -LiteralPath $prebuilt -Destination $installedCli -Force -ErrorAction Stop
  Ok "coderun.exe installed to $installedCli"
} catch { Warn "failed to copy coderun.exe to ${binDir}: $_"; $installedCli = $prebuilt }
if (Test-Path $prebuiltDaemon) {
  try {
    Copy-Item -LiteralPath $prebuiltDaemon -Destination $installedDaemon -Force -ErrorAction Stop
    Ok "coderun-daemon.exe installed to $installedDaemon"
  } catch {
    Warn "failed to copy coderun-daemon.exe to ${binDir}: $_"
    if (-not (Test-Path $installedDaemon)) { $installedDaemon = $prebuiltDaemon }
  }
}
# Persist on the user PATH (HKCU Environment) — append only when missing (idempotent)
try {
  $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
  if ($null -eq $userPath) { $userPath = "" }
  $entries = $userPath -split ';' | Where-Object { $_ -ne '' }
  if ($entries -notcontains $binDir) {
    $newUserPath = ($entries + $binDir) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $newUserPath, 'User')
    Info "Added $binDir to USER PATH (persisted in HKCU Environment)"
  } else { Ok "$binDir already on USER PATH" }
} catch { Warn "could not persist USER PATH: $_" }
# Current session PATH so subsequent steps resolve coderun without the repo checkout
if (($env:Path -split ';') -notcontains $binDir) { $env:Path = "$binDir;$env:Path" }

# 1c. Ship the bundled skills library to ~\.coderun\skills (installation destination),
# mirroring the ~\.coderun\models layout - so behavioral skills work from ANY directory,
# independent of this repo checkout. Idempotent re-run refreshes the copy.
$srcSkills = Join-Path $Root ".coderun\skills"
$dstSkills = Join-Path $env:USERPROFILE ".coderun\skills"
if (Test-Path $srcSkills) {
  try {
    New-Item -ItemType Directory -Force -Path $dstSkills | Out-Null
    Copy-Item -Path (Join-Path $srcSkills "*") -Destination $dstSkills -Recurse -Force -ErrorAction Stop
    $skillCount = @(Get-ChildItem -LiteralPath $dstSkills -Directory -ErrorAction SilentlyContinue).Count
    Ok "skills library copied to $dstSkills ($skillCount skills)"
  } catch { Warn "failed to copy skills folder to ${dstSkills}: $_" }
} else { Warn "no skills folder found at $srcSkills - skipped" }

# 2. Verify installation (doctor)
# NOTE: `coderun init` / `coderun index` are NOT run here on purpose - they bootstrap the
# repository they run IN (per-repo .coderun/ + index), which is meaningless for the coderun
# source checkout itself. Run them inside each repo you want analyzed.
Info "Verifying installation (doctor)..."
$prevEA2 = $ErrorActionPreference; $ErrorActionPreference = "Continue"
try { & $installedCli doctor } catch {}
$ErrorActionPreference = $prevEA2

# 3. opencode plugin (GLOBAL: ~/.config/opencode - loaded in EVERY project)
Info "Configuring opencode plugin..."
$opencodeDir = Join-Path $Root ".opencode"   # legacy project dir - cleaned below
$ocGlobalDir = Join-Path $env:USERPROFILE ".config\opencode"
$ocGlobalCfg = Join-Path $ocGlobalDir "opencode.jsonc"
New-Item -ItemType Directory -Force -Path $ocGlobalDir | Out-Null
# Skip engram copy if already in ~/bin (the plugin resolves it from PATH)
if (-not (Test-Path $engramBinPath)) {
  # Only copy engram to opencode dir if not installed in ~/bin
  $opencodeEngramDir = Join-Path $ocGlobalDir "engram"
  $opencodeEngramBin = Join-Path $opencodeEngramDir "engram.exe"
  $engramSrc = $null
  if (Get-Command engram -ErrorAction SilentlyContinue) { $engramSrc = (Get-Command engram -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source) }
  if ($engramSrc -and (Test-Path $engramSrc)) {
    try {
      New-Item -ItemType Directory -Force -Path $opencodeEngramDir | Out-Null
      Copy-Item -LiteralPath $engramSrc -Destination $opencodeEngramBin -Force -ErrorAction Stop
      Ok "engram copied to $opencodeEngramDir"
    } catch {}
  }
}
# Global config: plugin only
$opencodeJsonc = @"
{
    "`$schema": "https://opencode.ai/config.json",
    "plugin": ["opencode-coderun"]
}
"@
try { Set-Content -LiteralPath $ocGlobalCfg -Value $opencodeJsonc -Encoding UTF8; Ok "opencode plugin at $ocGlobalCfg" } catch { Warn "failed to write $ocGlobalCfg : $_" }
# Remove legacy paths
$globalPlugin = "$env:USERPROFILE\.config\opencode\plugins\coderun.ts"
if (Test-Path $globalPlugin) { try { Remove-Item -LiteralPath $globalPlugin -Force } catch {} }
$localPlugin = Join-Path $opencodeDir "plugins\coderun.ts"
if (Test-Path $localPlugin) { try { Remove-Item -LiteralPath $localPlugin -Force } catch {} }
foreach ($f in @((Join-Path $opencodeDir "opencode.jsonc"), (Join-Path $opencodeDir "opencode.json"), (Join-Path $opencodeDir "package.json"), (Join-Path $opencodeDir "package-lock.json"))) {
  if (Test-Path $f) { try { Remove-Item -LiteralPath $f -Force } catch {} }
}

# Ensure npm plugin is built
$pluginDir = Join-Path $Root "packages\opencode-coderun"
$pluginDist = Join-Path $pluginDir "dist\index.js"
if (Test-Path $pluginDir) {
  if (-not (Test-Path $pluginDist)) {
    if (Test-Cmd npm) {
      Info "Building opencode-coderun npm package..."
      Push-Location $pluginDir
      try {
        & npm install --silent 2>&1 | Out-Null
        & npm run build --silent 2>&1 | Out-Null
        if (Test-Path $pluginDist) { Ok "opencode-coderun built to packages/opencode-coderun/dist" } else { Warn "opencode-coderun build failed - run: cd packages/opencode-coderun; npm install; npm run build" }
      } catch { Warn "opencode-coderun build failed: $_" }
      Pop-Location
    } else { Warn "npm not found - cannot build opencode-coderun (install Node.js 18+)" }
  } else { Ok "opencode-coderun dist at packages/opencode-coderun/dist/index.js" }
  if (Test-Cmd npm) {
    $pkgJson = Join-Path $ocGlobalDir "package.json"
    $pluginFileRef = "file:" + ((Join-Path $Root "packages\opencode-coderun") -replace '\\','/')
    if (-not (Test-Path $pkgJson)) {
      try { Set-Content -LiteralPath $pkgJson -Value (@{ dependencies = @{ "@opencode-ai/plugin" = "1.18.22"; "opencode-coderun" = $pluginFileRef } } | ConvertTo-Json -Depth 10) -Encoding UTF8 } catch {}
    } else {
      try {
        $j = Get-Content -LiteralPath $pkgJson -Raw | ConvertFrom-Json
        if (-not $j.dependencies) { $j | Add-Member -NotePropertyName dependencies -NotePropertyValue @{} }
        $j.dependencies."opencode-coderun" = $pluginFileRef
        if (-not $j.dependencies.PSObject.Properties["@opencode-ai/plugin"]) { $j.dependencies | Add-Member -NotePropertyName "@opencode-ai/plugin" -NotePropertyValue "1.18.22" }
        $j | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $pkgJson -Encoding UTF8
      } catch {}
    }
    Push-Location $ocGlobalDir
    try { & npm install --silent 2>&1 | Out-Null; if (Test-Path "node_modules\opencode-coderun\dist\index.js") { Ok "opencode-coderun plugin installed" } else { Warn "opencode-coderun npm install failed" } } catch { Warn "opencode-coderun npm install failed: $_" }
    Pop-Location
  }
} else { Warn "packages/opencode-coderun not found - skipping npm plugin install" }
Info "Restart opencode to load plugin (daemon http://127.0.0.1:9527)"

# 4. Start daemon - coderun must be in RUNNING state after installation
# TASK-037: launch the daemon from ~\.coderun\bin (installed copy) so the runtime keeps
# working if the repo is moved/cleaned — NOT from target\release.
function Test-DaemonHealth {
  try { $r = Invoke-WebRequest -Uri "http://127.0.0.1:9527/health" -UseBasicParsing -TimeoutSec 2; return ($r.StatusCode -ge 200 -and $r.StatusCode -lt 500) } catch { return $false }
}
Info "Starting coderun daemon..."
$daemonUp = Test-DaemonHealth
if ($daemonUp) {
  Ok "coderun daemon already running at http://127.0.0.1:9527 (status: running)"
} elseif (-not (Test-Path $installedDaemon)) {
  Warn "coderun-daemon.exe not found at $installedDaemon - build first (cargo build --release) then re-run installer or start manually"
} else {
  # Stale processes (holding old binary/port but not answering /health) - stop them before restart
  foreach ($procName in @("coderun-daemon", "coderun")) {
    Get-Process -Name $procName -ErrorAction SilentlyContinue | ForEach-Object {
      try { Stop-Process -Id $_.Id -Force -ErrorAction Stop; Info "  stopped stale $procName PID $($_.Id)" } catch {}
    }
  }
  $prevEA3 = $ErrorActionPreference; $ErrorActionPreference = "Continue"
  try {
    # WorkingDirectory: user .coderun home (repo-independent); scoping comes from per-request repository_path
    $daemonWorkDir = Join-Path $env:USERPROFILE ".coderun"
    New-Item -ItemType Directory -Force -Path $daemonWorkDir | Out-Null
    $daemonProc = Start-Process -FilePath $installedDaemon -WorkingDirectory $daemonWorkDir -WindowStyle Hidden -PassThru -ErrorAction Stop
    for ($i = 0; $i -lt 40; $i++) {
      Start-Sleep -Milliseconds 500
      if ($daemonProc.HasExited) { break }
      if (Test-DaemonHealth) { $daemonUp = $true; break }
    }
    if ($daemonUp) { Ok "coderun daemon RUNNING (PID $($daemonProc.Id), http://127.0.0.1:9527, from $installedDaemon)" }
    elseif ($daemonProc.HasExited) { Warn "daemon exited immediately (exit code $($daemonProc.ExitCode)) - start manually: $installedDaemon (check .coderun\config.toml)" }
    else { Warn "daemon started (PID $($daemonProc.Id)) but /health not responding within 20s - verify: curl http://127.0.0.1:9527/metrics" }
  } catch { Warn "failed to start daemon: $_ - start manually: $installedDaemon" }
  $ErrorActionPreference = $prevEA3
}

Info "Done - daemon: $(if ($daemonUp) { 'RUNNING at http://127.0.0.1:9527' } else { 'NOT running (start: ' + $installedDaemon + ')' }) | coderun preview 'add auth' | curl http://127.0.0.1:9527/metrics | coderun doctor"
Info "Docs: mkdocs serve  |  promptfoo eval --config eval/promptfooconfig.yaml  |  coderun doctor"
