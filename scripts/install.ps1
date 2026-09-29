# scripts/install.ps1 — install the my-aidlc command on Windows.
#
# Usage:
#   ./scripts/install.ps1 [-From <dir>] [-Prefix <dir>] [-BinDir <dir>]
#                         [-Version <x.y.z>] [-Uninstall] [-Quiet]

param(
  [string]$From = "",
  [string]$Prefix = "",
  [string]$BinDir = "",
  [string]$Version = "",
  [switch]$Uninstall,
  [switch]$Quiet
)

$ErrorActionPreference = "Stop"

function Say($Message) { if (-not $Quiet) { Write-Host $Message } }
function Die($Message) { Write-Error "ERROR $Message"; exit 1 }

if (-not $Prefix) {
  $Prefix = if ($env:MY_AIDLC_INSTALL_ROOT) { $env:MY_AIDLC_INSTALL_ROOT }
            else { Join-Path $env:LOCALAPPDATA "my-aidlc" }
}
if (-not $BinDir) {
  $BinDir = if ($env:MY_AIDLC_BIN_DIR) { $env:MY_AIDLC_BIN_DIR }
            else { Join-Path $env:LOCALAPPDATA "my-aidlc\bin" }
}

if ($Uninstall) {
  $launcher = Join-Path $BinDir "my-aidlc.cmd"
  if (Test-Path $launcher) { Remove-Item $launcher -Force }
  if (Test-Path $Prefix) { Remove-Item $Prefix -Recurse -Force }
  Say "Removed my-aidlc."
  exit 0
}

if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
  Die "Node.js 20+ is required but 'node' was not found."
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
if (-not $From) {
  $candidate = Split-Path -Parent $scriptDir
  if ((Test-Path (Join-Path $candidate "package.json")) -and (Test-Path (Join-Path $candidate "core"))) {
    $From = $candidate
  }
}
if (-not $From) { Die "no local source found; pass -From <dir> or -Version <x.y.z>" }
if (-not (Test-Path (Join-Path $From "package.json"))) { Die "$From is not a my-aidlc source tree" }
if (-not (Test-Path (Join-Path $From "core"))) { Die "$From is missing core/" }

Say "Installing my-aidlc from $From"
if (Test-Path $Prefix) { Remove-Item $Prefix -Recurse -Force }
New-Item -ItemType Directory -Path $Prefix -Force | Out-Null
foreach ($item in @("core", "harness", "scripts", "package.json", "README.md", "LICENSE")) {
  $source = Join-Path $From $item
  if (Test-Path $source) {
    Copy-Item $source -Destination $Prefix -Recurse -Force
  }
}

New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
$launcher = Join-Path $BinDir "my-aidlc.cmd"
@"
@echo off
set MY_AIDLC_HOME=$Prefix
node "%MY_AIDLC_HOME%\core\tools\my-aidlc.mjs" %*
"@ | Set-Content -Path $launcher -Encoding ASCII

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$BinDir*") {
  [Environment]::SetEnvironmentVariable("Path", "$userPath;$BinDir", "User")
  Say "Added $BinDir to the user PATH. Open a new terminal."
}

$installed = (Get-Content (Join-Path $Prefix "package.json") | ConvertFrom-Json).version
Say "PASS installed my-aidlc $installed"
Say "Next: my-aidlc config --harness pi"
