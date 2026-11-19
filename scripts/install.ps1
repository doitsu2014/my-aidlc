# Install dtech-aidlc on Windows. No network access is used.
# Usage: powershell -File scripts/install.ps1 [-From SOURCE] [-Prefix PREFIX]
param(
  [string]$From,
  [string]$Prefix
)
$ErrorActionPreference = 'Stop'
if (-not $From) { $From = Resolve-Path (Join-Path $PSScriptRoot '..') }
if (-not $Prefix) { $Prefix = Join-Path $env:LOCALAPPDATA 'dtech-aidlc' }
$source = (Resolve-Path $From).Path
foreach ($required in @('package.json', 'core\tools\dtech-aidlc.mjs', 'core\tools\lib.mjs', 'core\data\phase-graph.json')) {
  if (-not (Test-Path (Join-Path $source $required))) { throw "source is missing $required" }
}
$installRoot = Join-Path $Prefix 'lib'
$launcher = Join-Path $Prefix 'bin\dtech-aidlc.cmd'
if ((Test-Path $installRoot) -or (Test-Path $launcher)) { throw "installation already exists under $Prefix" }
New-Item -ItemType Directory -Path (Split-Path $launcher) | Out-Null
Copy-Item (Join-Path $source 'core') (Join-Path $installRoot 'core') -Recurse
Copy-Item (Join-Path $source 'harness') (Join-Path $installRoot 'harness') -Recurse
Copy-Item (Join-Path $source 'package.json') (Join-Path $installRoot 'package.json')
@(
  '@echo off',
  'node "%~dp0..\lib\core\tools\dtech-aidlc.mjs" %*'
) | Set-Content -Path $launcher -Encoding ascii
Write-Output "Installed dtech-aidlc to $launcher"
Write-Output "Add $(Split-Path $launcher) to PATH if needed."
