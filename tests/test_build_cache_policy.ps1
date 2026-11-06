<#
.SYNOPSIS
    Unit test suite for Agent Studios Build Cache & Storage Stabilization Module.
#>

Set-StrictMode -Version 2.0
$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$modulePath = Join-Path (Split-Path -Parent $scriptDir) "scripts\build-cache\BuildCacheModule.psm1"

Import-Module $modulePath -Force

$passCount = 0
$failCount = 0

function Assert-Test([string]$Name, [bool]$Condition, [string]$Details = "") {
    if ($Condition) {
        Write-Output ("  [PASS] {0}" -f $Name)
        $script:passCount++
    } else {
        Write-Error ("  [FAIL] {0}: {1}" -f $Name, $Details)
        $script:failCount++
    }
}

Write-Output ("=" * 80)
Write-Output "RUNNING BUILD CACHE POLICY UNIT TESTS"
Write-Output ("=" * 80)

# Test 1: Deterministic & Isolated Path Hashing
$id1 = Get-PathHash8 "D:\Projects\agent-studios"
$id2 = Get-PathHash8 "D:\Projects\agent-studios"
$id3 = Get-PathHash8 "D:\Projects\agent-studios-active\m15-1-backend"
Assert-Test "Hash is deterministic" ($id1 -eq $id2)
Assert-Test "Different paths produce different hashes" ($id1 -ne $id3)
Assert-Test "Hash format is 8 hexadecimal characters" ($id1.Length -eq 8 -and $id1 -match '^[0-9a-f]{8}$')

# Test 2: Safe Directory Path Validation
$testBase = "C:\AgentStudiosBuild\cargo"
$safePath = "C:\AgentStudiosBuild\cargo\test-target"
$escapedPath = "C:\AgentStudiosBuild\cargo\..\other"
$resolvedSafe = Assert-SafeDirectoryPath -Path $safePath -BaseDir $testBase
Assert-Test "Safe path within base dir succeeds" ($resolvedSafe -eq $safePath)

$escapeCaught = $false
try {
    Assert-SafeDirectoryPath -Path $escapedPath -BaseDir $testBase
} catch {
    $escapeCaught = $_.Exception.Message -match "Security Violation"
}
Assert-Test "Escaped path traversal throws Security Violation" $escapeCaught

# Test 3: Worktree Metadata Resolution
$wt = "C:\AgentStudiosBuild\worktrees\build-cache-policy"
$meta = Get-WorktreeMetadata -WorktreePath $wt
Assert-Test "Worktree path resolved correctly" ($meta.WorktreePath -eq $wt)
Assert-Test "Branch resolved as chore/dev-build-cache-policy" ($meta.Branch -eq "chore/dev-build-cache-policy")
Assert-Test "HEAD is non-empty SHA" ($meta.Head.Length -eq 40)
Assert-Test "WorktreeId matches format" ($meta.WorktreeId -match '^build-cache-policy-[0-9a-f]{8}$')

# Test 4: Target Directory Resolution on Drive C:
$tgt = Get-WorktreeTargetDirectory -WorktreePath $wt
Assert-Test "Target directory starts with C:\AgentStudiosBuild\cargo" ($tgt.StartsWith("C:\AgentStudiosBuild\cargo\", [System.StringComparison]::OrdinalIgnoreCase))
Assert-Test "Target directory contains worktree id" ($tgt.EndsWith($meta.WorktreeId))

# Test 5: Disk Space Ledger
$ledger = Get-DiskSpaceLedger
Assert-Test "Drive D: free space reported" ($ledger.DFreeGB -gt 0)
Assert-Test "Drive C: free space reported" ($ledger.CFreeGB -gt 0)

# Test 6: Provenance Recording and Verification
$testBinDir = "C:\AgentStudiosBuild\diagnostics\test-bin"
if (-not (Test-Path $testBinDir)) { New-Item -ItemType Directory -Path $testBinDir -Force | Out-Null }
$testBin = Join-Path $testBinDir "dummy_agsd.exe"
Set-Content -Path $testBin -Value "AGENT_STUDIOS_TEST_BINARY_V1" -Encoding utf8

$rec = New-DaemonProvenance `
    -WorktreePath $wt `
    -BinaryPath $testBin `
    -BuildCommand "cargo build --bin agsd" `
    -Profile "debug"

Assert-Test "Provenance record generated with SHA-256" ($rec.binary_sha256.Length -eq 64)
Assert-Test "Provenance record references current HEAD" ($rec.git_head -eq $meta.Head)

$verifyGood = Test-DaemonProvenance `
    -WorktreePath $wt `
    -BinaryPath $testBin `
    -Profile "debug"
Assert-Test "Provenance verification succeeds on unmodified binary" ($verifyGood.IsValid -eq $true)

# Tamper test: modify binary
Set-Content -Path $testBin -Value "TAMPERED_AGENT_STUDIOS_BINARY" -Encoding utf8
$verifyTampered = Test-DaemonProvenance `
    -WorktreePath $wt `
    -BinaryPath $testBin `
    -Profile "debug"
Assert-Test "Provenance verification detects tampered binary SHA-256" ($verifyTampered.IsValid -eq $false -and $verifyTampered.Reason -match "Corrupt binary")

# Cleanup test binary
Remove-Item -LiteralPath $testBinDir -Recurse -Force -ErrorAction SilentlyContinue

Write-Output ("=" * 80)
Write-Output ("TEST SUMMARY: {0} PASSED | {1} FAILED" -f $script:passCount, $script:failCount)
Write-Output ("=" * 80)

if ($script:failCount -gt 0) {
    throw "$($script:failCount) tests failed"
}
