# Install, upgrade or uninstall a prebuilt Rust dtech-aidlc and dtech-aidlc-mcp (target/release binaries plus assets/).
# Supports Windows PowerShell 5.1 and PowerShell 7.
param(
  [string]$From,
  [string]$Prefix,
  [switch]$Upgrade,
  [switch]$Uninstall
)
$ErrorActionPreference = 'Stop'
if ($Upgrade -and $Uninstall) { throw '-Upgrade and -Uninstall cannot be combined' }
function Present([string]$Path) { return (Test-Path -LiteralPath $Path) }
if (-not $Prefix) { $Prefix = if ($env:DTECH_AIDLC_PREFIX) { $env:DTECH_AIDLC_PREFIX } else { Join-Path $env:LOCALAPPDATA 'dtech-aidlc' } }
if (-not $From) { $From = Split-Path -Parent (Split-Path -Parent $PSScriptRoot) }
$installRoot = Join-Path (Join-Path $Prefix 'lib') 'dtech-aidlc'
$binDir = Join-Path $Prefix 'bin'
$launcher = Join-Path $binDir 'dtech-aidlc.cmd'
$mcpExe = Join-Path $binDir 'dtech-aidlc-mcp.exe'

function Is-Ours {
  $rust = (Test-Path -LiteralPath (Join-Path $installRoot '.dtech-aidlc-rust-install') -PathType Leaf) -and
    (Test-Path -LiteralPath (Join-Path $installRoot 'dtech-aidlc.exe') -PathType Leaf) -and
    (Test-Path -LiteralPath (Join-Path $installRoot 'assets') -PathType Container)
  $nodePkg = Join-Path $installRoot 'package.json'
  $nodeEntry = Join-Path $installRoot 'core/tools/dtech-aidlc.mjs'
  $node = (Test-Path -LiteralPath $nodePkg -PathType Leaf) -and (Test-Path -LiteralPath $nodeEntry -PathType Leaf) -and
    ((Get-Content -LiteralPath $nodePkg -Raw) -match '"name"\s*:\s*"dtech-aidlc"')
  return ($rust -or $node)
}
function Launcher-Is-Ours {
  if (-not (Test-Path -LiteralPath $launcher -PathType Leaf)) { return $false }
  $text = Get-Content -LiteralPath $launcher -Raw
  return (($text -match 'lib\\dtech-aidlc\\dtech-aidlc\.exe') -or ($text -match 'lib\\dtech-aidlc\\core\\tools\\dtech-aidlc\.mjs'))
}
function Mcp-Is-Ours {
  # A real exe cannot be grepped: it is ours when it is byte-identical to the installed copy.
  $installed = Join-Path $installRoot 'dtech-aidlc-mcp.exe'
  if (-not (Test-Path -LiteralPath $mcpExe -PathType Leaf) -or -not (Test-Path -LiteralPath $installed -PathType Leaf)) { return $false }
  return ((Get-FileHash -LiteralPath $mcpExe).Hash -eq (Get-FileHash -LiteralPath $installed).Hash)
}
function Check-InUse {
  $sep = [IO.Path]::DirectorySeparatorChar
  $lib = [IO.Path]::GetFullPath($installRoot).TrimEnd('\', '/') + $sep
  $mcpFull = [IO.Path]::GetFullPath($mcpExe)
  $busy = @(Get-Process -ErrorAction SilentlyContinue | Where-Object {
    $raw = $_.Path
    if (-not $raw) { return $false }
    try { $p = [IO.Path]::GetFullPath($raw) } catch { $p = $raw }
    $p.StartsWith($lib, [System.StringComparison]::OrdinalIgnoreCase) -or ($p -ieq $mcpFull)
  })
  # Process paths can differ from ours (8.3 short names), so on Windows also probe the files:
  # an exclusive open fails with a sharing violation while an executable is running.
  if ($busy.Count -eq 0 -and [Environment]::OSVersion.Platform -eq 'Win32NT') {
    $probe = @(Get-ChildItem -LiteralPath $installRoot -Filter '*.exe' -File -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName })
    if (Test-Path -LiteralPath $mcpExe -PathType Leaf) { $probe += $mcpFull }
    foreach ($file in $probe) {
      try { [IO.File]::Open($file, 'Open', 'ReadWrite', 'None').Dispose() }
      catch [IO.IOException] { $busy += $file }
      catch { }
    }
  }
  if ($busy.Count -gt 0) { throw 'dtech-aidlc files are in use: stop the AI host and any dtech-aidlc monitor, then re-run' }
}
function Check-Recognised {
  if ((Present $installRoot) -and -not (Is-Ours)) { throw "$installRoot exists but is not a recognised Rust installation; not touching it" }
  if ((Present $launcher) -and -not (Launcher-Is-Ours)) { throw "$launcher exists but is not a recognised Rust launcher; not touching it" }
  if ((Present $mcpExe) -and -not (Mcp-Is-Ours)) { throw "$mcpExe exists but is not a recognised Rust launcher; not touching it" }
}

if ($Uninstall) {
  if (-not (Present $Prefix)) { Write-Output "nothing to uninstall under $Prefix"; return }
  Check-Recognised
  if (Test-Path -LiteralPath (Join-Path $installRoot '.dtech-aidlc-rust-install')) {
    Check-InUse
    if (Present $mcpExe) { Remove-Item -LiteralPath $mcpExe -Force; Write-Output "Removed $mcpExe" }
    if (Present $launcher) { Remove-Item -LiteralPath $launcher -Force; Write-Output "Removed $launcher" }
    Remove-Item -LiteralPath $installRoot -Recurse -Force
    Write-Output "Removed Rust installation under $Prefix"
  } else { Write-Output 'No Rust installation found; existing Node installation left unchanged.' }
  return
}
if (-not (Test-Path -LiteralPath $From -PathType Container)) { throw "source directory not found: $From" }
$source = (Resolve-Path -LiteralPath $From).Path
$binary = Join-Path $source 'target/release/dtech-aidlc.exe'
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { $binary = Join-Path $source 'target/release/dtech-aidlc' }
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw 'missing target/release/dtech-aidlc executable; build it before installing' }
$mcpBinary = Join-Path $source 'target/release/dtech-aidlc-mcp.exe'
if (-not (Test-Path -LiteralPath $mcpBinary -PathType Leaf)) { $mcpBinary = Join-Path $source 'target/release/dtech-aidlc-mcp' }
if (-not (Test-Path -LiteralPath $mcpBinary -PathType Leaf)) { throw 'missing target/release/dtech-aidlc-mcp executable; build it before installing' }
$assets = Join-Path $source 'assets'
if (-not (Test-Path -LiteralPath (Join-Path $assets 'phase-graph.json') -PathType Leaf)) { throw 'missing assets/phase-graph.json' }
if ($Upgrade) { Check-Recognised }
else {
  $existing = @($installRoot, $launcher, $mcpExe) | Where-Object { Present $_ }
  if ($existing.Count -gt 0) { throw ("already exists: {0}; use -Upgrade or -Uninstall" -f ($existing -join ' ')) }
}

Check-InUse
$lib = Join-Path $Prefix 'lib'
# Leftovers from an earlier run are best-effort removed first.
Get-ChildItem -LiteralPath $lib -Filter '.dtech-aidlc-rust.old.*' -Force -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
Get-ChildItem -LiteralPath $binDir -Filter '.dtech-aidlc-rust.mcp-backup.*' -Force -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $lib, $binDir | Out-Null
$stage = Join-Path $lib ('.dtech-aidlc-rust.new.' + $PID)
$old = Join-Path $lib ('.dtech-aidlc-rust.old.' + $PID)
$tmpLauncher = Join-Path $binDir ('.dtech-aidlc-rust.tmp.' + $PID)
$launcherBackup = Join-Path $binDir ('.dtech-aidlc-rust.launcher-backup.' + $PID)
$tmpMcp = Join-Path $binDir ('.dtech-aidlc-rust.mcp-tmp.' + $PID)
$mcpBackup = Join-Path $binDir ('.dtech-aidlc-rust.mcp-backup.' + $PID)
$hadLauncher = (Present $launcher)
$hadMcp = (Present $mcpExe)
$movedOld = $false
$movedNew = $false
try {
  New-Item -ItemType Directory -Path (Join-Path $stage 'assets') | Out-Null
  Copy-Item -LiteralPath $binary -Destination (Join-Path $stage 'dtech-aidlc.exe')
  Copy-Item -LiteralPath $mcpBinary -Destination (Join-Path $stage 'dtech-aidlc-mcp.exe')
  Copy-Item -LiteralPath $mcpBinary -Destination $tmpMcp
  Copy-Item -Path (Join-Path $assets '*') -Destination (Join-Path $stage 'assets') -Recurse -Force
  Set-Content -LiteralPath (Join-Path $stage '.dtech-aidlc-rust-install') -Value 'dtech-aidlc rust installer v2' -Encoding ascii
  @('@echo off', 'set "DTECH_AIDLC_ASSETS=%~dp0..\lib\dtech-aidlc\assets"', '"%~dp0..\lib\dtech-aidlc\dtech-aidlc.exe" %*') | Set-Content -LiteralPath $tmpLauncher -Encoding ascii
  if (Present $installRoot) { Move-Item -LiteralPath $installRoot -Destination $old; $movedOld = $true }
  Move-Item -LiteralPath $stage -Destination $installRoot; $movedNew = $true
  if ($hadLauncher) { Move-Item -LiteralPath $launcher -Destination $launcherBackup }
  Move-Item -LiteralPath $tmpLauncher -Destination $launcher
  if ($hadMcp) { Move-Item -LiteralPath $mcpExe -Destination $mcpBackup }
  Move-Item -LiteralPath $tmpMcp -Destination $mcpExe
} catch {
  if (Present $mcpBackup) {
    if (Present $mcpExe) { Remove-Item -LiteralPath $mcpExe -Force -ErrorAction SilentlyContinue }
    Move-Item -LiteralPath $mcpBackup -Destination $mcpExe -ErrorAction SilentlyContinue
  } elseif (-not $hadMcp -and (Present $mcpExe)) { Remove-Item -LiteralPath $mcpExe -Force -ErrorAction SilentlyContinue }
  if (Present $launcherBackup) {
    if (Present $launcher) { Remove-Item -LiteralPath $launcher -Force -ErrorAction SilentlyContinue }
    Move-Item -LiteralPath $launcherBackup -Destination $launcher -ErrorAction SilentlyContinue
  } elseif (-not $hadLauncher -and (Present $launcher)) { Remove-Item -LiteralPath $launcher -Force -ErrorAction SilentlyContinue }
  if ($movedNew -and (Present $installRoot)) { Remove-Item -LiteralPath $installRoot -Recurse -Force -ErrorAction SilentlyContinue }
  if ($movedOld -and (Present $old)) { Move-Item -LiteralPath $old -Destination $installRoot -ErrorAction SilentlyContinue }
  throw
} finally {
  if (Present $stage) { Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue }
  if (Present $tmpLauncher) { Remove-Item -LiteralPath $tmpLauncher -Force -ErrorAction SilentlyContinue }
  if (Present $tmpMcp) { Remove-Item -LiteralPath $tmpMcp -Force -ErrorAction SilentlyContinue }
}
# Past the commit point: removing the old install and backups is best-effort and must never
# trigger the rollback above.
foreach ($leftover in @($old, $launcherBackup, $mcpBackup)) {
  if (Present $leftover) { Remove-Item -LiteralPath $leftover -Recurse -Force -ErrorAction SilentlyContinue }
}
Write-Output "Installed Rust dtech-aidlc to $launcher"
Write-Output "Installed Rust dtech-aidlc-mcp to $mcpExe"
Write-Output "Assets installed to $(Join-Path $installRoot 'assets')"
