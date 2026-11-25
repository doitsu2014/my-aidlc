# Experimental Rust installer prototype. Kept separate from the production installer.
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
function Check-Recognised {
  if ((Present $installRoot) -and -not (Is-Ours)) { throw "$installRoot exists but is not a recognised Rust installation; not touching it" }
  if ((Present $launcher) -and -not (Launcher-Is-Ours)) { throw "$launcher exists but is not a recognised Rust launcher; not touching it" }
}

if ($Uninstall) {
  if (-not (Present $Prefix)) { Write-Output "nothing to uninstall under $Prefix"; return }
  Check-Recognised
  if (Test-Path -LiteralPath (Join-Path $installRoot '.dtech-aidlc-rust-install')) {
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
$assets = Join-Path $source 'assets'
if (-not (Test-Path -LiteralPath (Join-Path $assets 'phase-graph.json') -PathType Leaf)) { throw 'missing assets/phase-graph.json' }
if ($Upgrade) { Check-Recognised }
else {
  $existing = @($installRoot, $launcher) | Where-Object { Present $_ }
  if ($existing.Count -gt 0) { throw ("already exists: {0}; use -Upgrade or -Uninstall" -f ($existing -join ' ')) }
}

$lib = Join-Path $Prefix 'lib'
New-Item -ItemType Directory -Force -Path $lib, $binDir | Out-Null
$stage = Join-Path $lib ('.dtech-aidlc-rust.new.' + $PID)
$old = Join-Path $lib ('.dtech-aidlc-rust.old.' + $PID)
$tmpLauncher = Join-Path $binDir ('.dtech-aidlc-rust.tmp.' + $PID)
$launcherBackup = Join-Path $binDir ('.dtech-aidlc-rust.launcher-backup.' + $PID)
$hadLauncher = (Present $launcher)
$movedOld = $false
$movedNew = $false
try {
  New-Item -ItemType Directory -Path (Join-Path $stage 'assets') | Out-Null
  Copy-Item -LiteralPath $binary -Destination (Join-Path $stage 'dtech-aidlc.exe')
  Copy-Item -Path (Join-Path $assets '*') -Destination (Join-Path $stage 'assets') -Recurse -Force
  Set-Content -LiteralPath (Join-Path $stage '.dtech-aidlc-rust-install') -Value 'dtech-aidlc rust installer v1' -Encoding ascii
  @('@echo off', 'set "DTECH_AIDLC_ASSETS=%~dp0..\lib\dtech-aidlc\assets"', '"%~dp0..\lib\dtech-aidlc\dtech-aidlc.exe" %*') | Set-Content -LiteralPath $tmpLauncher -Encoding ascii
  if (Present $installRoot) { Move-Item -LiteralPath $installRoot -Destination $old; $movedOld = $true }
  Move-Item -LiteralPath $stage -Destination $installRoot; $movedNew = $true
  if ($hadLauncher) { Move-Item -LiteralPath $launcher -Destination $launcherBackup }
  Move-Item -LiteralPath $tmpLauncher -Destination $launcher
  if (Present $old) { Remove-Item -LiteralPath $old -Recurse -Force }
  if (Present $launcherBackup) { Remove-Item -LiteralPath $launcherBackup -Force }
} catch {
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
  if (Present $launcherBackup) { Remove-Item -LiteralPath $launcherBackup -Force -ErrorAction SilentlyContinue }
}
Write-Output "Installed Rust dtech-aidlc to $launcher"
Write-Output "Assets installed to $(Join-Path $installRoot 'assets')"
