<#
.SYNOPSIS
    Verifies integrity, binary execution (ags, AgentStudios.exe), VSIX presence, SHA-256 checksums,
    installer dry-run, AND a real isolated sandbox install -> upgrade -> headless execution -> uninstall cycle.
#>
[CmdletBinding()]
param(
    [string]$BundleDir
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $BundleDir) {
    $releaseBase = "C:\AgentStudiosBuild\release-bundle"
    $latestRc2 = Get-ChildItem -Path $releaseBase -Directory -Filter "AgentStudios-v1.0.0-rc2-*-win-x64" |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $latestRc2) {
        throw "No RC2 bundle directory found under $releaseBase"
    }
    $BundleDir = $latestRc2.FullName
}

if (-not (Test-Path $BundleDir)) {
    throw "Release bundle directory does not exist: $BundleDir"
}

$requiredFiles = @(
    "bin\ags.exe",
    "bin\agsd.exe",
    "bin\AgentStudios.exe",
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

$vsixFiles = Get-ChildItem -Path (Join-Path $BundleDir "extension") -Filter "*.vsix"
if (-not $vsixFiles -or $vsixFiles.Count -lt 1) {
    throw "Missing required VSIX package under $(Join-Path $BundleDir 'extension')"
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
$desktopExe = Join-Path $BundleDir "bin\AgentStudios.exe"

Write-Host "==> Running ags.exe --version & doctor --json..."
& $agsExe --version
if ($LASTEXITCODE -ne 0) { throw "ags.exe --version failed" }
$doctorOut = & $agsExe doctor --json
if ($LASTEXITCODE -ne 0) { throw "ags.exe doctor --json failed" }
$doctorJson = $doctorOut | ConvertFrom-Json
if ($doctorJson.checks.Count -lt 8) {
    throw "Expected at least 8 diagnostic checks from ags doctor --json"
}

Write-Host "==> Running bundled AgentStudios.exe --headless-check --json..."
$headlessOut = & $desktopExe --headless-check --json
if ($LASTEXITCODE -ne 0) { throw "AgentStudios.exe --headless-check --json failed" }
$headlessJson = $headlessOut | ConvertFrom-Json
if ($headlessJson.product -ne "Agent Studios" -or -not $headlessJson.agsd_binary) {
    throw "Bundled AgentStudios.exe headless-check missing expected product or agsd_binary"
}

Write-Host "==> Running installer/uninstaller dry-runs..."
$installScript = Join-Path $BundleDir "Install-AgentStudios.ps1"
$uninstallScript = Join-Path $BundleDir "Uninstall-AgentStudios.ps1"
& $installScript -BundleRoot $BundleDir -DryRun
if ($LASTEXITCODE -ne 0) { throw "Install-AgentStudios.ps1 -DryRun failed" }
& $uninstallScript -DryRun
if ($LASTEXITCODE -ne 0) { throw "Uninstall-AgentStudios.ps1 -DryRun failed" }

Write-Host "==> Running isolated real sandbox install -> upgrade -> headless-check -> uninstall smoke test..."
$sandboxRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("ags-sandbox-" + [Guid]::NewGuid().ToString("N"))
$sandboxInstall = Join-Path $sandboxRoot "Programs\AgentStudios"
$sandboxAppData = Join-Path $sandboxRoot "AppData"
$sandboxLocalAppData = Join-Path $sandboxRoot "LocalAppData"
try {
    & $installScript -BundleRoot $BundleDir -InstallDir $sandboxInstall -AppDataRoot $sandboxAppData -LocalAppDataRoot $sandboxLocalAppData
    if ($LASTEXITCODE -ne 0) { throw "Sandbox Install-AgentStudios.ps1 failed" }

    $sandboxConfig = Join-Path $sandboxAppData "AgentStudios\config.toml"
    if (-not (Test-Path $sandboxConfig)) { throw "Sandbox config.toml not provisioned" }
    Add-Content -Path $sandboxConfig -Value "`n# custom_user_setting = true"

    # Upgrade install must preserve existing config.toml
    & $installScript -BundleRoot $BundleDir -InstallDir $sandboxInstall -AppDataRoot $sandboxAppData -LocalAppDataRoot $sandboxLocalAppData
    if ($LASTEXITCODE -ne 0) { throw "Sandbox upgrade Install-AgentStudios.ps1 failed" }
    $cfgAfterUpgrade = Get-Content $sandboxConfig -Raw
    if ($cfgAfterUpgrade -notlike "*custom_user_setting = true*") {
        throw "Upgrade install overwrote custom user config.toml!"
    }

    $installedDesktopExe = Join-Path $sandboxInstall "bin\AgentStudios.exe"
    if (-not (Test-Path $installedDesktopExe)) { throw "Installed AgentStudios.exe missing in sandbox" }
    $installedCheckOut = & $installedDesktopExe --headless-check --json
    if ($LASTEXITCODE -ne 0) { throw "Installed sandbox AgentStudios.exe --headless-check failed" }
    $installedJson = $installedCheckOut | ConvertFrom-Json
    if ($installedJson.product -ne "Agent Studios" -or -not $installedJson.bundled_vsix) {
        throw "Installed sandbox AgentStudios.exe did not resolve bundled_vsix or product"
    }

    & $uninstallScript -InstallDir $sandboxInstall -AppDataRoot $sandboxAppData -LocalAppDataRoot $sandboxLocalAppData -PurgeUserData
    if ($LASTEXITCODE -ne 0) { throw "Sandbox Uninstall-AgentStudios.ps1 failed" }
    if (Test-Path $sandboxInstall) { throw "Sandbox InstallDir still exists after uninstall" }
} finally {
    if (Test-Path $sandboxRoot) {
        Remove-Item -Recurse -Force $sandboxRoot -ErrorAction SilentlyContinue
    }
}
Write-Host "    Isolated sandbox install/upgrade/execute/uninstall passed cleanly."

$parentDir = Split-Path -Parent $BundleDir
$bundleLeaf = Split-Path -Leaf $BundleDir
$zipPath = Join-Path $parentDir "$bundleLeaf.zip"
$zipShaPath = "$zipPath.sha256"
if (-not (Test-Path $zipPath)) { throw "Missing release ZIP archive: $zipPath" }
if (-not (Test-Path $zipShaPath)) { throw "Missing release ZIP checksum: $zipShaPath" }
$expectedZipHash = ((Get-Content $zipShaPath | Select-Object -First 1) -split "\s+", 2)[0].Trim().ToLowerInvariant()
$actualZipHash = (Get-FileHash -Path $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualZipHash -ne $expectedZipHash) {
    throw "Release ZIP checksum mismatch: expected $expectedZipHash, got $actualZipHash"
}
Write-Host "    Verified release ZIP archive checksum cleanly ($zipPath)."

Write-Host "==> ALL RELEASE BUNDLE VERIFICATION CHECKS PASSED."
