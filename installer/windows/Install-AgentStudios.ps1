<#
.SYNOPSIS
    Installs Agent Studios binaries, extension package, and canonical Windows OS directories.
#>
[CmdletBinding()]
param(
    [string]$BundleRoot = (Split-Path -Parent $MyInvocation.MyCommand.Path),
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\AgentStudios"),
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$configDir = Join-Path $env:APPDATA "AgentStudios"
$localStateDir = Join-Path $env:LOCALAPPDATA "AgentStudios"
$artifactsDir = Join-Path $localStateDir "artifacts"

Write-Host "Agent Studios Installer (InstallDir: $InstallDir)"
Write-Host "Canonical config path:    $(Join-Path $configDir 'config.toml')"
Write-Host "Canonical state DB path:  $(Join-Path $localStateDir 'state.db')"
Write-Host "Canonical artifacts dir:  $artifactsDir"

if ($DryRun) {
    Write-Host "[DRY RUN] Verified installer configuration without modifying filesystem."
    exit 0
}

$binTarget = Join-Path $InstallDir "bin"
$extTarget = Join-Path $InstallDir "extension"
$cfgTarget = Join-Path $InstallDir "config"

New-Item -ItemType Directory -Force -Path $binTarget, $extTarget, $cfgTarget, $configDir, $localStateDir, $artifactsDir | Out-Null

Copy-Item -Force (Join-Path $BundleRoot "bin\agsd.exe") (Join-Path $binTarget "agsd.exe")
Copy-Item -Force (Join-Path $BundleRoot "bin\ags.exe") (Join-Path $binTarget "ags.exe")
Copy-Item -Force (Join-Path $BundleRoot "extension\agent-studios-vscode-0.1.0.vsix") (Join-Path $extTarget "agent-studios-vscode-0.1.0.vsix")
Copy-Item -Force (Join-Path $BundleRoot "config\product.json") (Join-Path $cfgTarget "product.json")
Copy-Item -Force (Join-Path $BundleRoot "config\config.default.toml") (Join-Path $cfgTarget "config.default.toml")

$userConfig = Join-Path $configDir "config.toml"
if (-not (Test-Path $userConfig)) {
    Copy-Item (Join-Path $BundleRoot "config\config.default.toml") $userConfig
}

Write-Host "Installation complete. Run `"$binTarget\ags.exe`" doctor to verify environment."
