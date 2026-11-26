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

if ($Version -eq "1.1.0" -or $Version -eq "1.1.1") {
    throw "Refusing to overwrite preserved v1.1.0 / v1.1.1 VSIX baseline."
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

    Write-Host "==> Compiling production TypeScript (excluding tests/fixtures) and packaging VSIX to $vsixPath..."
    Push-Location $vscodeDir
    try {
        & npm run build:prod
        if ($LASTEXITCODE -ne 0) { throw "npm run build:prod failed with exit code $LASTEXITCODE" }
        & npx @vscode/vsce package --no-dependencies --out $vsixPath
        if ($LASTEXITCODE -ne 0) { throw "vsce package failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }

    Write-Host "==> Running Production VSIX Hygiene Audit..."
    $auditDir = Join-Path $env:TEMP ("ags-vsix-audit-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force -Path $auditDir | Out-Null
    try {
        $auditZip = Join-Path $auditDir "inspect.zip"
        Copy-Item -Force $vsixPath $auditZip
        Expand-Archive -Path $auditZip -DestinationPath (Join-Path $auditDir "unpacked") -Force
        $extRoot = Join-Path $auditDir "unpacked\extension"
        $leakedTestDir = Join-Path $extRoot "out\test"
        if (Test-Path $leakedTestDir) {
            throw "VSIX HYGIENE FAILURE: extension/out/test was packaged into production VSIX!"
        }
        $forbiddenMarkers = @(
            "FixtureBridge",
            "missionControlFixtures",
            "session-m14-fixture",
            "Developer Preview Mode",
            "PREVIEW FIXTURE",
            "agent-studios-ide-ui",
            "toggleFixturePreview",
            "E2E Genuine Verification Studio",
            "Host Modal Gate (HMAC-SHA256)",
            "Dynamic Provider Discovery pending"
        )
        $migrationScrubberMarkers = @(
            "session-m14-fixture",
            "agent-studios-ide-ui",
            "E2E Genuine Verification Studio"
        )
        $inspectFiles = @(Get-ChildItem -Path (Join-Path $extRoot "out") -Recurse -File -ErrorAction SilentlyContinue)
        if (Test-Path (Join-Path $extRoot "package.json")) {
            $inspectFiles += (Get-Item (Join-Path $extRoot "package.json"))
        }
        foreach ($f in $inspectFiles) {
            $content = Get-Content -Raw -Path $f.FullName -ErrorAction SilentlyContinue
            if (-not $content) { continue }
            $isMigrationFile = $f.FullName.EndsWith("out\state\UserStateMigration.js", [System.StringComparison]::OrdinalIgnoreCase)
            foreach ($marker in $forbiddenMarkers) {
                if ($isMigrationFile -and ($migrationScrubberMarkers -contains $marker)) { continue }
                if ($content.IndexOf($marker, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
                    throw "VSIX HYGIENE FAILURE: Forbidden marker '$marker' found in packaged file $($f.FullName)"
                }
            }
        }
        Write-Host "==> Production VSIX Hygiene Audit PASSED (0 test/fixture markers in $($inspectFiles.Count) packaged files)."
    } catch {
        if (Test-Path $vsixPath) { Remove-Item -Force $vsixPath -ErrorAction SilentlyContinue }
        throw
    } finally {
        if (Test-Path $auditDir) { Remove-Item -Recurse -Force $auditDir -ErrorAction SilentlyContinue }
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
