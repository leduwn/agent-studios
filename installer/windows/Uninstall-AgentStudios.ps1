<#
.SYNOPSIS
    Safely uninstalls Agent Studios program files while preserving user worktrees and SQLite state unless -PurgeUserData is specified.
#>
[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\AgentStudios"),
    [string]$AppDataRoot = $env:APPDATA,
    [string]$LocalAppDataRoot = $env:LOCALAPPDATA,
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

$startMenuLnk = Join-Path $AppDataRoot "Microsoft\Windows\Start Menu\Programs\Agent Studios.lnk"
if (Test-Path $startMenuLnk) {
    Remove-Item -Force $startMenuLnk
}

if ($PurgeUserData) {
    $roaming = Join-Path $AppDataRoot "AgentStudios"
    $local = Join-Path $LocalAppDataRoot "AgentStudios"
    if (Test-Path $roaming) { Remove-Item -Recurse -Force $roaming }
    if (Test-Path $local) { Remove-Item -Recurse -Force $local }
    Write-Host "Purged user configuration and local state directories."
}
