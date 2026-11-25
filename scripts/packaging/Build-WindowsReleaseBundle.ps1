<#
.SYNOPSIS
    Builds and stages the Agent Studios Windows x64 RC1 standalone distribution bundle on Drive C:.
#>
[CmdletBinding()]
param(
    [ValidateSet("debug", "release")]
    [string]$Profile = "debug",
    [string]$OutputBase = "C:\AgentStudiosBuild\release-bundle"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$rustDir = Join-Path $repoRoot "agent-studios-rs"
$vscodeDir = Join-Path $repoRoot "apps\agent-studios-vscode"
$targetDir = "C:\AgentStudiosBuild\worktrees\m15-1-joint-validation"
$env:CARGO_TARGET_DIR = $targetDir

$bundleDir = Join-Path $OutputBase "AgentStudios-v1.0.0-rc1-win-x64"
$binDir = Join-Path $bundleDir "bin"
$extDir = Join-Path $bundleDir "extension"
$cfgDir = Join-Path $bundleDir "config"
$docsDir = Join-Path $bundleDir "docs"

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

Write-Host "==> Building and packaging VS Code / Code-OSS extension..."
Push-Location $vscodeDir
try {
    & npm run build
    if ($LASTEXITCODE -ne 0) { throw "npm run build failed" }
    & npx @vscode/vsce package --no-dependencies
    if ($LASTEXITCODE -ne 0) { throw "vsce package failed" }
} finally {
    Pop-Location
}

Write-Host "==> Staging bundle at $bundleDir..."
if (Test-Path $bundleDir) {
    Remove-Item -Recurse -Force $bundleDir
}
New-Item -ItemType Directory -Force -Path $binDir, $extDir, $cfgDir, $docsDir | Out-Null

$profileBinDir = Join-Path $targetDir $Profile
Copy-Item -Force (Join-Path $profileBinDir "agsd.exe") (Join-Path $binDir "agsd.exe")
Copy-Item -Force (Join-Path $profileBinDir "ags.exe") (Join-Path $binDir "ags.exe")

$vsixSource = Get-ChildItem -Path $vscodeDir -Filter "*.vsix" | Select-Object -First 1
if (-not $vsixSource) { throw "VSIX package not found in $vscodeDir" }
Copy-Item -Force $vsixSource.FullName (Join-Path $extDir "agent-studios-vscode-0.1.0.vsix")

Copy-Item -Force (Join-Path $repoRoot "installer\windows\product.json") (Join-Path $cfgDir "product.json")
Copy-Item -Force (Join-Path $repoRoot "installer\windows\AgentStudios.nsi") (Join-Path $bundleDir "AgentStudios.nsi")
Copy-Item -Force (Join-Path $repoRoot "installer\windows\Install-AgentStudios.ps1") (Join-Path $bundleDir "Install-AgentStudios.ps1")
Copy-Item -Force (Join-Path $repoRoot "installer\windows\Uninstall-AgentStudios.ps1") (Join-Path $bundleDir "Uninstall-AgentStudios.ps1")

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

$commitSha = (& git -C $repoRoot rev-parse HEAD).Trim()
$manifest = [ordered]@{
    product = "Agent Studios"
    version = "1.0.0-rc1"
    profile = $Profile
    target = "win32-x64"
    signed = $false
    signing_status = "UNSIGNED_RC1_REQUIRES_OPERATOR_CERT"
    source_commit = $commitSha
    built_at = (Get-Date).ToUniversalTime().ToString("o")
}
$manifest | ConvertTo-Json -Depth 4 | Set-Content -Path (Join-Path $bundleDir "RELEASE_MANIFEST.json") -Encoding UTF8

# Generate SHA256SUMS.txt
$hashLines = Get-ChildItem -Path $bundleDir -Recurse -File | Where-Object { $_.Name -ne "SHA256SUMS.txt" } | ForEach-Object {
    $hash = (Get-FileHash -Path $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    $rel = $_.FullName.Substring($bundleDir.Length + 1).Replace('\', '/')
    "$hash  $rel"
}
Set-Content -Path (Join-Path $bundleDir "SHA256SUMS.txt") -Value $hashLines -Encoding UTF8

# Package standalone distributable ZIP archive + checksum
$zipPath = Join-Path $OutputBase "AgentStudios-v1.0.0-rc1-win-x64.zip"
Compress-Archive -Path "$bundleDir\*" -DestinationPath $zipPath -Force
$zipHash = (Get-FileHash -Path $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -Path "$zipPath.sha256" -Value "$zipHash  $(Split-Path -Leaf $zipPath)" -Encoding UTF8

Write-Host "==> Bundle staged cleanly at $bundleDir (archive: $zipPath)"
