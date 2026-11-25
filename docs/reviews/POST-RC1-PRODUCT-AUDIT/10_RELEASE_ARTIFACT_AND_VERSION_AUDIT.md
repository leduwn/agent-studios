# 10 — Release Artifact & Version Consistency Audit (`RC1`)

> **Preserved Bundle Path**: `C:\AgentStudiosBuild\release-bundle\AgentStudios-v1.0.0-rc1-win-x64`
> **Preserved Archive**: `C:\AgentStudiosBuild\release-bundle\AgentStudios-v1.0.0-rc1-win-x64.zip` (`SHA-256: 3ce6d4e8fd307c78042bb297171af0c174382ef01f88435dd11c483e4c8c3eb9`)

---

## 1. Verified RC1 Artifact & Manifest Inventory

- `RELEASE_MANIFEST.json`: `product: "Agent Studios"`, `version: "1.0.0-rc1"`, `profile: "release"`, `target: "win32-x64"`, `signed: false`, `signing_status: "UNSIGNED_RC1_REQUIRES_OPERATOR_CERT"`, `source_commit: "41480cfa025e41df105a3c01ecda2307b51fb180"`.
- Verified `9` checksum entries in `SHA256SUMS.txt`: `bin/ags.exe` (`ags 0.1.0`), `bin/agsd.exe`, `extension/agent-studios-vscode-0.1.0.vsix` (`91 files, 270.86 KB`), `config/product.json`, `config/config.default.toml`, `AgentStudios.nsi`, `Install-AgentStudios.ps1`, `Uninstall-AgentStudios.ps1`, `RELEASE_MANIFEST.json`.

---

## 2. Release Engineering Defects Identified in RC1 Scripts (`12`-Point Audit)

| # | Audited Question / Defect | Observed in RC1 Scripts? | Fix Planned in Wave 7 (`RC2`) |
|---:|---|---|---|
| 1 | `Build-WindowsReleaseBundle.ps1` defaults to `debug` profile | **YES** (`[string]$Profile = "debug"`) | Default to `"release"`; fail if debug accidentally requested for release tag |
| 2 | Hardcoded `CARGO_TARGET_DIR` (`m15-1-joint-validation`) | **YES** (line 18) | Resolve from `$env:CARGO_TARGET_DIR` or default `C:\AgentStudiosBuild\cargo-target-rc2` |
| 3 | Hardcoded worktree name assumptions | **YES** (line 18) | Parameterize `-CargoTargetDir` |
| 4 | Hardcoded product version (`1.0.0-rc1`) | **YES** (`Build-WindowsReleaseBundle.ps1:90`, `AgentStudios.nsi:5`) | Parameterize `-Version` defaulting to `1.0.0-rc2` and sync manifest |
| 5 | Hardcoded VSIX filename (`agent-studios-vscode-0.1.0.vsix`) | **YES** (line 63) | Pass explicit `--out` to `vsce package` and read `package.json` version |
| 6 | Fragile `Get-ChildItem -Filter "*.vsix" \| Select-Object -First 1` | **YES** (line 61) | Eliminate wildcard lookup via explicit `--out` target in `$extDir` |
| 7 | Risk of picking up stale `.vsix` left in `apps/agent-studios-vscode/` | **YES** (line 61) | Package directly into staging temp / `$extDir`, zero `.vsix` left in repo |
| 8 | New build overwrites prior bundle directory (`Remove-Item $bundleDir`) | **YES** (lines 21, 52–54) | Stage into `AgentStudios-v$Version-$shortSha-win-x64` (never overwrites RC1!) |
| 9 | Manifest lacks dirty-tree check, compiler versions, and SBOM/notices | **PARTIAL** | Add `git status --porcelain` clean check, `rustc --version`, `node --version`, per-artifact SHA-256 map, and Apache-2.0 `LICENSE`/`NOTICE` |
| 10 | Installer bundles CLI + daemon + VSIX only (no desktop executable) | **YES** | Add `bin\AgentStudios.exe` + Start Menu/Desktop shortcut support |
| 11 | `Verify-ReleaseBundle.ps1` runs `-DryRun` only (no real install test) | **YES** (lines 68–73) | Execute **real sandbox install -> custom config preservation upgrade -> installed `AgentStudios.exe --headless-check` -> uninstall** |
| 12 | Version skew (`ags 0.1.0`, extension `0.1.0`, product `1.0.0-rc1`) | **YES** | Document unified semver policy (`1.0.0-rc2` release channel, `1.0.0` VSIX semver, crate workspace alignment) |
