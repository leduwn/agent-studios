<#
.SYNOPSIS
    Builds and stages an Agent Studios Windows x64 RC2 standalone distribution bundle on Drive C:.
    Preserves any previously signed/accepted RC1 artifacts in separate directories.
#>
[CmdletBinding()]
param(
    [ValidateSet("debug", "release")]
    [string]$Profile = "release",
    [string]$Version = "1.0.0-rc2",
    [string]$OutputBase = "C:\AgentStudiosBuild\release-bundle"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$rustDir = Join-Path $repoRoot "agent-studios-rs"
$vscodeDir = Join-Path $repoRoot "apps\agent-studios-vscode"
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "C:\AgentStudiosBuild\cargo-target-rc2" }
$env:CARGO_TARGET_DIR = $targetDir

$commitSha = (& git -C $repoRoot rev-parse HEAD).Trim()
$shortSha = (& git -C $repoRoot rev-parse --short HEAD).Trim()
if (-not $commitSha -or -not $shortSha) {
    throw "Failed to resolve git source SHA from $repoRoot"
}

$bundleName = "AgentStudios-v$Version-$shortSha-win-x64"
if ($bundleName -like "*v1.0.0-rc1*") {
    throw "Refusing to target immutable RC1 directory '$bundleName'."
}

$bundleDir = Join-Path $OutputBase $bundleName
$binDir = Join-Path $bundleDir "bin"
$extDir = Join-Path $bundleDir "extension"
$cfgDir = Join-Path $bundleDir "config"
$docsDir = Join-Path $bundleDir "docs"

Write-Host "==> Building agsd, ags, and AgentStudios ($Profile) into $targetDir..."
Push-Location $rustDir
try {
    if ($Profile -eq "release") {
        & cargo build --release --bin agsd --bin ags --bin AgentStudios
    } else {
        & cargo build --bin agsd --bin ags --bin AgentStudios
    }
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}

Write-Host "==> Staging bundle directories at $bundleDir..."
if (Test-Path $bundleDir) {
    Remove-Item -Recurse -Force $bundleDir
}
New-Item -ItemType Directory -Force -Path $binDir, $extDir, $cfgDir, $docsDir | Out-Null

$vsixOut = Join-Path $extDir "agent-studios-vscode-$Version.vsix"
Write-Host "==> Building and packaging VS Code / Code-OSS extension directly to $vsixOut..."
Push-Location $vscodeDir
try {
    & npm run build
    if ($LASTEXITCODE -ne 0) { throw "npm run build failed" }
    & npx @vscode/vsce package --no-dependencies --out $vsixOut
    if ($LASTEXITCODE -ne 0) { throw "vsce package failed" }
} finally {
    Pop-Location
}

$profileBinDir = Join-Path $targetDir $Profile
Copy-Item -Force (Join-Path $profileBinDir "agsd.exe") (Join-Path $binDir "agsd.exe")
Copy-Item -Force (Join-Path $profileBinDir "ags.exe") (Join-Path $binDir "ags.exe")
Copy-Item -Force (Join-Path $profileBinDir "AgentStudios.exe") (Join-Path $binDir "AgentStudios.exe")

Copy-Item -Force (Join-Path $repoRoot "installer\windows\product.json") (Join-Path $cfgDir "product.json")
Copy-Item -Force (Join-Path $repoRoot "installer\windows\AgentStudios.nsi") (Join-Path $bundleDir "AgentStudios.nsi")
Copy-Item -Force (Join-Path $repoRoot "installer\windows\Install-AgentStudios.ps1") (Join-Path $bundleDir "Install-AgentStudios.ps1")
Copy-Item -Force (Join-Path $repoRoot "installer\windows\Uninstall-AgentStudios.ps1") (Join-Path $bundleDir "Uninstall-AgentStudios.ps1")

foreach ($legalFile in @("LICENSE", "NOTICE", "README.md")) {
    $legalPath = Join-Path $repoRoot $legalFile
    if (Test-Path $legalPath) {
        Copy-Item -Force $legalPath (Join-Path $docsDir $legalFile)
    }
}

$defaultToml = @"
# Agent Studios Canonical Windows Configuration (%APPDATA%\AgentStudios\config.toml)
[ui]
display_language = "en-US"
agent_response_language = "auto"
default_mode = "agent"

[storage]
state_db = "%LOCALAPPDATA%\\AgentStudios\\state.db"
artifacts_dir = "%LOCALAPPDATA%\\AgentStudios\\artifacts"

[security]
zero_billable_guard = true
fail_closed_sandbox = true
"@
Set-Content -Path (Join-Path $cfgDir "config.default.toml") -Value $defaultToml -Encoding UTF8

$manifest = [ordered]@{
    product = "Agent Studios"
    version = $Version
    bundle_name = $bundleName
    profile = $Profile
    target = "win32-x64"
    signed = $false
    signing_status = "UNSIGNED_RC2_REQUIRES_OPERATOR_CERT"
    source_commit = $commitSha
    short_sha = $shortSha
    binaries = @("ags.exe", "agsd.exe", "AgentStudios.exe")
    extension = (Split-Path -Leaf $vsixOut)
    built_at = (Get-Date).ToUniversalTime().ToString("o")
}
$manifest | ConvertTo-Json -Depth 4 | Set-Content -Path (Join-Path $bundleDir "RELEASE_MANIFEST.json") -Encoding UTF8

$hashLines = Get-ChildItem -Path $bundleDir -Recurse -File | Where-Object { $_.Name -ne "SHA256SUMS.txt" } | ForEach-Object {
    $hash = (Get-FileHash -Path $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    $rel = $_.FullName.Substring($bundleDir.Length + 1).Replace('\', '/')
    "$hash  $rel"
}
Set-Content -Path (Join-Path $bundleDir "SHA256SUMS.txt") -Value $hashLines -Encoding UTF8

$zipPath = Join-Path $OutputBase "$bundleName.zip"
Compress-Archive -Path "$bundleDir\*" -DestinationPath $zipPath -Force
$zipHash = (Get-FileHash -Path $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -Path "$zipPath.sha256" -Value "$zipHash  $(Split-Path -Leaf $zipPath)" -Encoding UTF8

Write-Host "==> Bundle staged cleanly at $bundleDir (archive: $zipPath)"
