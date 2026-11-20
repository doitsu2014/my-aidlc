# Install, upgrade or uninstall dtech-aidlc from a local source tree. No network access is used.
# Valid for Windows PowerShell 5.1 and PowerShell 7. Paths are built with Join-Path.
param(
  [string]$From,
  [string]$Prefix,
  [switch]$Upgrade,
  [switch]$Uninstall
)
$ErrorActionPreference = 'Stop'
if ($Upgrade -and $Uninstall) { throw '-Upgrade and -Uninstall cannot be combined' }

function Join-Nested([string]$Base) {
  $path = $Base
  foreach ($part in $args) { $path = Join-Path $path $part }
  return $path
}
function Present($path) { return (Test-Path -LiteralPath $path) }
function Read-Version($pkg) {
  if (-not (Test-Path -LiteralPath $pkg)) { return 'unknown' }
  $m = Select-String -LiteralPath $pkg -Pattern '"version"\s*:\s*"([^"]+)"' | Select-Object -First 1
  if ($m) { return $m.Matches[0].Groups[1].Value }
  return 'unknown'
}
function LibIsOurs($dir) {
  if (-not (Test-Path -LiteralPath $dir -PathType Container)) { return $false }
  $pkg = Join-Path $dir 'package.json'
  $entry = Join-Nested $dir 'core' 'tools' 'dtech-aidlc.mjs'
  if (-not (Test-Path -LiteralPath $pkg)) { return $false }
  if (-not (Test-Path -LiteralPath $entry)) { return $false }
  return ((Get-Content -LiteralPath $pkg -Raw) -match '"name"\s*:\s*"dtech-aidlc"')
}
function OldLibIsOurs($lib) {
  $entry = Join-Nested $lib 'core' 'tools' 'dtech-aidlc.mjs'
  $pkg = Join-Path $lib 'package.json'
  if (-not (Test-Path -LiteralPath $entry)) { return $false }
  if (-not (Test-Path -LiteralPath $pkg)) { return $false }
  return ((Get-Content -LiteralPath $pkg -Raw) -match '"name"\s*:\s*"dtech-aidlc"')
}
function LauncherIsOurs($path) {
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return $false }
  $text = Get-Content -LiteralPath $path -Raw
  return ($text -match 'lib\\dtech-aidlc\\core\\tools\\dtech-aidlc\.mjs') -or ($text -match 'lib/dtech-aidlc/core/tools/dtech-aidlc.mjs') -or ($text -match 'lib\\core\\tools\\dtech-aidlc\.mjs') -or ($text -match 'lib/core/tools/dtech-aidlc.mjs')
}
function Check-Recognised($installRoot, $cmd, $sh) {
  if ((Present $installRoot) -and -not (LibIsOurs $installRoot)) { throw "$installRoot exists but is not a dtech-aidlc file; not touching it" }
  if ((Present $cmd) -and -not (LauncherIsOurs $cmd)) { throw "$cmd exists but is not a dtech-aidlc file; not touching it" }
  if ((Present $sh) -and -not (LauncherIsOurs $sh)) { throw "$sh exists but is not a dtech-aidlc file; not touching it" }
}
function Write-CmdLauncher($path) {
  $dir = Split-Path -Parent $path
  if (-not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
  @(
    '@echo off',
    'node "%~dp0..\lib\dtech-aidlc\core\tools\dtech-aidlc.mjs" %*'
  ) | Set-Content -Path $path -Encoding ascii
}
function Write-ShLauncher($path) {
  $dir = Split-Path -Parent $path
  if (-not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
  $text = @"
#!/bin/sh
set -eu
self=`$0
while [ -L "`$self" ]; do
  link=`$(readlink "`$self")
  case `$link in /*) self=`$link ;; *) self=`$(dirname -- "`$self")/`$link ;; esac
done
script_dir=`$(CDPATH= cd -- "`$(dirname -- "`$self")" && pwd -P)
entry=`$script_dir/../lib/dtech-aidlc/core/tools/dtech-aidlc.mjs
if [ ! -f "`$entry" ]; then
  printf '%s\n' "dtech-aidlc: installation is incomplete (`$entry is missing). Run install.ps1 -Upgrade from a dtech-aidlc checkout." >&2
  exit 1
fi
exec node "`$entry" "`$@"
"@
  $text = $text -replace "`r`n", "`n"
  [System.IO.File]::WriteAllText($path, $text, [System.Text.Encoding]::ASCII)
}
function Copy-Tree($source, $dest) {
  if (-not (Test-Path -LiteralPath $dest)) { New-Item -ItemType Directory -Path $dest | Out-Null }
  Copy-Item (Join-Path $source 'core') (Join-Path $dest 'core') -Recurse
  Copy-Item (Join-Path $source 'harness') (Join-Path $dest 'harness') -Recurse
  Copy-Item (Join-Path $source 'package.json') (Join-Path $dest 'package.json')
}

if (-not $From) {
  if ($env:DTECH_AIDLC_SOURCE) { $From = $env:DTECH_AIDLC_SOURCE }
  else { $From = Split-Path -Parent $PSScriptRoot }
}
if (-not $Prefix) {
  if ($env:DTECH_AIDLC_PREFIX) { $Prefix = $env:DTECH_AIDLC_PREFIX }
  else { $Prefix = Join-Path $env:LOCALAPPDATA 'dtech-aidlc' }
}

$lib = Join-Path $Prefix 'lib'
$installRoot = Join-Path $lib 'dtech-aidlc'
$bin = Join-Path $Prefix 'bin'
$cmdLauncher = Join-Path $bin 'dtech-aidlc.cmd'
$shLauncher = Join-Path $bin 'dtech-aidlc'
$oldEntry = Join-Nested $lib 'core' 'tools' 'dtech-aidlc.mjs'

if ($Uninstall) {
  if (-not (Present $Prefix)) { Write-Output "nothing to uninstall under $Prefix"; return }
  Check-Recognised $installRoot $cmdLauncher $shLauncher
  if (Present $cmdLauncher) { Remove-Item -LiteralPath $cmdLauncher -Force; Write-Output "Removed $cmdLauncher" }
  if (Present $shLauncher) { Remove-Item -LiteralPath $shLauncher -Force; Write-Output "Removed $shLauncher" }
  if (Present $installRoot) { Remove-Item -LiteralPath $installRoot -Recurse -Force; Write-Output "Removed $installRoot" }
  if ((Present $oldEntry) -and (OldLibIsOurs $lib)) {
    Remove-Item -LiteralPath (Join-Path $lib 'core') -Recurse -Force
    if (Present (Join-Path $lib 'harness')) { Remove-Item -LiteralPath (Join-Path $lib 'harness') -Recurse -Force }
    if (Present (Join-Path $lib 'package.json')) { Remove-Item -LiteralPath (Join-Path $lib 'package.json') -Force }
    Write-Output "Removed old layout under $lib"
  }
  return
}

if (-not (Test-Path -LiteralPath $From -PathType Container)) { throw "source directory not found: $From" }
$source = (Resolve-Path -LiteralPath $From).Path
foreach ($required in @('package.json', (Join-Nested 'core' 'tools' 'dtech-aidlc.mjs'), (Join-Nested 'core' 'tools' 'lib.mjs'), (Join-Nested 'core' 'data' 'phase-graph.json'))) {
  if (-not (Test-Path -LiteralPath (Join-Path $source $required))) { throw "source is missing $required" }
}
if (-not (Test-Path -LiteralPath (Join-Path $source 'core') -PathType Container)) { throw 'source is missing core/ or harness/' }
if (-not (Test-Path -LiteralPath (Join-Path $source 'harness') -PathType Container)) { throw 'source is missing core/ or harness/' }

if ($Upgrade) {
  Check-Recognised $installRoot $cmdLauncher $shLauncher
} else {
  $existing = @()
  foreach ($path in @($installRoot, $cmdLauncher, $shLauncher)) {
    if (Present $path) { $existing += $path }
  }
  if ($existing.Count -gt 0) {
    throw ("already exists: {0}; run install.ps1 -Upgrade to replace them or install.ps1 -Uninstall to remove them" -f ($existing -join ' '))
  }
}

if (-not (Test-Path -LiteralPath $lib)) { New-Item -ItemType Directory -Path $lib | Out-Null }
if (-not (Test-Path -LiteralPath $bin)) { New-Item -ItemType Directory -Path $bin | Out-Null }

$stage = Join-Path $lib ('.dtech-aidlc.new.' + $PID)
$old = Join-Path $lib ('.dtech-aidlc.old.' + $PID)
$createdRoot = $false
try {
  Copy-Tree $source $stage
  if ($Upgrade -and (Present $installRoot)) { Move-Item -LiteralPath $installRoot -Destination $old }
  Move-Item -LiteralPath $stage -Destination $installRoot
  $createdRoot = $true
  Write-CmdLauncher $cmdLauncher
  Write-ShLauncher $shLauncher
  if ((Present $oldEntry) -and (OldLibIsOurs $lib) -and (LibIsOurs $installRoot)) {
    Remove-Item -LiteralPath (Join-Path $lib 'core') -Recurse -Force
    if (Present (Join-Path $lib 'harness')) { Remove-Item -LiteralPath (Join-Path $lib 'harness') -Recurse -Force }
    if (Present (Join-Path $lib 'package.json')) { Remove-Item -LiteralPath (Join-Path $lib 'package.json') -Force }
  }
  if (Present $old) { Remove-Item -LiteralPath $old -Recurse -Force }
} catch {
  if ($createdRoot -and (Present $old)) {
    Remove-Item -LiteralPath $installRoot -Recurse -Force -ErrorAction SilentlyContinue
    Move-Item -LiteralPath $old -Destination $installRoot -ErrorAction SilentlyContinue
  } elseif ($createdRoot -and -not $Upgrade) {
    Remove-Item -LiteralPath $installRoot -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $cmdLauncher -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $shLauncher -Force -ErrorAction SilentlyContinue
  }
  if (Present $stage) { Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue }
  throw
}

if ($Upgrade) {
  Write-Output ('Upgraded dtech-aidlc {0} -> {1}' -f (Read-Version (Join-Path $old 'package.json')), (Read-Version (Join-Path $installRoot 'package.json')))
  Write-Output 'Workflow state migrates by itself. In each onboarded project run: dtech-aidlc harness refresh (skill files) and dtech-aidlc onboard (PROJECT.md, knowledge manifest and profile).'
} else {
  Write-Output "Installed dtech-aidlc to $cmdLauncher"
}
Write-Output 'Installed files:'
Write-Output "  $installRoot"
Write-Output "  $cmdLauncher"
Write-Output "  $shLauncher"
Write-Output "Add $bin to PATH if needed."
