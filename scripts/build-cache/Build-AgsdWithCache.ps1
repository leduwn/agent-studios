<#
.SYNOPSIS
    Builds the native agsd daemon binary with process-scoped target on Drive C:
    and cryptographically records its build provenance.
.EXAMPLE
    .\Build-AgsdWithCache.ps1 -WorktreePath "D:\Projects\agent-studios" -Profile debug
#>
param(
    [Parameter(Position=0)][string]$WorktreePath = (Get-Location).Path,
    [ValidateSet("debug", "release")][string]$Profile = "debug",
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
Write-Output "AGENT STUDIOS AGSD BUILD & PROVENANCE PIPELINE"
Write-Output ("Worktree:         {0}" -f $meta.WorktreePath)
Write-Output ("Branch:           {0}" -f $meta.Branch)
Write-Output ("Git HEAD:         {0}" -f $meta.Head)
Write-Output ("Profile:          {0}" -f $Profile)
Write-Output ("CARGO_TARGET_DIR: {0}" -f $targetDir)
Write-Output ("=" * 80)

Assert-DiskCapacityGuard -IgnoreLimits:$IgnoreLimits -OperationName "Building native agsd daemon"

$cargoArgs = @("build", "--bin", "agsd", "-p", "agent-studios-ipc-transport")
if ($Profile -eq "release") {
    $cargoArgs += "--release"
}

$buildCmd = "cargo " + ($cargoArgs -join ' ')
$invokeScript = Join-Path $scriptDir "Invoke-CargoWithCache.ps1"

& $invokeScript -WorktreePath $meta.WorktreePath -CargoArgs $cargoArgs -BuildRoot $BuildRoot -IgnoreLimits:$IgnoreLimits

$binaryName = if ($IsWindows -or ($env:OS -match "Windows")) { "agsd.exe" } else { "agsd" }
$expectedBinary = Join-Path $targetDir (Join-Path $Profile $binaryName)

if (-not (Test-Path -LiteralPath $expectedBinary)) {
    throw "Build completed but daemon binary not found at expected path: '$expectedBinary'"
}

$prov = Record-DaemonProvenance `
    -WorktreePath $meta.WorktreePath `
    -BinaryPath $expectedBinary `
    -BuildCommand $buildCmd `
    -Profile $Profile `
    -BuildRoot $BuildRoot

Write-Output ("=" * 80)
Write-Output "DAEMON PROVENANCE RECORDED SUCCESSFULLY"
Write-Output ("Binary Path:   {0}" -f $prov.binary_path)
Write-Output ("Binary Size:   {0:N0} bytes ({1:N2} MB)" -f $prov.binary_size_bytes, ($prov.binary_size_bytes / 1MB))
Write-Output ("SHA-256:       {0}" -f $prov.binary_sha256)
Write-Output ("Source Commit: {0}" -f $prov.git_head)
Write-Output ("Timestamp:     {0}" -f $prov.recorded_at_utc)
Write-Output ("=" * 80)

return $prov
