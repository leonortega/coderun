#Requires -Version 5.1
<#
.SYNOPSIS
  Coderun v0.5.0 first-class installer (Windows PowerShell 5.1)
  Installs ALL stack tools as first-class (no optional except LSP, no Temporal) + uses prebuilt coderun (no compile/test).
  Idempotent - re-run to update.

.DESCRIPTION
  Tools: Rust 1.75, Node >=20, Python+pip, Git, SQLite(bundled), tree-sitter/ripgrep/tantivy/tiktoken embedded,
         ast-grep, engram (local .coderun/engram/*.zip), FlashRank (local .coderun/models/flashrank.onnx), codebase-memory-mcp, LiteLLM, RTK, MkDocs, analyzers (clippy/eslint), promptfoo, DBOS sidecar
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

Info "Coderun v0.5.0 installer"

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

  # ast-grep (binary is `ast-grep`, `sg` is deprecated alias - check ast-grep only to avoid warning)
  if (Test-Cmd ast-grep) { Ok "ast-grep $(ast-grep --version)" } elseif (Test-Cmd cargo) {
    Info "  ast-grep via cargo (sg-core)..."
    try { cargo install ast-grep --locked 2>&1 | Out-Null; Ok "ast-grep installed" } catch { Warn "ast-grep cargo install failed - heuristic fallback will WARN" }
  }

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

  # FlashRank ort model - local from .coderun/models/flashrank.onnx -> user profile
  $repoModel = "$Root\.coderun\models\flashrank.onnx"
  $modelDir = "$env:USERPROFILE\.coderun\models"
  New-Item -ItemType Directory -Force -Path $modelDir | Out-Null
  $modelPath = "$modelDir\flashrank.onnx"
  if (Test-Path $modelPath) { Ok "FlashRank model $modelPath" }
  elseif (Test-Path $repoModel) {
    try { Copy-Item -LiteralPath $repoModel -Destination $modelPath -Force; Ok "FlashRank model installed to $modelPath (from .coderun\models)" } catch { Warn "FlashRank copy failed: $_ - TF-IDF fallback until then" }
  } else { Warn "FlashRank model not found at $modelPath - TF-IDF fallback until then (expected at .coderun\models\flashrank.onnx)" }

  # codebase-memory-mcp
  if (Test-Cmd npx) {
    try { npm list -g codebase-memory-mcp 2>&1 | Out-Null; if ($LASTEXITCODE -ne 0) { npm i -g codebase-memory-mcp 2>&1 | Out-Null }; Ok "codebase-memory-mcp $(npm list -g codebase-memory-mcp 2>&1 | Select-String codebase-memory-mcp)" } catch { Warn "codebase-memory-mcp npm install failed" }
  }

  # LiteLLM proxy
  if (Test-Cmd pip) { try { pip show litellm 2>&1 | Out-Null; if ($LASTEXITCODE -ne 0) { pip install "litellm[proxy]" 2>&1 | Out-Null }; Ok "litellm pip" } catch { Warn "litellm pip install failed" } }

  # RTK (needs Rust 1.85+ for edition2024)
  if (Test-Cmd rtk) { Ok "rtk $(rtk --version 2>&1 | Select-Object -First 1)" }
  else {
    $rtkOk = $false
    try {
      # Ensure Rust is up to date for edition2024
      if (Test-Cmd rustup) { rustup update stable 2>&1 | Out-Null; rustup default stable 2>&1 | Out-Null }
      cargo install --git https://github.com/rtk-ai/rtk --locked 2>&1 | Out-Null
      if (Test-Cmd rtk) { Ok "rtk $(rtk --version 2>&1 | Select-Object -First 1) (cargo git)"; $rtkOk = $true }
    } catch {}
    if (-not $rtkOk) {
      try { cargo install rtk --locked 2>&1 | Out-Null; if (Test-Cmd rtk) { Ok "rtk $(rtk --version 2>&1 | Select-Object -First 1) (crates.io)"; $rtkOk = $true } } catch {}
    }
    if (-not $rtkOk) { Warn "rtk cargo install failed - built-ins fallback (needs Rust 1.85+; try: rustup update stable; cargo install --git https://github.com/rtk-ai/rtk)" }
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

  # DBOS sidecar deps - build to dist for reliable start (fix ts-node loader)
  if (Test-Path "$Root\workflow\dbos\package.json") {
    Push-Location "$Root\workflow\dbos"
    try {
      npm install 2>&1 | Out-Null
      npx tsc 2>&1 | Out-Null
      npx tsc --noEmit 2>&1 | Out-Null
      if (Test-Path "dist/main.js") { Ok "DBOS sidecar deps + built dist/main.js" } else { Warn "DBOS build failed - dist/main.js missing" }
    } catch { Warn "DBOS npm install/build failed - $_" }
    Pop-Location
  }
}

# 0b. Ensure workflow config exists (local DBOS sidecar, no secret required)
$cfgPath = Join-Path $Root ".coderun/config.toml"
try {
  New-Item -ItemType Directory -Force -Path (Split-Path $cfgPath -Parent) | Out-Null
  if (-not (Test-Path $cfgPath)) {
    $initCfg = "[workflow]`r`nenabled = true`r`nengine = `"dbos`"`r`ndbos_endpoint = `"http://localhost:3001`"`r`n"
    Set-Content -LiteralPath $cfgPath -Value $initCfg -Encoding UTF8
    Ok "Created .coderun/config.toml with [workflow] dbos"
  } else {
    $content = Get-Content -LiteralPath $cfgPath -Raw -ErrorAction SilentlyContinue
    if ($content -notmatch '\[workflow\]') {
      $append = "`r`n[workflow]`r`nenabled = true`r`nengine = `"dbos`"`r`ndbos_endpoint = `"http://localhost:3001`"`r`n"
      Add-Content -LiteralPath $cfgPath -Value $append -Encoding UTF8; Ok "Appended [workflow] to .coderun/config.toml"
    } else {
      Ok "workflow config at .coderun/config.toml"
    }
    # Remove legacy dbos_shared_secret if present (local sidecar no longer uses CODERUN_DBOS_SECRET)
    if ($content -match 'dbos_shared_secret') {
      try {
        $cleaned = $content -replace '(?m)^\s*dbos_shared_secret\s*=.*\r?\n', ""
        [System.IO.File]::WriteAllText($cfgPath, $cleaned, [System.Text.Encoding]::UTF8)
        Info "  Removed legacy dbos_shared_secret from .coderun/config.toml (no longer required for local DBOS)"
      } catch {}
    }
  }
} catch { Info "  workflow config check skipped: $_" }

# 0c. Ensure DBOS health (local sidecar, no auth)
$cfgDbosEndpoint = "http://localhost:3001"
try {
  $projCfg = Get-Content -LiteralPath $cfgPath -Raw -ErrorAction SilentlyContinue
  if ($projCfg -match 'dbos_endpoint\s*=\s*"([^"]+)"') { $cfgDbosEndpoint = $Matches[1] }
} catch {}
if (-not [string]::IsNullOrWhiteSpace($env:CODERUN_DBOS_ENDPOINT)) { $cfgDbosEndpoint = $env:CODERUN_DBOS_ENDPOINT }
Info "Checking DBOS health at $cfgDbosEndpoint/health ..."
$reachable = $false
try {
  $resp = Invoke-WebRequest -Uri "$cfgDbosEndpoint/health" -TimeoutSec 2 -UseBasicParsing -ErrorAction SilentlyContinue
  if ($resp -and $resp.StatusCode -eq 200) { $reachable = $true }
} catch {}
if ($reachable) { Ok "DBOS reachable at $cfgDbosEndpoint" } else {
  Info "  DBOS not reachable - attempting to start sidecar..."
  $started = $false
  try {
    if (Test-Path "$Root\workflow\dbos\dist\main.js") {
      $nodeExe = "node"
      if (Get-Command "node.exe" -ErrorAction SilentlyContinue) { $nodeExe = "node.exe" }
      $psi = New-Object System.Diagnostics.ProcessStartInfo
      $psi.FileName = $nodeExe
      $psi.Arguments = "dist/main.js"
      $psi.WorkingDirectory = "$Root\workflow\dbos"
      $psi.UseShellExecute = $false
      $psi.CreateNoWindow = $true
      $psi.EnvironmentVariables["DBOS_PORT"] = "3001"
      # also ensure no secret required
      $proc = [System.Diagnostics.Process]::Start($psi)
      if ($proc) { $started = $true; Start-Sleep -Seconds 3 }
    } elseif (Test-Path "$Root\workflow\dbos\package.json") {
      # Fallback: npm start (now runs node dist/main.js after package.json fix)
      $npmCmd = "npm"
      if (Get-Command "npm.cmd" -ErrorAction SilentlyContinue) { $npmCmd = "npm.cmd" }
      $psi = New-Object System.Diagnostics.ProcessStartInfo
      $psi.FileName = $npmCmd
      $psi.Arguments = "start"
      $psi.WorkingDirectory = "$Root\workflow\dbos"
      $psi.UseShellExecute = $false
      $psi.CreateNoWindow = $true
      $psi.EnvironmentVariables["DBOS_PORT"] = "3001"
      $proc = [System.Diagnostics.Process]::Start($psi)
      if ($proc) { $started = $true; Start-Sleep -Seconds 3 }
    }
  } catch { Info "  DBOS autostart skipped: $_" }
  for ($i = 0; $i -lt 12 -and -not $reachable; $i++) {
    Start-Sleep -Seconds 1
    try {
      $resp = Invoke-WebRequest -Uri "$cfgDbosEndpoint/health" -TimeoutSec 2 -UseBasicParsing -ErrorAction SilentlyContinue
      if ($resp -and $resp.StatusCode -eq 200) { $reachable = $true; break }
    } catch {}
  }
  if ($reachable) { Ok "DBOS sidecar started at $cfgDbosEndpoint" } else {
    if ($started) { Warn "DBOS started but not reachable at $cfgDbosEndpoint - check: cd workflow/dbos; node dist/main.js (logs at workflow/dbos)" } else { Warn "DBOS not reachable at $cfgDbosEndpoint - hint: cd workflow/dbos; npm run build; node dist/main.js" }
  }
}

# 1. Use prebuilt coderun (no compile/test - use repository binary)
if ($SkipBuild) { Info "Skipping build check (--SkipBuild)" }
Info "Checking prebuilt coderun..."
$prebuilt = Join-Path $Root "target\release\coderun.exe"
$prebuiltDaemon = Join-Path $Root "target\release\coderun-daemon.exe"
if (Test-Path $prebuilt) { Ok "coderun at target/release/coderun.exe" } else { Warn "coderun binary not found at target/release/coderun.exe - build manually: cargo build --release"; Fail "prebuilt coderun.exe missing - expected at target/release/coderun.exe" }
if (Test-Path $prebuiltDaemon) { Ok "coderun-daemon at target/release/coderun-daemon.exe" } else { Warn "coderun-daemon not found at target/release/coderun-daemon.exe" }

# 2. init + index + doctor
Info "Initializing repo..."
& ".\target\release\coderun.exe" init 2>&1 | Out-Null
& ".\target\release\coderun.exe" index 2>&1 | Out-Null
& ".\target\release\coderun.exe" doctor

# 3. opencode MCPs + plugin (use .opencode folder, no absolute repo path)
Info "Configuring opencode MCPs and plugin..."
$opencodeDir = Join-Path $Root ".opencode"
$opencodeCfg = Join-Path $opencodeDir "opencode.jsonc"
New-Item -ItemType Directory -Force -Path $opencodeDir | Out-Null
# Copy engram binary into .opencode/engram/ for portable relative reference (never use  absolute)
$opencodeEngramDir = Join-Path $opencodeDir "engram"
$opencodeEngramBin = Join-Path $opencodeEngramDir "engram.exe"
New-Item -ItemType Directory -Force -Path $opencodeEngramDir | Out-Null
# Resolve source engram binary (prefer installed user bin, else repo zip)
$engramSrc = $null
if (Test-Path $engramBinPath) { $engramSrc = $engramBinPath }
elseif (Get-Command engram -ErrorAction SilentlyContinue) { $engramSrc = (Get-Command engram -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source) }
if ($engramSrc -and (Test-Path $engramSrc)) {
  try {
    $doCopy = $true
    if ((Test-Path $opencodeEngramBin) -and (Test-Path $engramSrc)) {
      try { if ((Get-Item $engramSrc).Length -eq (Get-Item $opencodeEngramBin).Length) { $doCopy = $false; Ok "engram already at .opencode/engram/engram(.exe) (portable)" } } catch {}
    }
    if ($doCopy) {
      Copy-Item -LiteralPath $engramSrc -Destination $opencodeEngramBin -Force -ErrorAction Stop
      $opencodeEngramBinNoExt = Join-Path $opencodeEngramDir "engram"
      Copy-Item -LiteralPath $engramSrc -Destination $opencodeEngramBinNoExt -Force -ErrorAction Stop
      Ok "engram copied to .opencode/engram/engram(.exe) (portable)"
    }
  } catch {
    # If file in use but already exists, treat as OK (portable copy already there)
    if (Test-Path $opencodeEngramBin) { Ok "engram at .opencode/engram/engram(.exe) (portable, in use)" }
    else { Warn "failed to copy engram to .opencode: $_" }
  }
} else {
  # Fallback: copy directly from .coderun/engram zip if user bin not yet available
  $repoZip = Get-ChildItem -LiteralPath "$Root\.coderun\engram" -Filter "*.zip" -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($repoZip) {
    try {
      $tmp = Join-Path $env:TEMP "engram_opencode_copy"
      if (Test-Path $tmp) { Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue }
      Expand-Archive -LiteralPath $repoZip.FullName -DestinationPath $tmp -Force
      $src = Get-ChildItem -LiteralPath $tmp -Recurse -Filter "engram.exe" | Select-Object -First 1
      if ($src) {
        try {
          Copy-Item -LiteralPath $src.FullName -Destination $opencodeEngramBin -Force -ErrorAction Stop
          $opencodeEngramBinNoExt = Join-Path $opencodeEngramDir "engram"
          Copy-Item -LiteralPath $src.FullName -Destination $opencodeEngramBinNoExt -Force -ErrorAction Stop
          Ok "engram copied to .opencode/engram/engram(.exe) (from zip)"
        } catch {
          if (Test-Path $opencodeEngramBin) { Ok "engram at .opencode/engram/engram(.exe) (portable, in use)" }
          else { Warn "failed to copy engram to .opencode from zip: $_" }
        }
      }
    } catch { Warn "failed to copy engram to .opencode from zip: $_" }
  }
}
# Use relative .opencode path for MCP (never absolute )
$opencodeJsonc = @"
{
    "`$schema": "https://opencode.ai/config.json",
    "mcp": {
        "codebase-memory": {
            "command": ["npx", "-y", "codebase-memory-mcp"],
            "type": "local",
            "enabled": true
        },
        "engram": {
            "command": [".opencode/engram/engram.exe", "mcp", "--tools=agent"],
            "type": "local",
            "enabled": true
        }
    }
}
"@
try { Set-Content -LiteralPath $opencodeCfg -Value $opencodeJsonc -Encoding UTF8; Ok "opencode MCPs at .opencode/opencode.jsonc (codebase-memory + engram -> .opencode/engram/engram.exe)" } catch { Warn "failed to write $opencodeCfg : $_" }

$srcPlugin = Join-Path $opencodeDir "plugins\coderun.ts"
$pluginsDir = Join-Path $opencodeDir "plugins"
New-Item -ItemType Directory -Force -Path $pluginsDir | Out-Null
if (Test-Path $srcPlugin) {
  Ok "opencode plugin 'coderun' at .opencode/plugins/coderun.ts"
  # Global fallback for opencode installed via user config
  $globalPluginDir = "$env:USERPROFILE\.config\opencode\plugins"
  try {
    New-Item -ItemType Directory -Force -Path $globalPluginDir | Out-Null
    Copy-Item -LiteralPath $srcPlugin -Destination $globalPluginDir -Force
    Ok "opencode plugin 'coderun' copied to global"
  } catch { Info "  global plugin copy skipped: $_" }
  Info "Restart opencode to load plugin 'coderun' (hooks: message.updated + tool.execute.before, daemon http://127.0.0.1:9527, 30s fail-open)"
} else { Info "  opencode plugin 'coderun' not in repository (removed) - skipping" }

Info "Done - next: coderun serve  |  coderun preview 'add auth'  |  coderun workflow start 'refactor' --require-approval  |  curl http://127.0.0.1:9527/metrics"
Info "Docs: mkdocs serve  |  promptfoo eval --config eval/promptfooconfig.yaml  |  coderun doctor"
