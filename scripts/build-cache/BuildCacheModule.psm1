<#
.SYNOPSIS
    Agent Studios — Build Cache & Storage Stabilization Module
.DESCRIPTION
    Provides process-scoped CARGO_TARGET_DIR management on Drive C: to prevent
    exhaustion of Drive D:, maintaining strict worktree isolation and binary provenance.
#>

Set-StrictMode -Version 2.0

$script:DEFAULT_BUILD_ROOT = "C:\AgentStudiosBuild"
$script:MIN_D_DRIVE_WARN_GIB = 25.0
$script:MIN_D_DRIVE_BLOCK_GIB = 15.0
$script:MIN_C_DRIVE_WARN_GIB = 20.0

function Get-BuildRootPath {
    param([string]$CustomRoot)
    if ($CustomRoot -and $CustomRoot.Trim().Length -gt 0) {
        return [System.IO.Path]::GetFullPath($CustomRoot)
    }
    if ($env:AGENT_STUDIOS_BUILD_ROOT -and $env:AGENT_STUDIOS_BUILD_ROOT.Trim().Length -gt 0) {
        return [System.IO.Path]::GetFullPath($env:AGENT_STUDIOS_BUILD_ROOT)
    }
    return [System.IO.Path]::GetFullPath($script:DEFAULT_BUILD_ROOT)
}

function Assert-SafeDirectoryPath {
    param(
        [Parameter(Mandatory=$true)][string]$Path,
        [Parameter(Mandatory=$true)][string]$BaseDir
    )
    $fullPath = [System.IO.Path]::GetFullPath($Path)
    $fullBase = [System.IO.Path]::GetFullPath($BaseDir)

    if (-not $fullPath.StartsWith($fullBase, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Security Violation: Path '$fullPath' escapes authorized base directory '$fullBase'."
    }

    if (Test-Path -LiteralPath $fullPath) {
        $item = Get-Item -LiteralPath $fullPath -Force
        $isReparse = ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0
        if ($isReparse) {
            throw "Security Violation: Target directory '$fullPath' is a reparse point/junction, which is prohibited."
        }
    }
    return $fullPath
}

function Get-PathHash8 {
    param([Parameter(Mandatory=$true)][string]$Path)
    $norm = [System.IO.Path]::GetFullPath($Path).ToLowerInvariant().Replace("/", "\")
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($norm)
    $hashBytes = $hasher.ComputeHash($bytes)
    $hex = -join ($hashBytes | ForEach-Object { $_.ToString("x2") })
    return $hex.Substring(0, 8)
}

function Get-WorktreeMetadata {
    param([Parameter(Mandatory=$true)][string]$WorktreePath)

    if (-not (Test-Path -LiteralPath $WorktreePath)) {
        throw "Worktree path not found: '$WorktreePath'"
    }

    $fullPath = [System.IO.Path]::GetFullPath($WorktreePath)
    $cargoWorkspace = Join-Path $fullPath "agent-studios-rs\Cargo.toml"
    if (-not (Test-Path -LiteralPath $cargoWorkspace)) {
        throw "Directory is not a valid Agent Studios workspace (agent-studios-rs\Cargo.toml missing): '$fullPath'"
    }

    $head = (git -C $fullPath rev-parse HEAD 2>$null)
    if (-not $head) {
        throw "Failed to resolve git HEAD for worktree: '$fullPath'"
    }
    $head = $head.Trim()

    $branch = (git -C $fullPath branch --show-current 2>$null)
    if ($branch) {
        $branch = $branch.Trim()
    } else {
        $branch = "(detached)"
    }

    $leaf = Split-Path $fullPath -Leaf
    if ($leaf -match '^[a-zA-Z0-9_-]+$') {
        $safeLeaf = $leaf
    } else {
        $safeLeaf = "wt"
    }

    $hash8 = Get-PathHash8 $fullPath
    $worktreeId = "{0}-{1}" -f $safeLeaf, $hash8

    return [PSCustomObject]@{
        WorktreePath = $fullPath
        WorktreeId = $worktreeId
        Branch = $branch
        Head = $head
        HasCargoWorkspace = $true
    }
}

function Get-WorktreeTargetDirectory {
    param(
        [Parameter(Mandatory=$true)][string]$WorktreePath,
        [string]$BuildRoot
    )
    $root = Get-BuildRootPath $BuildRoot
    $cargoRoot = Join-Path $root "cargo"
    if (-not (Test-Path -LiteralPath $cargoRoot)) {
        New-Item -ItemType Directory -Path $cargoRoot -Force | Out-Null
    }

    $meta = Get-WorktreeMetadata -WorktreePath $WorktreePath
    $targetDir = Join-Path $cargoRoot $meta.WorktreeId

    Assert-SafeDirectoryPath -Path $targetDir -BaseDir $cargoRoot | Out-Null
    return $targetDir
}

function Get-DiskSpaceLedger {
    $dDrive = Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='D:'" -ErrorAction SilentlyContinue
    $cDrive = Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='C:'" -ErrorAction SilentlyContinue

    $dFreeGB = if ($dDrive) { [math]::Round($dDrive.FreeSpace / 1GB, 2) } else { 0 }
    $cFreeGB = if ($cDrive) { [math]::Round($cDrive.FreeSpace / 1GB, 2) } else { 0 }

    return [PSCustomObject]@{
        DFreeGB = $dFreeGB
        CFreeGB = $cFreeGB
        DFreeBytes = if ($dDrive) { $dDrive.FreeSpace } else { 0 }
        CFreeBytes = if ($cDrive) { $cDrive.FreeSpace } else { 0 }
    }
}

function Assert-DiskCapacityGuard {
    param(
        [switch]$IgnoreLimits,
        [string]$OperationName = "Rust build"
    )
    $space = Get-DiskSpaceLedger

    if ($space.DFreeGB -lt $script:MIN_D_DRIVE_BLOCK_GIB -and (-not $IgnoreLimits)) {
        throw "CAPACITY GUARD BLOCKED: Drive D: free space ($($space.DFreeGB) GB) is below critical threshold ($($script:MIN_D_DRIVE_BLOCK_GIB) GB). Cannot safely execute $OperationName without operator override (-IgnoreLimits)."
    }

    if ($space.DFreeGB -lt $script:MIN_D_DRIVE_WARN_GIB) {
        Write-Warning "CAPACITY GUARD WARNING: Drive D: free space ($($space.DFreeGB) GB) is below warning threshold ($($script:MIN_D_DRIVE_WARN_GIB) GB)."
    }

    if ($space.CFreeGB -lt $script:MIN_C_DRIVE_WARN_GIB) {
        Write-Warning "CAPACITY GUARD WARNING: Drive C: free space ($($space.CFreeGB) GB) is below warning threshold ($($script:MIN_C_DRIVE_WARN_GIB) GB)."
    }
}

function New-DaemonProvenance {
    param(
        [Parameter(Mandatory=$true)][string]$WorktreePath,
        [Parameter(Mandatory=$true)][string]$BinaryPath,
        [Parameter(Mandatory=$true)][string]$BuildCommand,
        [Parameter(Mandatory=$true)][string]$Profile,
        [string]$BuildRoot
    )
    if (-not (Test-Path -LiteralPath $BinaryPath)) {
        throw "Cannot record provenance: Binary does not exist at '$BinaryPath'"
    }

    $root = Get-BuildRootPath $BuildRoot
    $provDir = Join-Path $root "provenance"
    if (-not (Test-Path -LiteralPath $provDir)) {
        New-Item -ItemType Directory -Path $provDir -Force | Out-Null
    }

    $meta = Get-WorktreeMetadata -WorktreePath $WorktreePath
    $targetDir = Get-WorktreeTargetDirectory -WorktreePath $WorktreePath -BuildRoot $root

    $fileItem = Get-Item -LiteralPath $BinaryPath
    $fileHash = (Get-FileHash -Path $BinaryPath -Algorithm SHA256).Hash

    $provRecord = [PSCustomObject]@{
        schema_version = "1.0"
        recorded_at_utc = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
        worktree_path = $meta.WorktreePath
        worktree_id = $meta.WorktreeId
        git_branch = $meta.Branch
        git_head = $meta.Head
        cargo_target_dir = $targetDir
        build_command = $BuildCommand
        build_profile = $Profile
        binary_path = [System.IO.Path]::GetFullPath($BinaryPath)
        binary_sha256 = $fileHash
        binary_size_bytes = $fileItem.Length
    }

    $provFile = Join-Path $provDir ("{0}-{1}.provenance.json" -f $meta.WorktreeId, $Profile)
    $provRecord | ConvertTo-Json -Depth 5 | Out-File -FilePath $provFile -Encoding utf8
    return $provRecord
}
Set-Alias Record-DaemonProvenance New-DaemonProvenance

function Test-DaemonProvenance {
    param(
        [Parameter(Mandatory=$true)][string]$WorktreePath,
        [Parameter(Mandatory=$true)][string]$BinaryPath,
        [string]$Profile = "debug",
        [string]$BuildRoot
    )
    if (-not (Test-Path -LiteralPath $BinaryPath)) {
        return [PSCustomObject]@{ IsValid = $false; Reason = "Binary missing on disk: '$BinaryPath'" }
    }

    $root = Get-BuildRootPath $BuildRoot
    $provDir = Join-Path $root "provenance"
    $meta = Get-WorktreeMetadata -WorktreePath $WorktreePath
    $provFile = Join-Path $provDir ("{0}-{1}.provenance.json" -f $meta.WorktreeId, $Profile)

    if (-not (Test-Path -LiteralPath $provFile)) {
        return [PSCustomObject]@{ IsValid = $false; Reason = "No provenance record found at '$provFile'" }
    }

    $record = Get-Content -LiteralPath $provFile -Raw | ConvertFrom-Json
    if ($record.git_head -ne $meta.Head) {
        return [PSCustomObject]@{
            IsValid = $false
            Reason = "Stale provenance: Record Git HEAD ($($record.git_head)) does not match current worktree HEAD ($($meta.Head))"
            Record = $record
        }
    }

    $currentHash = (Get-FileHash -Path $BinaryPath -Algorithm SHA256).Hash
    if ($record.binary_sha256 -ne $currentHash) {
        return [PSCustomObject]@{
            IsValid = $false
            Reason = "Corrupt binary: Binary SHA-256 ($currentHash) does not match recorded SHA-256 ($($record.binary_sha256))"
            Record = $record
        }
    }

    return [PSCustomObject]@{
        IsValid = $true
        Reason = "Provenance matches source HEAD ($($meta.Head)) and SHA-256 ($currentHash)"
        Record = $record
    }
}
Set-Alias Verify-DaemonProvenance Test-DaemonProvenance

Export-ModuleMember -Function `
    Get-BuildRootPath, `
    Assert-SafeDirectoryPath, `
    Get-PathHash8, `
    Get-WorktreeMetadata, `
    Get-WorktreeTargetDirectory, `
    Get-DiskSpaceLedger, `
    Assert-DiskCapacityGuard, `
    New-DaemonProvenance, `
    Test-DaemonProvenance `
    -Alias Record-DaemonProvenance, Verify-DaemonProvenance
