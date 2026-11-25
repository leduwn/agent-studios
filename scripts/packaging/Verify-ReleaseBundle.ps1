<#
.SYNOPSIS
    Verifies integrity, binary execution, VSIX presence, installer dry-run, and SHA-256 checksums of the release bundle.
#>
[CmdletBinding()]
param(
    [string]$BundleDir = "C:\AgentStudiosBuild\release-bundle\AgentStudios-v1.0.0-rc1-win-x64"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not (Test-Path $BundleDir)) {
    throw "Release bundle directory does not exist: $BundleDir"
}

$requiredFiles = @(
    "bin\ags.exe",
    "bin\agsd.exe",
    "extension\agent-studios-vscode-0.1.0.vsix",
    "config\product.json",
    "config\config.default.toml",
    "Install-AgentStudios.ps1",
    "Uninstall-AgentStudios.ps1",
    "AgentStudios.nsi",
    "RELEASE_MANIFEST.json",
    "SHA256SUMS.txt"
)

foreach ($f in $requiredFiles) {
    $full = Join-Path $BundleDir $f
    if (-not (Test-Path $full)) {
        throw "Missing required bundle artifact: $full"
    }
}

Write-Host "==> Verifying SHA256SUMS.txt..."
$checksumFile = Join-Path $BundleDir "SHA256SUMS.txt"
$lines = Get-Content $checksumFile | Where-Object { $_.Trim() -ne "" }
foreach ($line in $lines) {
    $parts = $line -split "\s{2,}", 2
    $expectedHash = $parts[0].Trim().ToLowerInvariant()
    $relPath = $parts[1].Trim().Replace('/', '\')
    $fullPath = Join-Path $BundleDir $relPath
    if (-not (Test-Path $fullPath)) {
        throw "Checksum target missing: $fullPath"
    }
    $actualHash = (Get-FileHash -Path $fullPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $expectedHash) {
        throw "Checksum mismatch for ${relPath}: expected $expectedHash, got $actualHash"
    }
}
Write-Host "    Verified $($lines.Count) SHA-256 checksums cleanly."

$agsExe = Join-Path $BundleDir "bin\ags.exe"
Write-Host "==> Running ags.exe --version..."
& $agsExe --version
if ($LASTEXITCODE -ne 0) { throw "ags.exe --version failed" }

Write-Host "==> Running ags.exe doctor --json..."
$doctorOut = & $agsExe doctor --json
if ($LASTEXITCODE -ne 0) { throw "ags.exe doctor --json failed" }
$doctorJson = $doctorOut | ConvertFrom-Json
if ($doctorJson.checks.Count -lt 8) {
    throw "Expected at least 8 diagnostic checks from ags doctor --json"
}

Write-Host "==> Running installer/uninstaller dry-runs..."
& (Join-Path $BundleDir "Install-AgentStudios.ps1") -BundleRoot $BundleDir -DryRun
if ($LASTEXITCODE -ne 0) { throw "Install-AgentStudios.ps1 -DryRun failed" }

& (Join-Path $BundleDir "Uninstall-AgentStudios.ps1") -DryRun
if ($LASTEXITCODE -ne 0) { throw "Uninstall-AgentStudios.ps1 -DryRun failed" }

Write-Host "==> ALL RELEASE BUNDLE VERIFICATION CHECKS PASSED."
