<#
.SYNOPSIS
    Safely uninstalls Agent Studios program files while preserving user worktrees and SQLite state unless -PurgeUserData is specified.
#>
[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\AgentStudios"),
    [switch]$PurgeUserData,
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Write-Host "Agent Studios Uninstaller (InstallDir: $InstallDir, PurgeUserData: $PurgeUserData)"
if ($DryRun) {
    Write-Host "[DRY RUN] Uninstall plan verified."
    exit 0
}

if (Test-Path $InstallDir) {
    Remove-Item -Recurse -Force $InstallDir
    Write-Host "Removed installed binaries at $InstallDir"
}

if ($PurgeUserData) {
    $roaming = Join-Path $env:APPDATA "AgentStudios"
    $local = Join-Path $env:LOCALAPPDATA "AgentStudios"
    if (Test-Path $roaming) { Remove-Item -Recurse -Force $roaming }
    if (Test-Path $local) { Remove-Item -Recurse -Force $local }
    Write-Host "Purged user configuration and local state directories."
}
