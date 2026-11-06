<#
.SYNOPSIS
    Runs VS Code extension / daemon E2E test suites with strict binary provenance validation
    and explicit AGSD_BIN_PATH pointing to verified build artifact on Drive C:.
.EXAMPLE
    .\Invoke-E2EWithCache.ps1 -WorktreePath "D:\Projects\agent-studios" -Profile debug -TestCommand @("npm", "run", "test:e2e")
#>
param(
    [Parameter(Position=0)][string]$WorktreePath = (Get-Location).Path,
    [ValidateSet("debug", "release")][string]$Profile = "debug",
    [string[]]$TestCommand = @("npm", "test"),
    [string]$BuildRoot,
    [switch]$AutoBuildIfMissing
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Import-Module (Join-Path $scriptDir "BuildCacheModule.psm1") -Force

$meta = Get-WorktreeMetadata -WorktreePath $WorktreePath
$targetDir = Get-WorktreeTargetDirectory -WorktreePath $meta.WorktreePath -BuildRoot $BuildRoot

$binaryName = if ($IsWindows -or ($env:OS -match "Windows")) { "agsd.exe" } else { "agsd" }
$expectedBinary = Join-Path $targetDir (Join-Path $Profile $binaryName)

Write-Output ("=" * 80)
Write-Output "AGENT STUDIOS LOCAL E2E LAUNCHER WITH PROVENANCE VERIFICATION"
Write-Output ("Worktree:         {0}" -f $meta.WorktreePath)
Write-Output ("Current HEAD:     {0}" -f $meta.Head)
Write-Output ("Daemon Path:      {0}" -f $expectedBinary)
Write-Output ("Profile:          {0}" -f $Profile)
Write-Output ("=" * 80)

# Check existence and provenance
if (-not (Test-Path -LiteralPath $expectedBinary)) {
    if ($AutoBuildIfMissing) {
        Write-Output "Daemon binary not found. Auto-building via Build-AgsdWithCache.ps1..."
        $buildScript = Join-Path $scriptDir "Build-AgsdWithCache.ps1"
        & $buildScript -WorktreePath $meta.WorktreePath -Profile $Profile -BuildRoot $BuildRoot
    } else {
        throw "Daemon binary not found at '$expectedBinary'. Please build it first using Build-AgsdWithCache.ps1 or specify -AutoBuildIfMissing."
    }
}

$provVerification = Verify-DaemonProvenance `
    -WorktreePath $meta.WorktreePath `
    -BinaryPath $expectedBinary `
    -Profile $Profile `
    -BuildRoot $BuildRoot

if (-not $provVerification.IsValid) {
    if ($AutoBuildIfMissing) {
        Write-Warning "Provenance verification failed: $($provVerification.Reason). Rebuilding daemon..."
        $buildScript = Join-Path $scriptDir "Build-AgsdWithCache.ps1"
        & $buildScript -WorktreePath $meta.WorktreePath -Profile $Profile -BuildRoot $BuildRoot

        # Re-verify after rebuild
        $provVerification = Verify-DaemonProvenance `
            -WorktreePath $meta.WorktreePath `
            -BinaryPath $expectedBinary `
            -Profile $Profile `
            -BuildRoot $BuildRoot

        if (-not $provVerification.IsValid) {
            throw "Strict Provenance Error after rebuild: $($provVerification.Reason)"
        }
    } else {
        throw "Strict Provenance Error: $($provVerification.Reason). Run Build-AgsdWithCache.ps1 to recompile for current HEAD ($($meta.Head))."
    }
}

Write-Output ("Provenance Verified OK: Binary matches source commit {0} (SHA-256: {1})" -f $meta.Head, $provVerification.Record.binary_sha256)

$oldAgsdPath = $env:AGSD_BIN_PATH
$oldCI = $env:CI
$oldLocation = Get-Location

try {
    # Ensure CI=true is not set in local dev mode so AgsdBinaryResolver doesn't enforce runner-workspace boundary
    if ($null -ne $env:CI) {
        Remove-Item Env:\CI -ErrorAction SilentlyContinue
    }
    $env:AGSD_BIN_PATH = [System.IO.Path]::GetFullPath($expectedBinary)

    $vscodeAppDir = Join-Path $meta.WorktreePath "apps\agent-studios-vscode"
    if (Test-Path -LiteralPath $vscodeAppDir) {
        Set-Location $vscodeAppDir
    } else {
        Set-Location $meta.WorktreePath
    }

    Write-Output ("Executing test command in {0}: {1}" -f (Get-Location), ($TestCommand -join ' '))
    $cmd = $TestCommand[0]
    $cmdArgs = if ($TestCommand.Length -gt 1) { $TestCommand[1..($TestCommand.Length - 1)] } else { @() }

    & $cmd @cmdArgs
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "Test execution failed with exit code $exitCode"
    }
    Write-Output "Test suite completed successfully."
} finally {
    Set-Location $oldLocation
    if ($null -ne $oldAgsdPath) {
        $env:AGSD_BIN_PATH = $oldAgsdPath
    } else {
        Remove-Item Env:\AGSD_BIN_PATH -ErrorAction SilentlyContinue
    }
    if ($null -ne $oldCI) {
        $env:CI = $oldCI
    }
}
