<#
.SYNOPSIS
    Runs Cargo commands with process-scoped target directory on Drive C:.
.EXAMPLE
    .\Invoke-CargoWithCache.ps1 -WorktreePath "D:\Projects\agent-studios" -CargoArgs @("check", "-p", "agent-studios-protocol")
#>
param(
    [Parameter(Position=0)][string]$WorktreePath = (Get-Location).Path,
    [Parameter(Position=1)][string[]]$CargoArgs = @("check"),
    [string]$BuildRoot,
    [switch]$IgnoreLimits
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Import-Module (Join-Path $scriptDir "BuildCacheModule.psm1") -Force

$meta = Get-WorktreeMetadata -WorktreePath $WorktreePath
$targetDir = Get-WorktreeTargetDirectory -WorktreePath $meta.WorktreePath -BuildRoot $BuildRoot

Write-Output ("=" * 80)
Write-Output "AGENT STUDIOS CARGO WRAPPER (PROCESS-SCOPED TARGET ON DRIVE C:)"
Write-Output ("Worktree:         {0}" -f $meta.WorktreePath)
Write-Output ("Branch:           {0}" -f $meta.Branch)
Write-Output ("Git HEAD:         {0}" -f $meta.Head)
Write-Output ("CARGO_TARGET_DIR: {0}" -f $targetDir)
Write-Output ("=" * 80)

Assert-DiskCapacityGuard -IgnoreLimits:$IgnoreLimits -OperationName "Cargo $($CargoArgs -join ' ')"
$diskBefore = Get-DiskSpaceLedger
Write-Output ("Initial Disk: D: {0} GB free | C: {1} GB free" -f $diskBefore.DFreeGB, $diskBefore.CFreeGB)

# Ensure target directory exists on Drive C:
if (-not (Test-Path -LiteralPath $targetDir)) {
    New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
}

$oldTarget = $env:CARGO_TARGET_DIR
$oldLocation = Get-Location

try {
    $env:CARGO_TARGET_DIR = $targetDir
    $cargoWorkspaceDir = Join-Path $meta.WorktreePath "agent-studios-rs"
    Set-Location $cargoWorkspaceDir

    Write-Output ("Executing: cargo {0}" -f ($CargoArgs -join ' '))
    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    & cargo @CargoArgs
    $exitCode = $LASTEXITCODE
    $stopwatch.Stop()

    Write-Output ("Cargo finished in {0:N2}s with exit code {1}" -f $stopwatch.Elapsed.TotalSeconds, $exitCode)
    if ($exitCode -ne 0) {
        throw "Cargo execution failed with exit code $exitCode"
    }
} finally {
    Set-Location $oldLocation
    if ($null -ne $oldTarget) {
        $env:CARGO_TARGET_DIR = $oldTarget
    } else {
        Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    }

    $diskAfter = Get-DiskSpaceLedger
    $deltaD = [math]::Round($diskAfter.DFreeGB - $diskBefore.DFreeGB, 3)
    $deltaC = [math]::Round($diskAfter.CFreeGB - $diskBefore.CFreeGB, 3)
    Write-Output ("=" * 80)
    Write-Output ("Final Disk:   D: {0} GB free (delta: {1} GB) | C: {2} GB free (delta: {3} GB)" -f $diskAfter.DFreeGB, $deltaD, $diskAfter.CFreeGB, $deltaC)
    Write-Output ("=" * 80)
}
