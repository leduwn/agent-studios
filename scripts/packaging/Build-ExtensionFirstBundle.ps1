<#
.SYNOPSIS
    Builds and packages a self-contained Agent Studios VS Code Extension VSIX bundling agsd.exe + ags.exe.
    Preserves immutable RC1 and RC2 release artifacts in separate output paths.
#>
[CmdletBinding()]
param(
    [ValidateSet("debug", "release")]
    [string]$Profile = "release",
    [string]$Version = "1.1.1",
    [string]$OutputBase = "C:\AgentStudiosBuild\release-bundle",
    [switch]$SkipCargoBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$rustDir = Join-Path $repoRoot "agent-studios-rs"
$vscodeDir = Join-Path $repoRoot "apps\agent-studios-vscode"
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "C:\AgentStudiosBuild\cargo-target-rc2" }
$env:CARGO_TARGET_DIR = $targetDir

if ($Version -like "*rc1*" -or $Version -like "*rc2*") {
    throw "Refusing to overwrite immutable RC1 or RC2 release artifacts with version '$Version'."
}

$commitSha = (& git -C $repoRoot rev-parse HEAD).Trim()
$shortSha = (& git -C $repoRoot rev-parse --short HEAD).Trim()
if (-not $commitSha -or -not $shortSha) {
    throw "Failed to resolve git commit SHA from $repoRoot"
}

New-Item -ItemType Directory -Force -Path $OutputBase | Out-Null

if (-not $SkipCargoBuild) {
    Write-Host "==> Building agsd and ags ($Profile) into $targetDir..."
    Push-Location $rustDir
    try {
        if ($Profile -eq "release") {
            & cargo build --release --bin agsd --bin ags
        } else {
            & cargo build --bin agsd --bin ags
        }
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
}

$profileBinDir = Join-Path $targetDir $Profile
$agsdSrc = Join-Path $profileBinDir "agsd.exe"
$agsSrc = Join-Path $profileBinDir "ags.exe"
if (-not (Test-Path $agsdSrc)) { throw "Missing compiled binary: $agsdSrc" }
if (-not (Test-Path $agsSrc)) { throw "Missing compiled binary: $agsSrc" }

$binRootDir = Join-Path $vscodeDir "bin"
$platformBinDir = Join-Path $binRootDir "win32-x64"
$vsixPath = Join-Path $OutputBase "agent-studios-vscode-$Version-$shortSha-win32-x64.vsix"
$vsixShaPath = "$vsixPath.sha256"
$manifestPath = Join-Path $OutputBase "EXTENSION_MANIFEST_$Version-$shortSha.json"

if ($Version -eq "1.1.0") {
    throw "Refusing to overwrite preserved v1.1.0 VSIX baseline."
}

$pkgJsonPath = Join-Path $vscodeDir "package.json"
$pkgObj = Get-Content -Raw $pkgJsonPath | ConvertFrom-Json
if ($pkgObj.version -ne $Version) {
    Write-Host "==> Setting package.json version to $Version..."
    $rawPkg = Get-Content -Raw $pkgJsonPath
    $updatedPkg = [regex]::Replace($rawPkg, '"version"\s*:\s*"[^"]+"', "`"version`": `"$Version`"", 1)
    [System.IO.File]::WriteAllText($pkgJsonPath, $updatedPkg, [System.Text.UTF8Encoding]::new($false))
}

try {
    Write-Host "==> Staging bundled binaries at $platformBinDir..."
    if (Test-Path $binRootDir) {
        Remove-Item -Recurse -Force $binRootDir
    }
    New-Item -ItemType Directory -Force -Path $platformBinDir | Out-Null

    $agsdDest = Join-Path $platformBinDir "agsd.exe"
    $agsDest = Join-Path $platformBinDir "ags.exe"
    Copy-Item -Force $agsdSrc $agsdDest
    Copy-Item -Force $agsSrc $agsDest

    $agsdHash = (Get-FileHash -Algorithm SHA256 $agsdDest).Hash.ToLowerInvariant()
    $agsHash = (Get-FileHash -Algorithm SHA256 $agsDest).Hash.ToLowerInvariant()
    $sumsContent = "$agsdHash  agsd.exe`n$agsHash  ags.exe`n"
    [System.IO.File]::WriteAllText((Join-Path $platformBinDir "SHA256SUMS.txt"), $sumsContent, [System.Text.UTF8Encoding]::new($false))

    Write-Host "==> Compiling TypeScript and packaging self-contained VSIX to $vsixPath..."
    Push-Location $vscodeDir
    try {
        & npm run build
        if ($LASTEXITCODE -ne 0) { throw "npm run build failed with exit code $LASTEXITCODE" }
        & npx @vscode/vsce package --no-dependencies --out $vsixPath
        if ($LASTEXITCODE -ne 0) { throw "vsce package failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }

    $vsixHash = (Get-FileHash -Algorithm SHA256 $vsixPath).Hash.ToLowerInvariant()
    [System.IO.File]::WriteAllText($vsixShaPath, "$vsixHash  $(Split-Path -Leaf $vsixPath)`n", [System.Text.UTF8Encoding]::new($false))

    $manifest = [ordered]@{
        product = "Agent Studios VS Code Extension (Extension-First Self-Contained)"
        version = $Version
        commit_sha = $commitSha
        short_sha = $shortSha
        profile = $Profile
        vsix_path = $vsixPath
        vsix_sha256 = $vsixHash
        bundled_binaries = [ordered]@{
            "bin/win32-x64/agsd.exe" = $agsdHash
            "bin/win32-x64/ags.exe" = $agsHash
        }
        generated_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    }
    $manifestJson = $manifest | ConvertTo-Json -Depth 5
    [System.IO.File]::WriteAllText($manifestPath, $manifestJson, [System.Text.UTF8Encoding]::new($false))

    Write-Host "==> VSIX packaged cleanly: $vsixPath ($vsixHash)"
} finally {
    if (Test-Path $binRootDir) {
        Remove-Item -Recurse -Force $binRootDir
    }
}
