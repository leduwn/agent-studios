<#
.SYNOPSIS
    Preflight diagnostic inspection of disk capacity, worktrees, and Cargo target storage.
.EXAMPLE
    .\Get-BuildCacheDiagnostics.ps1
#>
param(
    [string]$BuildRoot
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Import-Module (Join-Path $scriptDir "BuildCacheModule.psm1") -Force

$root = Get-BuildRootPath $BuildRoot
$cargoRoot = Join-Path $root "cargo"
$provDir = Join-Path $root "provenance"

Write-Output ("=" * 80)
Write-Output "AGENT STUDIOS BUILD CACHE & STORAGE DIAGNOSTICS"
Write-Output ("Build Root:  {0}" -f $root)
Write-Output ("=" * 80)

# 1. Drive Capacity
$disk = Get-DiskSpaceLedger
$dStatus = "HEALTHY (>=25 GB)"
if ($disk.DFreeGB -lt 15) {
    $dStatus = "CRITICAL_BLOCKED (<15 GB)"
} elseif ($disk.DFreeGB -lt 25) {
    $dStatus = "WARNING (<25 GB)"
}
$cStatus = "HEALTHY"
if ($disk.CFreeGB -lt 20) {
    $cStatus = "WARNING (<20 GB)"
}

Write-Output "1. DRIVE CAPACITY BASELINE:"
Write-Output ("   Drive D: {0,8:N2} GB free | Status: {1}" -f $disk.DFreeGB, $dStatus)
Write-Output ("   Drive C: {0,8:N2} GB free | Status: {1}" -f $disk.CFreeGB, $cStatus)
Write-Output ""

# 2. Active Processes
Write-Output "2. ACTIVE BUILD PROCESSES:"
$procs = Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object { $_.Name -match 'cargo|rustc|agsd' }
if ($procs) {
    foreach ($p in $procs) {
        Write-Output ("   PID {0,-6} | {1,-12} | {2}" -f $p.ProcessId, $p.Name, $p.CommandLine)
    }
} else {
    Write-Output "   No cargo.exe, rustc.exe, or agsd.exe processes active."
}
Write-Output ""

# 3. Known Worktrees and Computed Cache Paths
Write-Output "3. KNOWN WORKTREES & TARGET MAPPINGS:"
$canonicalRepo = "D:\Projects\agent-studios"
$wtList = @()
if (Test-Path -LiteralPath $canonicalRepo) {
    $porcelain = git -C $canonicalRepo worktree list --porcelain 2>$null
    $currentWt = $null
    foreach ($line in $porcelain) {
        if ($line -match '^worktree\s+(.+)$') {
            $currentWt = $matches[1]
            if ($currentWt -notmatch 'AppData[\\/]Local[\\/]Temp' -and (Test-Path (Join-Path $currentWt "agent-studios-rs\Cargo.toml"))) {
                $wtList += $currentWt
            }
        }
    }
}
if ($wtList.Count -eq 0) {
    $wtList = @($canonicalRepo)
}

$fso = New-Object -ComObject Scripting.FileSystemObject

foreach ($wt in $wtList) {
    try {
        $meta = Get-WorktreeMetadata -WorktreePath $wt
        $targetDir = Get-WorktreeTargetDirectory -WorktreePath $wt -BuildRoot $root
        $cSizeMB = 0
        if (Test-Path -LiteralPath $targetDir) {
            try { $cSizeMB = [math]::Round($fso.GetFolder($targetDir).Size / 1MB, 2) } catch { $cSizeMB = -1 }
        }
        $dTarget = Join-Path $wt "agent-studios-rs\target"
        $dSizeGB = 0
        if (Test-Path -LiteralPath $dTarget) {
            try { $dSizeGB = [math]::Round($fso.GetFolder($dTarget).Size / 1GB, 2) } catch { $dSizeGB = -1 }
        }

        Write-Output ("   Worktree:  {0}" -f $wt)
        Write-Output ("     Branch:  {0} (HEAD: {1})" -f $meta.Branch, $meta.Head.Substring(0, [math]::Min(10, $meta.Head.Length)))
        Write-Output ("     Target:  {0} ({1:N2} MB on C:)" -f $targetDir, $cSizeMB)
        Write-Output ("     Legacy:  {0} ({1:N2} GB on D:)" -f $dTarget, $dSizeGB)
    } catch {
        Write-Output ("   Worktree:  {0} (Error inspecting: {1})" -f $wt, $_.Exception.Message)
    }
}
Write-Output ""

# 4. Provenance Records
Write-Output "4. RECORDED BUILD PROVENANCE RECORDS:"
if (Test-Path -LiteralPath $provDir) {
    $records = @(Get-ChildItem -Path $provDir -Filter "*.provenance.json" -File -ErrorAction SilentlyContinue)
    if ($records.Length -gt 0) {
        foreach ($r in $records) {
            $data = Get-Content -LiteralPath $r.FullName -Raw | ConvertFrom-Json
            Write-Output ("   Record:    {0}" -f $r.Name)
            Write-Output ("     HEAD:    {0} | Profile: {1}" -f $data.git_head, $data.build_profile)
            Write-Output ("     Binary:  {0} (SHA: {1})" -f $data.binary_path, $data.binary_sha256.Substring(0, 16) + "...")
        }
    } else {
        Write-Output "   No provenance records recorded yet."
    }
} else {
    Write-Output "   Provenance directory does not exist yet."
}

Write-Output ("=" * 80)
