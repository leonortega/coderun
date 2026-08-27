#Requires -Version 5.1
<#
.SYNOPSIS
  Coderun first-class installer (Windows PowerShell 5.1)
  V1 scope: local AI runtime only (DBOS/workflows in future/workflow, opt-in CODERUN_WORKFLOW_ENABLED=true)
  Installs ALL stack tools as first-class (no optional except LSP, no Temporal) + uses prebuilt coderun (no compile/test).
  Idempotent - re-run to update.

.DESCRIPTION
  Tools: Rust 1.75, Node >=20, Python+pip, Git, SQLite(bundled), tree-sitter/ripgrep/tantivy/tiktoken embedded,
         ast-grep, engram (local .coderun/engram/*.zip), codebase-memory-mcp, LiteLLM, RTK, MkDocs, analyzers (clippy/eslint), promptfoo, DBOS sidecar (future/workflow only)
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

Info "Coderun installer (DBOS/workflow future-only)"

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

  # FlashRank removed from v1 runtime per benchmark evaluation (see rerank.rs)

  # codebase-memory-mcp
  if (Test-Cmd npx) {
    try { npm list -g codebase-memory-mcp 2>&1 | Out-Null; if ($LASTEXITCODE -ne 0) { npm i -g codebase-memory-mcp 2>&1 | Out-Null }; Ok "codebase-memory-mcp $(npm list -g codebase-memory-mcp 2>&1 | Select-String codebase-memory-mcp)" } catch { Warn "codebase-memory-mcp npm install failed" }
    # Enable auto-indexing so repos are indexed automatically (persisted MCP config)
    # NOTE: judged via $LASTEXITCODE with EAP=Continue — under EAP=Stop, native stderr
    # (npm notices) redirected through 2>&1 becomes a terminating error even on success.
    $autoPrevEap = $ErrorActionPreference; $ErrorActionPreference = "Continue"
    try {
      $cfgOut = (& npx -y codebase-memory-mcp config set auto_index true 2>&1 | Out-String)
      if ($LASTEXITCODE -eq 0) { Ok "codebase-memory auto_index enabled" }
      else {
        $firstLine = ($cfgOut.Trim() -split "`r?`n" | Select-Object -First 1)
        Warn ("codebase-memory auto_index failed (exit {0}): {1}" -f $LASTEXITCODE, $firstLine)
      }
    } catch {
      Warn ("codebase-memory auto_index failed: " + $_.Exception.Message.Split("`n")[0])
    }
    $ErrorActionPreference = $autoPrevEap
  }

  # LiteLLM proxy
  if (Test-Cmd pip) { try { pip show litellm 2>&1 | Out-Null; if ($LASTEXITCODE -ne 0) { pip install "litellm[proxy]" 2>&1 | Out-Null }; Ok "litellm pip" } catch { Warn "litellm pip install failed" } }

  # RTK - PREBUILT binary from .coderun\rtk\rtk.exe -> ~\bin\rtk.exe (NO COMPILE). Cargo source build only as last resort.
  $rtkBinPath = "$env:USERPROFILE\bin\rtk.exe"
  if (Test-Cmd rtk) { Ok "rtk $(rtk --version 2>&1 | Select-Object -First 1)" }
  elseif (Test-Path $rtkBinPath) { $env:Path = "$(Split-Path $rtkBinPath -Parent);$env:Path"; Ok "rtk binary at $rtkBinPath" }
  elseif (Test-Path "$Root\.coderun\rtk\rtk.exe") {
    try {
      New-Item -ItemType Directory -Force -Path (Split-Path $rtkBinPath -Parent) | Out-Null
      Copy-Item -LiteralPath "$Root\.coderun\rtk\rtk.exe" -Destination $rtkBinPath -Force -ErrorAction Stop
      $env:Path = "$(Split-Path $rtkBinPath -Parent);$env:Path"
      Ok "rtk installed to $rtkBinPath (from .coderun\rtk\rtk.exe - no compile)"
    } catch { Warn "rtk copy from .coderun\rtk failed: $_" }
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

  # v1: DBOS sidecar NOT built -- future/workflow only, gated behind CODERUN_WORKFLOW_ENABLED=true (TASK-001)
  if ($env:CODERUN_WORKFLOW_ENABLED -eq "true" -and (Test-Path "$Root\future\workflow\dbos\package.json")) {
    Push-Location "$Root\future\workflow\dbos"
    try {
      npm install 2>&1 | Out-Null
      npx tsc 2>&1 | Out-Null
      npx tsc --noEmit 2>&1 | Out-Null
      if (Test-Path "dist/main.js") { Ok "future/workflow DBOS deps + built dist/main.js" } else { Warn "future/workflow DBOS build failed - dist/main.js missing" }
    } catch { Warn "future/workflow DBOS npm install/build failed - $_" }
    Pop-Location
  }
  # legacy workflow/dbos never built in v1 (TASK-001 purge)
  # if (Test-Path "$Root\workflow\dbos\package.json") { Warn "legacy workflow/dbos/package.json found -- v1 excludes workflow/dbos (use future/workflow/dbos with CODERUN_WORKFLOW_ENABLED=true)" }
}

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

# 3. opencode MCPs + plugin (GLOBAL: ~/.config/opencode - loaded in EVERY project)
Info "Configuring opencode MCPs and plugin (global ~\.config\opencode)..."
$opencodeDir = Join-Path $Root ".opencode"   # legacy project dir - cleaned below
$ocGlobalDir = Join-Path $env:USERPROFILE ".config\opencode"
$ocGlobalCfg = Join-Path $ocGlobalDir "opencode.jsonc"
New-Item -ItemType Directory -Force -Path $ocGlobalDir | Out-Null
# Copy engram binary into global opencode dir (ABSOLUTE path reference - must resolve from any project)
$opencodeEngramDir = Join-Path $ocGlobalDir "engram"
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
      try { if ((Get-Item $engramSrc).Length -eq (Get-Item $opencodeEngramBin).Length) { $doCopy = $false; Ok "engram already at ~\.config\opencode\engram\engram(.exe)" } } catch {}
    }
    if ($doCopy) {
      Copy-Item -LiteralPath $engramSrc -Destination $opencodeEngramBin -Force -ErrorAction Stop
      $opencodeEngramBinNoExt = Join-Path $opencodeEngramDir "engram"
      Copy-Item -LiteralPath $engramSrc -Destination $opencodeEngramBinNoExt -Force -ErrorAction Stop
      Ok "engram copied to $opencodeEngramBin"
    }
  } catch {
    # If file in use but already exists, treat as OK (portable copy already there)
    if (Test-Path $opencodeEngramBin) { Ok "engram at $opencodeEngramBin (in use)" }
    else { Warn "failed to copy engram to global opencode dir: $_" }
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
          Ok "engram copied to $opencodeEngramBin (from zip)"
        } catch {
          if (Test-Path $opencodeEngramBin) { Ok "engram at $opencodeEngramBin (in use)" }
          else { Warn "failed to copy engram to global opencode dir from zip: $_" }
        }
      }
    } catch { Warn "failed to copy engram to global opencode dir from zip: $_" }
  }
}
# Global config: ABSOLUTE engram path (project-relative .opencode paths only resolve inside this repo)
$ocEngramAbs = $opencodeEngramBin -replace '\\','/'
$opencodeJsonc = @"
{
    "`$schema": "https://opencode.ai/config.json",
    "plugin": ["opencode-coderun"],
    "mcp": {
        "codebase-memory": {
            "command": ["npx", "-y", "codebase-memory-mcp"],
            "type": "local",
            "enabled": true
        },
        "engram": {
            "command": ["$ocEngramAbs", "mcp", "--tools=agent"],
            "type": "local",
            "enabled": true
        }
    }
}
"@
try { Set-Content -LiteralPath $ocGlobalCfg -Value $opencodeJsonc -Encoding UTF8; Ok "opencode MCPs + plugin GLOBAL at $ocGlobalCfg (codebase-memory + engram -> $ocEngramAbs, plugin: opencode-coderun)" } catch { Warn "failed to write $ocGlobalCfg : $_" }

# Remove legacy global path plugin (now npm) and local path plugin
$globalPlugin = "$env:USERPROFILE\.config\opencode\plugins\coderun.ts"
if (Test-Path $globalPlugin) { try { Remove-Item -LiteralPath $globalPlugin -Force; Info "Removed legacy global path plugin coderun.ts" } catch {} }
$localPlugin = Join-Path $opencodeDir "plugins\coderun.ts"
if (Test-Path $localPlugin) { try { Remove-Item -LiteralPath $localPlugin -Force; Info "Removed legacy local path plugin .opencode/plugins/coderun.ts" } catch {} }

# Migrate: remove per-project opencode config/deps (MCPs + plugin are global now)
foreach ($lc in @((Join-Path $opencodeDir "opencode.jsonc"), (Join-Path $opencodeDir "opencode.json"))) {
  if (Test-Path $lc) { try { Remove-Item -LiteralPath $lc -Force; Info "Removed legacy project config $(Split-Path $lc -Leaf) (MCPs/plugin are global now)" } catch {} }
}
foreach ($lp in @((Join-Path $opencodeDir "package.json"), (Join-Path $opencodeDir "package-lock.json"))) {
  if (Test-Path $lp) { try { Remove-Item -LiteralPath $lp -Force; Info "Removed legacy project $(Split-Path $lp -Leaf) (plugin installs globally now)" } catch {} }
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
  # Install npm plugin GLOBALLY (~/.config/opencode/node_modules) via file: reference to this repo
  if (Test-Cmd npm) {
    Info "Installing opencode-coderun globally (~\.config\opencode)..."
    $pkgJson = Join-Path $ocGlobalDir "package.json"
    $pluginFileRef = "file:" + ((Join-Path $Root "packages\opencode-coderun") -replace '\\','/')
    if (-not (Test-Path $pkgJson)) {
      $initPkg = @"
{
  "dependencies": {
    "@opencode-ai/plugin": "1.18.22",
    "opencode-coderun": "$pluginFileRef"
  }
}
"@
      try { Set-Content -LiteralPath $pkgJson -Value $initPkg -Encoding UTF8 } catch {}
    } else {
      try {
        $j = Get-Content -LiteralPath $pkgJson -Raw | ConvertFrom-Json
        if (-not $j.dependencies) { $j | Add-Member -NotePropertyName dependencies -NotePropertyValue @{} }
        # Always refresh the file: ref (repo may have moved)
        if ($j.dependencies.PSObject.Properties["opencode-coderun"]) { $j.dependencies."opencode-coderun" = $pluginFileRef }
        else { $j.dependencies | Add-Member -NotePropertyName "opencode-coderun" -NotePropertyValue $pluginFileRef }
        $needsSave = $false
        if (-not $j.dependencies.PSObject.Properties["@opencode-ai/plugin"]) { $j.dependencies | Add-Member -NotePropertyName "@opencode-ai/plugin" -NotePropertyValue "1.18.22"; $needsSave = $true } else { $needsSave = $true }
        if ($needsSave) { $j | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $pkgJson -Encoding UTF8 }
      } catch {}
    }
    Push-Location $ocGlobalDir
    try { & npm install --silent 2>&1 | Out-Null; if (Test-Path "node_modules\opencode-coderun\dist\index.js") { Ok "opencode-coderun installed to ~\.config\opencode\node_modules (global)" } else { Warn "opencode-coderun npm install failed - try: cd ~\.config\opencode; npm install" } } catch { Warn "opencode-coderun npm install failed: $_" }
    Pop-Location
  } else { Warn "npm not found - skipping global opencode plugin install (install Node.js 18+)" }
} else { Warn "packages/opencode-coderun not found - skipping npm plugin install" }
Info "Restart opencode to load global plugin 'opencode-coderun' (hooks: chat.message + message.updated + tool.execute.before, daemon http://127.0.0.1:9527, 30s fail-open). MCPs+plugin now load in EVERY project (global ~\.config\opencode)."

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

Info "Done (v1) - daemon: $(if ($daemonUp) { 'RUNNING at http://127.0.0.1:9527' } else { 'NOT running (start: ' + $installedDaemon + ')' }) | coderun preview 'add auth' | curl http://127.0.0.1:9527/metrics | coderun doctor"
Info "Docs: mkdocs serve  |  promptfoo eval --config eval/promptfooconfig.yaml  |  coderun doctor"
Info "Opt-in workflow: `$env:CODERUN_WORKFLOW_ENABLED='true'; bash future/workflow/dbos/build.sh (future only) | cargo build --features workflow"
