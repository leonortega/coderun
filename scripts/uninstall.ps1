#Requires -Version 5.1
<#
.SYNOPSIS
  Coderun v1.0.0 (0.6.0) uninstaller (Windows PowerShell 5.1)
  V1 scope: local runtime only — reverses scripts/install.ps1 - removes everything by default, preserves future/workflow source unless -RemoveRepo.

.DESCRIPTION
  Default (no flags): stops daemon, removes project build artifacts (target/release/coderun*.exe),
  DBOS sidecar node_modules, opencode plugins (project-local + global), ALL first-class external tools
  (ast-grep, rtk, codebase-memory-mcp, litellm, mkdocs, promptfoo, eslint, engram, FlashRank ONNX),
  and ALL user/project data (%USERPROFILE%\.coderun, .coderun/, sockets). Idempotent - safe to re-run.

  This is strict mode: no fallbacks. Default uninstalls everything. Use -KeepExternal / -KeepData
  to preserve tools or data. -KeepBuild preserves target/.

.PARAMETER KeepExternal
  Keep first-class external tools (do not uninstall ast-grep, rtk, npm/pip packages, engram, FlashRank).

.PARAMETER KeepData
  Keep user and project data (do not delete %USERPROFILE%\.coderun or .coderun/).

.PARAMETER KeepBuild
  Keep target/ build artifacts (skip binary removal).

.PARAMETER RemoveExternal
  Legacy alias for default behavior (now default). Kept for backwards compat.

.PARAMETER RemoveData
  Legacy alias for default behavior (now default). Kept for backwards compat.

.PARAMETER Force
  Skip confirmation prompts (useful for CI). Without -Force, data removal prompts for confirmation.

.PARAMETER DryRun
  Preview what would be removed without deleting (alias for -WhatIf).

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/uninstall.ps1
  # full uninstall: binaries + plugins + external tools + data (prompts for data)

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/uninstall.ps1 -Force
  # full uninstall without prompt (CI)

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/uninstall.ps1 -KeepData -KeepExternal
  # only binaries + plugins, keep tools and data (old default)

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/uninstall.ps1 -DryRun
  # preview only

.NOTES
  Repository folders/files (.coderun/, target/, .opencode/, workflow/dbos/node_modules, .coderun/engram/*.zip) are NEVER deleted by default.
  Use -RemoveRepo to also delete repository artifacts (rarely needed).
#>
[CmdletBinding(SupportsShouldProcess=$true)]
param(
  [switch]$KeepExternal,
  [switch]$KeepData,
  [switch]$KeepBuild,
  [switch]$Force,
  [switch]$DryRun,
  [switch]$RemoveExternal,
  [switch]$RemoveData,
  [switch]$RemoveRepo
)

$ErrorActionPreference = "Continue"
# Always English in scripts
try { [System.Threading.Thread]::CurrentThread.CurrentUICulture = [System.Globalization.CultureInfo]::GetCultureInfo('en-US'); [System.Threading.Thread]::CurrentThread.CurrentCulture = [System.Globalization.CultureInfo]::GetCultureInfo('en-US') } catch {}
$Root = (Resolve-Path "$PSScriptRoot\..").Path
Set-Location $Root

function Test-Cmd($cmd) { $null -ne (Get-Command $cmd -ErrorAction SilentlyContinue) }
function Info($m) { Write-Host "[coderun] $m" -ForegroundColor Cyan }
function Ok($m) { Write-Host "  [OK] $m" -ForegroundColor Green }
function Warn($m) { Write-Host "  [WARN] $m" -ForegroundColor Yellow }
function Skip($m) { Write-Host "  [SKIP] $m" -ForegroundColor DarkGray }

# Default is to remove external tools and global data, but NEVER repository folders unless -RemoveRepo
$doRemoveExternal = $RemoveExternal -or -not $KeepExternal
$doRemoveData = $RemoveData -or -not $KeepData
$doRemoveRepo = $RemoveRepo  # only if explicitly requested
# For non-interactive CI, skip prompt by default when -Force is not set but -WhatIf is not set either
# We will not prompt for global data removal - only for repository removal which is destructive to source

if ($DryRun) { $PSBoundParameters["WhatIf"] = $true; $WhatIfPreference = $true }

Info "Coderun v1.0.0 uninstaller (0.6.0, DBOS/workflow future-only)"
if ($WhatIfPreference) { Warn "DryRun/WhatIf active - no changes will be made" }
Info "Options: RemoveExternal( effective=$doRemoveExternal KeepExternal=$KeepExternal ) RemoveData( effective=$doRemoveData KeepData=$KeepData ) KeepBuild=$KeepBuild Force=$Force"

# 0. Confirmation for destructive data removal - only for repository data (global is safe to delete)
if ($doRemoveRepo -and $doRemoveData -and -not $Force -and -not $WhatIfPreference) {
  $msg = "This will permanently delete repository .coderun/ (config, skills, models) at .coderun/. Global ~\.coderun will also be deleted. Continue?"
  $choice = Read-Host "$msg [y/N]"
  if ($choice -notin @("y","Y","yes","YES")) {
    Info "Aborted by user. Re-run with -Force to skip prompt or -KeepData to keep data."
    exit 0
  }
}

# 1. Stop daemon / clean socket
Info "Stopping daemon and cleaning socket..."

foreach ($procName in @("coderun-daemon","coderun")) {
  $procs = Get-Process -Name $procName -ErrorAction SilentlyContinue
  foreach ($p in $procs) {
    if ($PSCmdlet.ShouldProcess("Process $($p.ProcessName) PID $($p.Id)", "Stop-Process")) {
      try { Stop-Process -Id $p.Id -Force -ErrorAction Stop; Ok "stopped $procName PID $($p.Id)" } catch { Warn "failed to stop $procName PID $($p.Id): $_" }
    } else { Skip "would stop $procName PID $($p.Id)" }
  }
  if (-not $procs) { Skip "no running $procName process" }
}

$socketPaths = @(
  "$env:USERPROFILE\.coderun\coderun.sock",
  "/tmp/coderun.sock",
  (Join-Path $Root ".coderun/coderun.sock"),
  "/tmp/coderun.sock.lock"
)
$configToml = Join-Path $Root ".coderun/config.toml"
if (Test-Path $configToml) {
  try {
    $sockMatch = Select-String -Path $configToml -Pattern 'socket_path\s*=\s*"([^"]+)"' -ErrorAction SilentlyContinue
    if ($sockMatch) { $socketPaths += $sockMatch.Matches[0].Groups[1].Value }
  } catch {}
}
foreach ($sp in $socketPaths | Select-Object -Unique) {
  if (Test-Path $sp) {
    if ($PSCmdlet.ShouldProcess($sp, "Remove socket")) {
      try { Remove-Item -LiteralPath $sp -Force -ErrorAction Stop; Ok "removed socket $sp" } catch { Warn "failed to remove socket $sp : $_" }
    } else { Skip "would remove socket $sp" }
  }
}

# 2. Remove build artifacts - repository folders are NEVER deleted by default (use -RemoveRepo)
if ($KeepBuild -or -not $doRemoveRepo) {
  Info "Skipping build artifact removal (repository folders preserved - use -RemoveRepo to delete target/)"
  Skip "keeping target/ (repository - not deleted)"
  Skip "keeping workflow/dbos/node_modules + future/workflow/dbos/node_modules (repository - not deleted, v1 future-only)"
} else {
  Info "Removing build artifacts (--RemoveRepo)..."
  $binaries = @(
    "$Root\target\release\coderun.exe",
    "$Root\target\release\coderun-daemon.exe",
    "$Root\target\debug\coderun.exe",
    "$Root\target\debug\coderun-daemon.exe"
  )
  foreach ($bin in $binaries) {
    $display = $bin -replace [regex]::Escape($Root + "\"), "" -replace [regex]::Escape($Root + "/"), ""
    if (Test-Path $bin) {
      if ($PSCmdlet.ShouldProcess($display, "Remove-Item")) {
        try { Remove-Item -LiteralPath $bin -Force -ErrorAction Stop; Ok "removed $display" } catch { Warn "failed to remove $display : $_" }
      } else { Skip "would remove $display" }
    } else { Skip "not found $display" }
  }
  if (Test-Path "$Root\target") {
    if ($PSCmdlet.ShouldProcess("target/", "Remove-Item -Recurse")) {
      try { Remove-Item -LiteralPath "$Root\target" -Recurse -Force -ErrorAction Stop; Ok "removed target/ (cargo clean)" } catch { Warn "failed to remove target/: $_" }
    } else { Skip "would remove target/ (cargo clean)" }
  }
  # v1: workflow DBOS is future/workflow only — clean both legacy and future (legacy warned in install)
  $dbosPaths = @(
    "$Root\workflow\dbos\node_modules",
    "$Root\workflow\dbos\package-lock.json",
    "$Root\future\workflow\dbos\node_modules",
    "$Root\future\workflow\dbos\package-lock.json",
    "$Root\future\workflow\dbos\dist"
  )
  foreach ($p in $dbosPaths) {
    $display = $p -replace [regex]::Escape($Root + "\"), "" -replace [regex]::Escape($Root + "/"), ""
    if (Test-Path $p) {
      if ($PSCmdlet.ShouldProcess($display, "Remove-Item")) {
        try { Remove-Item -LiteralPath $p -Recurse -Force -ErrorAction Stop; Ok "removed $display (v1 workflow future-only)" } catch { Warn "failed to remove $display : $_" }
      } else { Skip "would remove $display" }
    } else { Skip "not found $display" }
  }
  if (Test-Path "$Root\future\workflow") { Skip "preserving future/workflow source (use --RemoveRepo + manual rm -rf future/workflow if needed)" }
}

# 3. Remove opencode plugins - repository plugin is NEVER deleted by default (use -RemoveRepo) - use .opencode folder
Info "Removing opencode plugins..."
$pluginProject = Join-Path $Root ".opencode\plugins\coderun.ts"
$pluginGlobal = Join-Path $env:USERPROFILE ".config\opencode\plugins\coderun.ts"
$hardcodedGlobalPlugin = "C:\Users\marce\.config\opencode\plugins\coderun.ts"
# Global plugin (outside repo) - always delete (hardcoded path as requested) - plugin 'coderun'
foreach ($g in @($pluginGlobal, $hardcodedGlobalPlugin) | Select-Object -Unique) {
  if (Test-Path $g) {
    if ($PSCmdlet.ShouldProcess($g, "Remove-Item")) {
      try { Remove-Item -LiteralPath $g -Force -ErrorAction Stop; Ok "removed global plugin 'coderun'" } catch { Warn "failed to remove global plugin 'coderun': $_" }
    } else { Skip "would remove global plugin 'coderun'" }
  } else { Skip "not found global plugin 'coderun'" }
}
# Repository plugin - keep unless -RemoveRepo (use .opencode folder, plugin 'coderun')
if (Test-Path $pluginProject) {
  if ($doRemoveRepo) {
    if ($PSCmdlet.ShouldProcess(".opencode/plugins/coderun.ts", "Remove-Item")) {
      try { Remove-Item -LiteralPath $pluginProject -Force -ErrorAction Stop; Ok "removed plugin 'coderun' (--RemoveRepo)" } catch { Warn "failed to remove plugin 'coderun': $_" }
    } else { Skip "would remove plugin 'coderun'" }
  } else { Skip "keeping plugin 'coderun' (use -RemoveRepo to delete)" }
} else { Skip "not found plugin 'coderun'" }
# Only clean global empty dir by default; repo dir is kept
if ((Test-Path $env:USERPROFILE\.config\opencode\plugins) -and -not (Get-ChildItem -LiteralPath "$env:USERPROFILE\.config\opencode\plugins" -Force -ErrorAction SilentlyContinue | Where-Object { $_.Name -ne ".gitkeep" })) {
  if ($PSCmdlet.ShouldProcess("$env:USERPROFILE\.config\opencode\plugins", "Remove-Item")) {
    try { Remove-Item -LiteralPath "$env:USERPROFILE\.config\opencode\plugins" -Force -ErrorAction SilentlyContinue; Ok "removed empty global dir" } catch {}
  }
}
if ($doRemoveRepo -and (Test-Path "$Root\.opencode\plugins") -and -not (Get-ChildItem -LiteralPath "$Root\.opencode\plugins" -Force -ErrorAction SilentlyContinue | Where-Object { $_.Name -ne ".gitkeep" })) {
  if ($PSCmdlet.ShouldProcess(".opencode/plugins", "Remove-Item")) {
    try { Remove-Item -LiteralPath "$Root\.opencode\plugins" -Force -ErrorAction SilentlyContinue; Ok "removed empty .opencode/plugins (--RemoveRepo)" } catch {}
  }
}

# 3b. Remove MCP + plugin from opencode (always for coderun entries — so plugin not showing after uninstall, file kept if has other config)
Info "Removing opencode MCP (codebase-memory + engram) + plugin (opencode-coderun)..."
function Remove-OpencodeMcp($configPath, $isRepo) {
  $displayPath = $configPath
  if ($isRepo) { $displayPath = $configPath -replace [regex]::Escape($Root + "\"), "" -replace [regex]::Escape($Root + "/"), ""; if ($displayPath -eq $configPath) { $displayPath = Split-Path $configPath -Leaf } ; if ($configPath -match "\.opencode") { $displayPath = ".opencode/" + (Split-Path $configPath -Leaf) } else { $displayPath = $displayPath } }
  if (-not (Test-Path $configPath)) { Skip "MCP config not found at $displayPath"; return }
  # v1: always clean coderun plugin/mcp from .opencode, even without -RemoveRepo — otherwise plugin/MCP still shows after uninstall+restart
  try {
    $raw = Get-Content -LiteralPath $configPath -Raw -ErrorAction SilentlyContinue
    if (-not $raw) { Skip "MCP config empty at $displayPath"; return }
    $noComments = $raw -replace '(?m)^\s*//.*$','' -replace '/\*.*?\*/',''
    $noComments = $noComments -replace ',\s*([\}\]])', '$1'
    $json = $null
    try {
      $obj = $noComments | ConvertFrom-Json -ErrorAction SilentlyContinue
      if ($obj) {
        $json = @{}
        foreach ($prop in $obj.PSObject.Properties) { $json[$prop.Name] = $prop.Value }
        if ($json.ContainsKey('mcp') -and $json['mcp'] -is [PSCustomObject]) {
          $mcpHash = @{}
          foreach ($p in $json['mcp'].PSObject.Properties) { $mcpHash[$p.Name] = $p.Value }
          $json['mcp'] = $mcpHash
        }
      }
    } catch {}
    if ($null -eq $json) { Skip "invalid JSON at $displayPath"; return }
    $removed = @(); $pluginRemoved = @()
    # handle plugin opencode-coderun / coderun
    if ($json.ContainsKey('plugin')) {
      $plugins = $json['plugin']
      $origCount = 0; $newPlugins = @()
      if ($plugins -is [System.Array]) { $origCount = $plugins.Count; $newPlugins = @($plugins | Where-Object { $_ -ne "opencode-coderun" -and $_ -ne "coderun" }) }
      elseif ($plugins -is [PSCustomObject]) { $origCount = 1; $newPlugins = @() }
      else { $origCount = 0; $newPlugins = @() }
      if ($origCount -ne $newPlugins.Count) {
        $pluginRemoved += "opencode-coderun"
        if ($newPlugins.Count -eq 0) { $json.Remove('plugin') } else { $json['plugin'] = $newPlugins }
        $removed += "plugin:opencode-coderun"
      }
    }
    # handle mcp codebase-memory + engram
    if ($json.ContainsKey('mcp')) {
      $mcp = $json['mcp']
      if ($mcp -is [PSCustomObject]) {
        $tmp = @{}
        foreach ($p in $mcp.PSObject.Properties) { $tmp[$p.Name] = $p.Value }
        $mcp = $tmp; $json['mcp'] = $mcp
      }
      foreach ($k in @('codebase-memory','engram','codebase-memory-mcp')) {
        if ($mcp.ContainsKey($k)) { $mcp.Remove($k); $removed += $k; $pluginRemoved += $k }
      }
      if ($mcp.Count -eq 0) { $json.Remove('mcp') }
    }
    if ($removed.Count -eq 0) { Skip "no coderun plugin/MCP entries at $displayPath"; return }
    # if file now only has $schema, remove it unless -RemoveRepo? keep file if has other keys, else clean
    $remainingKeys = @($json.Keys | Where-Object { $_ -ne '$schema' })
    if ($remainingKeys.Count -eq 0) {
      if ($PSCmdlet.ShouldProcess($configPath, "Remove empty config")) {
        try { Remove-Item -LiteralPath $configPath -Force -ErrorAction Stop; Ok "removed empty $displayPath (only coderun plugin/MCP)" } catch { Warn "failed to remove $displayPath : $_" }
      } else { Skip "would remove empty $displayPath" }
      return
    }
    if ($PSCmdlet.ShouldProcess($configPath, "Remove $removed")) {
      $out = $json | ConvertTo-Json -Depth 10
      [System.IO.File]::WriteAllText($configPath, $out, [System.Text.UTF8Encoding]::new($false))
      Ok "removed [$($removed -join ', ')] from $displayPath"
    } else { Skip "would remove [$($removed -join ', ')] from $displayPath" }
  } catch { Warn "MCP remove failed for $displayPath : $_" }
}
Remove-OpencodeMcp "$env:USERPROFILE\.config\opencode\opencode.jsonc" $false
Remove-OpencodeMcp "$env:USERPROFILE\.config\opencode\opencode.json" $false
Remove-OpencodeMcp (Join-Path $Root "opencode.jsonc") $true
Remove-OpencodeMcp (Join-Path $Root "opencode.json") $true
Remove-OpencodeMcp (Join-Path $Root ".opencode\opencode.jsonc") $true
Remove-OpencodeMcp (Join-Path $Root ".opencode\opencode.json") $true

# 4. Remove external tools (default: remove everything)
if (-not $doRemoveExternal) {
  Info "Skipping external tools (--KeepExternal)"
} else {
  Info "Removing external tools (strict default)..."

  if (Test-Cmd sg) {
    if ($PSCmdlet.ShouldProcess("ast-grep (cargo uninstall ast-grep)", "cargo uninstall")) {
      try { cargo uninstall ast-grep 2>&1 | Out-Null; Ok "uninstalled ast-grep (cargo)" } catch { Warn "ast-grep cargo uninstall failed: $_" }
    } else { Skip "would cargo uninstall ast-grep" }
  } else { Skip "ast-grep not installed (sg not on PATH)" }

  if (Test-Cmd rtk) {
    if ($PSCmdlet.ShouldProcess("rtk (cargo uninstall rtk)", "cargo uninstall")) {
      try { cargo uninstall rtk 2>&1 | Out-Null; Ok "uninstalled rtk (cargo)" } catch { Warn "rtk cargo uninstall failed: $_" }
    } else { Skip "would cargo uninstall rtk" }
  } else { Skip "rtk not installed" }

  if (Test-Cmd npm) {
    $hasMcp = $false
    try { npm list -g codebase-memory-mcp 2>&1 | Out-Null; if ($LASTEXITCODE -eq 0) { $hasMcp = $true } } catch {}
    if ($hasMcp) {
      if ($PSCmdlet.ShouldProcess("codebase-memory-mcp (npm -g)", "npm uninstall -g")) {
        try { npm uninstall -g codebase-memory-mcp 2>&1 | Out-Null; Ok "uninstalled codebase-memory-mcp (npm -g)" } catch { Warn "npm uninstall codebase-memory-mcp failed: $_" }
      } else { Skip "would npm uninstall -g codebase-memory-mcp" }
    } else { Skip "codebase-memory-mcp not installed (npm -g)" }
  }

  if (Test-Cmd npm) {
    foreach ($pkg in @("promptfoo","eslint")) {
      $hasPkg = $false
      try { npm list -g $pkg 2>&1 | Out-Null; if ($LASTEXITCODE -eq 0) { $hasPkg = $true } } catch {}
      if ($hasPkg) {
        if ($PSCmdlet.ShouldProcess("$pkg (npm -g)", "npm uninstall -g")) {
          try { npm uninstall -g $pkg 2>&1 | Out-Null; Ok "uninstalled $pkg (npm -g)" } catch { Warn "npm uninstall $pkg failed: $_" }
        } else { Skip "would npm uninstall -g $pkg" }
      } else { Skip "$pkg not installed (npm -g)" }
    }
  }

  # engram: remove from user bin and .opencode/engram (portable, never use  absolute in logs)
  $engramBin = "$env:USERPROFILE\bin\engram.exe"
  $engramBinDir = "$env:USERPROFILE\bin"
  $opencodeEngramDir = Join-Path $Root ".opencode\engram"
  $opencodeEngramBin = Join-Path $opencodeEngramDir "engram.exe"
  $opencodeEngramBinNoExt = Join-Path $opencodeEngramDir "engram"
  $engramLegacyClone = Resolve-Path -LiteralPath "$Root\..\engram" -ErrorAction SilentlyContinue
  if (-not $engramLegacyClone) { $engramLegacyClone = "$Root\..\engram" }
  if (Test-Path $engramBin) {
    if ($PSCmdlet.ShouldProcess($engramBin, "Remove-Item")) {
      try { Remove-Item -LiteralPath $engramBin -Force -ErrorAction Stop; Ok "removed engram binary $engramBin" } catch { Warn "failed to remove $engramBin : $_" }
      if ((Test-Path $engramBinDir) -and -not (Get-ChildItem -LiteralPath $engramBinDir -Force -ErrorAction SilentlyContinue | Where-Object { $_.Name -ne "engram.exe" })) {
        Skip "keeping $engramBinDir (user bin folder - not deleted, may contain other tools)"
      }
    } else { Skip "would remove engram binary $engramBin" }
  } else { Skip "engram binary not found at $engramBin (zip kept at .coderun\engram\)" }
  # Also remove portable copy in .opencode (use .opencode folder, not repo absolute)
  foreach ($p in @($opencodeEngramBin, $opencodeEngramBinNoExt)) {
    $display = ".opencode/engram/$(Split-Path $p -Leaf)"
    if (Test-Path $p) {
      if ($PSCmdlet.ShouldProcess($display, "Remove-Item")) {
        try { Remove-Item -LiteralPath $p -Force -ErrorAction Stop; Ok "removed engram portable $display" } catch { Warn "failed to remove $display : $_" }
      } else { Skip "would remove engram portable $display" }
    }
  }
  if (Test-Path $opencodeEngramDir) {
    if (-not (Get-ChildItem -LiteralPath $opencodeEngramDir -Force -ErrorAction SilentlyContinue)) {
      if ($PSCmdlet.ShouldProcess(".opencode/engram", "Remove-Item")) {
        try { Remove-Item -LiteralPath $opencodeEngramDir -Force -ErrorAction SilentlyContinue; Ok "removed empty .opencode/engram/" } catch {}
      }
    } else { Skip "keeping .opencode/engram/ (contains other files)" }
  }
  # 3c. Opencode npm plugin (file:../packages/opencode-coderun) — installed into .opencode/node_modules
  Info "Removing opencode npm plugin (opencode-coderun)..."
  $opencodeNodeModules = @(
    (Join-Path $Root ".opencode\node_modules\opencode-coderun"),
    (Join-Path $Root ".opencode\node_modules\@opencode-ai"),
    (Join-Path $Root ".opencode\package-lock.json")
  )
  foreach ($p in $opencodeNodeModules) {
    $display = $p -replace [regex]::Escape($Root + "\"), "" -replace [regex]::Escape($Root + "/"), ""
    if (Test-Path $p) {
      if ($PSCmdlet.ShouldProcess($display, "Remove-Item")) {
        try { Remove-Item -LiteralPath $p -Recurse -Force -ErrorAction Stop; Ok "removed $display (opencode npm plugin)" } catch { Warn "failed to remove $display : $_" }
      } else { Skip "would remove $display" }
    } else { Skip "not found $display" }
  }
  $opencodePkgJson = Join-Path $Root ".opencode\package.json"
  if (Test-Path $opencodePkgJson) {
    if ($PSCmdlet.ShouldProcess(".opencode/package.json", "clean opencode-coderun dep")) {
      try {
        $raw = Get-Content -LiteralPath $opencodePkgJson -Raw -ErrorAction Stop
        $obj = $raw | ConvertFrom-Json -ErrorAction Stop
        $changed = $false
        if ($obj.PSObject.Properties["dependencies"] -and $obj.dependencies.PSObject.Properties["opencode-coderun"]) {
          $obj.dependencies.PSObject.Properties.Remove("opencode-coderun"); $changed = $true
        }
        # If dependencies empty, remove file; else write back
        $depCount = 0; if ($obj.PSObject.Properties["dependencies"]) { $depCount = @($obj.dependencies.PSObject.Properties).Count }
        if ($depCount -eq 0) {
          Remove-Item -LiteralPath $opencodePkgJson -Force -ErrorAction Stop; Ok "removed empty .opencode/package.json"
        } elseif ($changed) {
          $obj | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $opencodePkgJson -Encoding UTF8; Ok "cleaned opencode-coderun from .opencode/package.json"
        } else { Skip "no opencode-coderun in .opencode/package.json" }
      } catch { Warn "failed to clean .opencode/package.json : $_" }
    } else { Skip "would clean .opencode/package.json" }
  } else { Skip "not found .opencode/package.json" }
  # Remove empty .opencode dir if only empty after plugin removal (keep if has opencode.jsonc)
  if ((Test-Path "$Root\.opencode") -and -not (Get-ChildItem -LiteralPath "$Root\.opencode" -Force -ErrorAction SilentlyContinue | Where-Object { $_.Name -notin @(".gitkeep") })) {
    if ($PSCmdlet.ShouldProcess(".opencode", "Remove-Item")) {
      try { Remove-Item -LiteralPath "$Root\.opencode" -Recurse -Force -ErrorAction SilentlyContinue; Ok "removed empty .opencode/" } catch {}
    }
  } else { Skip "keeping .opencode/ (has opencode.jsonc or other config)" }
  # Global npm plugin (if ever published)
  if (Test-Cmd npm) {
    $hasGlobal = $false; try { npm list -g opencode-coderun 2>&1 | Out-Null; if ($LASTEXITCODE -eq 0) { $hasGlobal = $true } } catch {}
    if ($hasGlobal) {
      if ($PSCmdlet.ShouldProcess("opencode-coderun (npm -g)", "npm uninstall -g")) {
        try { npm uninstall -g opencode-coderun 2>&1 | Out-Null; Ok "uninstalled opencode-coderun (npm -g)" } catch { Warn "npm uninstall opencode-coderun failed: $_" }
      } else { Skip "would npm uninstall -g opencode-coderun" }
    } else { Skip "opencode-coderun not installed globally (npm -g)" }
  }
  if (Test-Path $engramLegacyClone) {
    Skip "keeping legacy engram clone at ../engram (repository folder - not deleted per policy)"
  }

  $modelPath = "$env:USERPROFILE\.coderun\models\flashrank.onnx"
  if (Test-Path $modelPath) {
    if ($PSCmdlet.ShouldProcess($modelPath, "Remove-Item")) {
      try { Remove-Item -LiteralPath $modelPath -Force -ErrorAction Stop; Ok "removed FlashRank model $modelPath" } catch { Warn "failed to remove $modelPath : $_" }
      $modelDir = Split-Path $modelPath -Parent
      if ((Test-Path $modelDir) -and -not (Get-ChildItem -LiteralPath $modelDir -Force -ErrorAction SilentlyContinue)) {
        try { Remove-Item -LiteralPath $modelDir -Force -ErrorAction SilentlyContinue } catch {}
      }
    } else { Skip "would remove FlashRank model $modelPath" }
  } else { Skip "FlashRank model not found at $modelPath" }

  if (Test-Cmd pip) {
    foreach ($pipPkg in @("litellm","mkdocs","mkdocs-material","pymdown-extensions","markdown")) {
      $shown = $false
      try { pip show $pipPkg 2>&1 | Out-Null; if ($LASTEXITCODE -eq 0) { $shown = $true } } catch {}
      if ($shown) {
        if ($PSCmdlet.ShouldProcess("$pipPkg (pip)", "pip uninstall -y")) {
          try { pip uninstall -y $pipPkg 2>&1 | Out-Null; Ok "uninstalled $pipPkg (pip)" } catch { Warn "pip uninstall $pipPkg failed: $_" }
        } else { Skip "would pip uninstall -y $pipPkg" }
      } else { Skip "$pipPkg not installed (pip)" }
    }
  } else { Skip "pip not found - skipping pip package removal" }

  # clippy (never uninstall rustup itself)
  if (Test-Cmd rustup) {
    if ($PSCmdlet.ShouldProcess("clippy (rustup component remove clippy)", "rustup component remove")) {
      try { rustup component remove clippy 2>&1 | Out-Null; Ok "removed rustup component clippy" } catch { Warn "failed to remove clippy: $_" }
    } else { Skip "would rustup component remove clippy" }
    Skip "keeping rustup toolchain (never uninstall rustup)"
  } else { Skip "rustup not installed" }
}

# 5. Remove data - global data is removed, repository .coderun is NEVER deleted by default (use -RemoveRepo) - use .opencode/.coderun relative
if (-not $doRemoveData) {
  Info "Skipping data removal (--KeepData)"
  Info "  Kept: $env:USERPROFILE\.coderun (global)"
  Info "  Kept: .coderun/ (repository - use -RemoveRepo to delete)"
} else {
  Info "Removing data (global only, repository preserved)..."
  $globalData = "$env:USERPROFILE\.coderun"
  if (Test-Path $globalData) {
    if ($PSCmdlet.ShouldProcess($globalData, "Remove-Item -Recurse")) {
      try { Remove-Item -LiteralPath $globalData -Recurse -Force -ErrorAction Stop; Ok "removed $globalData (DB, index, cache, logs, models)" } catch { Warn "failed to remove $globalData : $_" }
    } else { Skip "would remove $globalData" }
  } else { Skip "not found $globalData" }

  $projData = Join-Path $Root ".coderun"
  if (Test-Path $projData) {
    if ($doRemoveRepo) {
      if ($PSCmdlet.ShouldProcess($projData, "Remove-Item -Recurse")) {
        try { Remove-Item -LiteralPath $projData -Recurse -Force -ErrorAction Stop; Ok "removed .coderun/ (--RemoveRepo)" } catch { Warn "failed to remove .coderun/ : $_" }
      } else { Skip "would remove .coderun/" }
    } else { Skip "keeping repository .coderun/ (use -RemoveRepo to delete)" }
  } else { Skip "not found .coderun/" }
}

# 6. Final status
Info "Uninstall complete."
if (-not $doRemoveExternal) { Info "  External tools were kept (--KeepExternal)" } else { Info "  External tools removed" }
if (-not $doRemoveData) { Info "  Data was kept (--KeepData)" } else { Info "  Global data removed (repository .coderun preserved)" }
if ($KeepBuild -or -not $doRemoveRepo) { Info "  Build artifacts kept (repository preserved - use -RemoveRepo to delete)" } else { Info "  Build artifacts removed (--RemoveRepo)" }
Info "To reinstall: powershell -ExecutionPolicy Bypass -File scripts/install.ps1"
# Only warn if global plugin still present after uninstall (repo plugin is intentionally kept)
if (Test-Path "$env:USERPROFILE\.config\opencode\plugins\coderun.ts") { Warn "global plugin still present at $env:USERPROFILE\.config\opencode\plugins\coderun.ts - may need manual removal or restart opencode" }
if ($doRemoveRepo -and (Test-Path "$Root\.opencode\plugins\coderun.ts")) { Warn "repository plugin still present at .opencode/plugins/coderun.ts even after --RemoveRepo" }
