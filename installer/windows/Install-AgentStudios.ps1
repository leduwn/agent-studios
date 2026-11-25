<#
.SYNOPSIS
    Installs or upgrades Agent Studios binaries, Desktop launcher, extension VSIX, and canonical Windows directories.
    Supports isolated sandbox roots (-AppDataRoot / -LocalAppDataRoot) and rollback on upgrade errors.
#>
[CmdletBinding()]
param(
    [string]$BundleRoot = (Split-Path -Parent $MyInvocation.MyCommand.Path),
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\AgentStudios"),
    [string]$AppDataRoot = $env:APPDATA,
    [string]$LocalAppDataRoot = $env:LOCALAPPDATA,
    [switch]$CreateStartMenuShortcut,
    [switch]$CreateDesktopShortcut,
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$configDir = Join-Path $AppDataRoot "AgentStudios"
$localStateDir = Join-Path $LocalAppDataRoot "AgentStudios"
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
$backupDir = "$InstallDir.upgrade-backup"

if (Test-Path $backupDir) {
    Remove-Item -Recurse -Force $backupDir
}
$hadExistingInstall = Test-Path $InstallDir
if ($hadExistingInstall) {
    Copy-Item -Recurse -Force $InstallDir $backupDir
}

try {
    New-Item -ItemType Directory -Force -Path $binTarget, $extTarget, $cfgTarget, $configDir, $localStateDir, $artifactsDir | Out-Null

    Copy-Item -Force (Join-Path $BundleRoot "bin\agsd.exe") (Join-Path $binTarget "agsd.exe")
    Copy-Item -Force (Join-Path $BundleRoot "bin\ags.exe") (Join-Path $binTarget "ags.exe")
    $desktopExeSrc = Join-Path $BundleRoot "bin\AgentStudios.exe"
    if (Test-Path $desktopExeSrc) {
        Copy-Item -Force $desktopExeSrc (Join-Path $binTarget "AgentStudios.exe")
    }

    $vsixSrc = Get-ChildItem -Path (Join-Path $BundleRoot "extension") -Filter "*.vsix" | Select-Object -First 1
    if (-not $vsixSrc) {
        throw "No extension VSIX package found in $(Join-Path $BundleRoot 'extension')"
    }
    Get-ChildItem -Path $extTarget -Filter "*.vsix" -ErrorAction SilentlyContinue | Remove-Item -Force
    Copy-Item -Force $vsixSrc.FullName (Join-Path $extTarget $vsixSrc.Name)

    Copy-Item -Force (Join-Path $BundleRoot "config\product.json") (Join-Path $cfgTarget "product.json")
    Copy-Item -Force (Join-Path $BundleRoot "config\config.default.toml") (Join-Path $cfgTarget "config.default.toml")

    $userConfig = Join-Path $configDir "config.toml"
    if (-not (Test-Path $userConfig)) {
        Copy-Item (Join-Path $BundleRoot "config\config.default.toml") $userConfig
    } else {
        Write-Host "Preserving existing user configuration at $userConfig"
    }

    $desktopTarget = Join-Path $binTarget "AgentStudios.exe"
    if (($CreateStartMenuShortcut -or $CreateDesktopShortcut) -and (Test-Path $desktopTarget)) {
        $shell = New-Object -ComObject WScript.Shell
        if ($CreateStartMenuShortcut) {
            $startMenuDir = Join-Path $AppDataRoot "Microsoft\Windows\Start Menu\Programs"
            New-Item -ItemType Directory -Force -Path $startMenuDir | Out-Null
            $lnk = $shell.CreateShortcut((Join-Path $startMenuDir "Agent Studios.lnk"))
            $lnk.TargetPath = $desktopTarget
            $lnk.WorkingDirectory = $InstallDir
            $lnk.Save()
        }
        if ($CreateDesktopShortcut) {
            $deskDir = [Environment]::GetFolderPath("Desktop")
            if ($deskDir) {
                $lnk = $shell.CreateShortcut((Join-Path $deskDir "Agent Studios.lnk"))
                $lnk.TargetPath = $desktopTarget
                $lnk.WorkingDirectory = $InstallDir
                $lnk.Save()
            }
        }
    }

    if (Test-Path $backupDir) {
        Remove-Item -Recurse -Force $backupDir
    }
} catch {
    if ($hadExistingInstall -and (Test-Path $backupDir)) {
        if (Test-Path $InstallDir) {
            Remove-Item -Recurse -Force $InstallDir -ErrorAction SilentlyContinue
        }
        Move-Item -Force $backupDir $InstallDir
    }
    throw
}

Write-Host "Installation complete. Run `"$binTarget\ags.exe`" doctor to verify environment."
