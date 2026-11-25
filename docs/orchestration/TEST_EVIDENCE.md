# Test & Quality Gate Evidence Ledger (`TEST_EVIDENCE.md`)

> **Zero Billable Invariant**: All runs below operate strictly under `$0.00` external LLM API spend (`AGENT_STUDIOS_LIVE_CLI_SMOKE` unset).

## Historical Baselines (Reference Only)
- `8040d195a6` (`integration/m15-1-joint-validation` prior tip): Documented GitHub Actions Run `#42094782523` green on PR #33 (`65397d709d`), 685 TS unit + 84 E2E + 25 extension host tests on PR #34.
- `c98a8a3572` (PR #31): Documented 227 passing tests across provider, control-plane, cli-client, tui, cli.

## Live Verified Runs on Current Branch (`integration/m15-1-joint-validation`)

### 1. Rust Workspace (`agent-studios-rs`)
- **Commit verified**: `402312516a56912f10db2cf9163f486e80877fc9` (unchanged Rust sources at `58293983af34ccf46ac44f2b44e224a33af31ba8`, which only updated `scripts/packaging/Verify-ReleaseBundle.ps1`)
- **Target directory**: `C:\AgentStudiosBuild\worktrees\m15-1-joint-validation`
- **Formatting & Static Analysis**:
  - `cargo fmt --check`: `exit 0` (ran before workspace tests; `cargo fmt --check` without `--all` avoids Windows OS error 206)
  - `cargo check --workspace --all-targets`: `exit 0` (`task-361`)
  - `cargo clippy --workspace --all-targets -- -D warnings`: `exit 0` (`task-361`, `task-455`)
- **Full Workspace Test Suite** (`task-489`, with `RUST_MIN_STACK=8388608` matching `.github/workflows/agent-studios-unified-ci.yml` and `AGENT_STUDIOS_LIVE_CLI_SMOKE` unset):
  - `cargo test --workspace`: **`exit 0`**
  - Aggregated `test result:` counts across **`114` test targets / doc-test suites**: **`1014 passed; 0 failed; 0 ignored`**
  - Note on local run without `RUST_MIN_STACK`: `thread_e2e_tests` in `agent-studios-runtime-session` hits Windows default 1 MB debug thread stack overflow (`0xc00000fd`) unless `RUST_MIN_STACK=8388608` (CI setting) is present; with `RUST_MIN_STACK=8388608`, `thread_e2e_tests` passed `7/7` standalone (`task-474`) and passed inside full `cargo test --workspace` (`task-489`).
- **Binary Builds**:
  - `cargo build --bin agsd --bin ags`: `exit 0` (`agsd.exe` and `ags.exe` in `C:\AgentStudiosBuild\worktrees\m15-1-joint-validation\debug`)

### 2. Frontend Extension (`apps/agent-studios-vscode`)
- **Static & Unit Gate** (`task-370`):
  - `npm run lint`: `exit 0` (`0 errors`, `352` pre-existing warnings in untouched files)
  - `npm run typecheck`: `exit 0`
  - `npm run build`: `exit 0`
  - `npm run test:unit`: `exit 0` — **`699 passing (16s)`**
- **Real Daemon E2E Gate** (`task-500`, `AGSD_BIN_PATH=C:\AgentStudiosBuild\worktrees\m15-1-joint-validation\debug\agsd.exe`, `GITHUB_ACTIONS` unset):
  - `npm run test:e2e`: `exit 0` — **`84 passing (5s)`** (Tier 2 mock + Tier 3 live compiled `agsd.exe` IPC, auth, artifact streaming, crash recovery, and single-icon / capability gating suites)
- **Not Executed in This Offline Session**:
  - `npm run test:integration` (VS Code Extension Host GUI test runner) was not executed locally here.

### 3. Windows x64 RC1 Distribution Bundle (`scripts/packaging/`)
- **Verification Run** (`task-530` on source commit `58293983af34ccf46ac44f2b44e224a33af31ba8`):
  - `Build-WindowsReleaseBundle.ps1`: `exit 0`; packaged `agent-studios-vscode-0.1.0.vsix` (`91 files, 270.59 KB`) and staged `C:\AgentStudiosBuild\release-bundle\AgentStudios-v1.0.0-rc1-win-x64`
  - `Verify-ReleaseBundle.ps1`: `exit 0`:
    - Verified `9` SHA-256 checksums cleanly against `SHA256SUMS.txt`
    - `ags.exe --version` -> `ags 0.1.0`
    - `ags.exe doctor --json` -> passed (`>= 8` diagnostic checks)
    - `Install-AgentStudios.ps1 -DryRun` & `Uninstall-AgentStudios.ps1 -DryRun` -> passed
  - `RELEASE_MANIFEST.json`: `version: "1.0.0-rc1"`, `target: "win32-x64"`, `signed: false`, `signing_status: "UNSIGNED_RC1_REQUIRES_OPERATOR_CERT"`, `source_commit: "58293983af34ccf46ac44f2b44e224a33af31ba8"`

### 4. Wave 6 Verification (`ace0f6b02628b796d5051ef8d8a660cbbfb6fd81`)
- **Self-Contained Windows Stack Hardening (`task-593`)**:
  - `cargo test -p agent-studios-runtime-session --test thread_e2e_tests` (**without** `RUST_MIN_STACK`): `exit 0` — **`7 passed; 0 failed` (`7.89s`)** using `run_with_large_stack` (8 MB worker stack).
- **Daemon-Connected TUI & Targeted Rust Gates (`task-624`)**:
  - `cargo fmt --check`: `exit 0`
  - `cargo clippy -p agent-studios-cli -p agent-studios-runtime-session --all-targets -- -D warnings`: `exit 0`
  - `cargo test -p agent-studios-cli`: `exit 0` — `6 passed` (`ags` unit tests) + `7 passed` (`cli_acceptance_tests`)
- **Full Frontend & Real VS Code Extension Host Suite (`task-630`, with `LocalizationManager` wired in `extension.ts`)**:
  - `npm run lint`: `exit 0` (`0 errors`, `352` pre-existing warnings)
  - `npm run typecheck`: `exit 0`
  - `npm run build`: `exit 0`
  - `npm run test:unit`: `exit 0` — **`699 passing (2s)`**
  - `npm run test:e2e`: `exit 0` — **`84 passing (5s)`** (live `agsd.exe`)
  - `npm run test:integration` (real VS Code Extension Development Host GUI process): `exit 0` — **`25 passing (14s)`**
- **Release Profile (`--release`) Bundle & ZIP Distribution (`task-641`)**:
  - `Build-WindowsReleaseBundle.ps1 -Profile release`: `exit 0`; built optimized `--release` `agsd.exe` and `ags.exe` (`10m 26s`), packaged `agent-studios-vscode-0.1.0.vsix` (`91 files, 270.86 KB`), created `C:\AgentStudiosBuild\release-bundle\AgentStudios-v1.0.0-rc1-win-x64.zip`.
  - `Verify-ReleaseBundle.ps1`: `exit 0`:
    - Verified `9` SHA-256 checksums in `SHA256SUMS.txt`
    - `--release` `ags.exe --version` -> `ags 0.1.0` & `ags.exe doctor --json` -> passed
    - `Install-AgentStudios.ps1 -DryRun` & `Uninstall-AgentStudios.ps1 -DryRun` -> passed
    - Verified `AgentStudios-v1.0.0-rc1-win-x64.zip` SHA-256: `c40dbb84726afd01cb97310928ab3905b08cd3afe46f04785ba61653a4698ca`
  - `RELEASE_MANIFEST.json`: `profile: "release"`, `source_commit: "ace0f6b02628b796d5051ef8d8a660cbbfb6fd81"`, `signing_status: "UNSIGNED_RC1_REQUIRES_OPERATOR_CERT"`

### 5. Wave 7 Cross-Platform CI Fix & Green Matrix Verification (`41480cfa025e41df105a3c01ecda2307b51fb180`)
- **Frontend Local Verification (`task-727`, post-`AgsdBinaryResolver.ts` Windows `path.win32` fix)**:
  - `npm run lint`: `exit 0` (`0` errors, `352` pre-existing warnings)
  - `npm run typecheck`: `exit 0`
  - `npm run build`: `exit 0`
  - Direct helper check: `WIN_NORM=d:\projects\agent-studios\agent-studios-rs\target\debug\agsd.exe`, `WIN_SUB=true`, `WIN_SIBLING=false`
  - `npm run test:unit`: `exit 0` — **`700 passing (3s)`**
  - `npm run test:e2e`: `exit 0` — **`84 passing (7s)`** (live `agsd.exe`)
  - `npm run test:integration` (real VS Code Extension Development Host GUI): `exit 0` — **`25 passing (19s)`**
- **GitHub Actions Remote Cross-Platform CI (`41480cfa025e41df105a3c01ecda2307b51fb180`)**:
  - **Run `45590072851`** (`pull_request`, https://github.com/leduwn/agent-studios/actions/runs/45590072851): **`completed / success`**
    - `Unified CI (windows-latest)`: **`completed / success`** (`job/139121602810`)
    - `Unified CI (ubuntu-latest)`: **`completed / success`** (`job/139121603348`)
  - **Run `45590064391`** (`push`, https://github.com/leduwn/agent-studios/actions/runs/45590064391): **`completed / success`**
    - `Unified CI (ubuntu-latest)`: **`completed / success`** (`job/139121568833`)
    - `Unified CI (windows-latest)`: **`completed / success`** (`job/139121569523`)
  - **PR `#35`**: `state=open`, `draft=True`, `mergeable=True`, `mergeable_state=clean`
- **Final Release Profile (`--release`) Bundle Rebuild (`task-753`, pinned to green CI commit `41480cfa02`)**:
  - `Build-WindowsReleaseBundle.ps1 -Profile release` (`2m 02s` incremental) + `Verify-ReleaseBundle.ps1`: **`exit 0`** (`9` SHA-256 checksums, `--release` `ags.exe --version`, `ags.exe doctor --json`, `-DryRun` installer/uninstaller, and ZIP checksum all passed)
  - `AgentStudios-v1.0.0-rc1-win-x64.zip` SHA-256: `3ce6d4e8fd307c78042bb297171af0c174382ef01f88435dd11c483e4c8c3eb9`
  - `RELEASE_MANIFEST.json`: `profile: "release"`, `source_commit: "41480cfa025e41df105a3c01ecda2307b51fb180"`, `signing_status: "UNSIGNED_RC1_REQUIRES_OPERATOR_CERT"`

### 6. Wave 8 Merge & Cleanup + Post-RC1 Baseline Check (`00e5c862b8173cb29914a4ff8dfe4dbb1147e03c`)
- **GitHub Actions on Final Merge Commit `00e5c862b8173cb29914a4ff8dfe4dbb1147e03c`**: Run `45597809243` (`pull_request`) verified green on Ubuntu and Windows.
- **Main Convergence (`task-797`)**: Fast-forwarded `origin/integration/m15-unified-codebase` and `origin/main` + local `main` to `00e5c862b8173cb29914a4ff8dfe4dbb1147e03c`.
- **RC1 Integrity Re-Verification (`task-819` & `task-828`)**:
  - `Verify-ReleaseBundle.ps1` passed cleanly after freeing `+96.85 GB` across Drives `C:` and `D:`.
  - `AgentStudios-v1.0.0-rc1-win-x64.zip` actual SHA-256 confirmed identical: `3ce6d4e8fd307c78042bb297171af0c174382ef01f88435dd11c483e4c8c3eb9`.

### 7. MASTER MISSION 03 — RC2 Product Completion Gates (`feat/rc2-product-completion`, Verified SHA `6a867172a8`)
- **Shared Target Directory**: `C:\AgentStudiosBuild\cargo-target-rc2` (`$0.00` live spend guard preserved)
- **Rust Quality & Workspace Verification (`task-1146` / `task-1158`)**:
  - `cargo fmt --check`: `exit 0`
  - `cargo clippy --workspace --all-targets -- -D warnings`: `exit 0` (`0` warnings)
  - `cargo test --workspace`: `exit 0` — **`119 test binaries: 1021 passed, 0 failed, 0 ignored`**
  - `AgentStudios.exe --headless-check --json`: `exit 0` (live process telemetry verified)
  - `agent-studios-extensibility`: **`5 passed; 0 failed`** (includes `wiremock` Streamable HTTP MCP handshake + tools/resources/prompts + secret redaction)
  - `agent-studios-cli-client`: **`30 passed; 0 failed`** across 6 test suites (includes 3 `$0.00`-guarded `live_harness_tests`)
- **Frontend Extension Verification (`task-1052`, `task-1100`, commit `33af678578`)**:
  - `npm run lint`: `exit 0` (`0` errors)
  - `npm run typecheck` & `npm run build`: `exit 0`
  - `npm run test:unit`: `exit 0` — **`706 passing`**
  - `npm run test:e2e`: `exit 0` — **`84 passing`** (with `AGSD_BIN_PATH=C:\AgentStudiosBuild\cargo-target-rc2\debug\agsd.exe`)
  - `npm run test:integration`: `exit 0` — **`25 passing`** in real VS Code Extension Host
- **RC2 Release Bundle & Real Sandbox Install/Upgrade/Uninstall (`task-1158`)**:
  - `Build-WindowsReleaseBundle.ps1 -Profile release -Version 1.0.0-rc2`: `exit 0`
  - Packaged `C:\AgentStudiosBuild\release-bundle\AgentStudios-v1.0.0-rc2-6a867172a8-win-x64` (`AgentStudios-v1.0.0-rc2-6a867172a8-win-x64.zip`, `22,624,975` bytes, SHA-256 `49a7a6c45affb88e5f3f11cba43b3f36a392af9b617ecc0df50cd5e31425dbda`)
  - `Verify-ReleaseBundle.ps1`: `exit 0` — verified 13 `SHA256SUMS.txt` hashes + ZIP checksum + `--release` `ags.exe doctor --json` + `AgentStudios.exe --headless-check --json` + real isolated temp sandbox install -> custom `config.toml` upgrade preservation -> installed `AgentStudios.exe` execution -> uninstall/purge
  - Preserved RC1 `AgentStudios-v1.0.0-rc1-win-x64.zip` SHA-256 re-confirmed unchanged: `3ce6d4e8fd307c78042bb297171af0c174382ef01f88435dd11c483e4c8c3eb9`

### 8. MASTER MISSION 04 — Extension-First Product Completion Gates (`feat/extension-first-product-completion`, Verified SHA `2e3195367f`)
- **Upstream Sync Verification (`cf223bd286`)**: `codex-rs/` synced `d25c114d49` -> `44dd77b71e` (26 commits / 152 files, Patches `001`–`004` preserved); `cargo check --workspace --all-targets`: `exit 0`.
- **Rust Quality & Workspace Verification (`task-1676` & `task-1744`)**:
  - `cargo fmt --check`: `exit 0`
  - `cargo clippy --workspace --all-targets -- -D warnings`: `exit 0`
  - `cargo test --workspace -j 1` (with `CARGO_INCREMENTAL=0`, `RUST_MIN_STACK=8388608`, `$0.00` live spend guard): `exit 0` — **`120 suites: 1022 passed, 0 failed, 0 ignored`** (includes `rc3_extension_first_ipc_tests` verifying `UpsertProvider`, HTTP `/v1/models` `DiscoverModels`, `TestProviderConnection`, `ForkStudio`, `ListExtensibility`, `RemoveProvider`)
- **Frontend Extension Verification (`task-1665`, with `AGSD_BIN_PATH=C:\AgentStudiosBuild\cargo-target-rc2\debug\agsd.exe`)**:
  - `npm run lint`: `exit 0` (`0` errors)
  - `npm run typecheck`: `exit 0`
  - `npm run build`: `exit 0`
  - `npm run test:unit`: `exit 0` — **`706 passing`**
  - `npm run test:e2e`: `exit 0` — **`89 passing`** (includes **Journeys A–G** against real `agsd.exe` + `$0.00` HTTP `/v1/models` mock + SQLite WAL restart durability)
  - `npm run test:integration`: `exit 0` — **`25 passing`** in real VS Code Extension Host
- **Self-Contained VSIX `v1.1.0` Packaging & Clean-Profile Smoke Test (`task-1750`)**:
  - `Build-ExtensionFirstBundle.ps1 -Profile release -Version 1.1.0`: `exit 0` — packaged `C:\AgentStudiosBuild\release-bundle\agent-studios-vscode-1.1.0-win32-x64.vsix` (`17.67 MB`, `101` files, SHA-256 `c5d33695419db6028a35adc9ece00d939185d36806e306596b473ce4a560a622`) bundling `bin/win32-x64/agsd.exe` (`2b1dd9e541652c9853d1a2d0e17d2c61cbe28eecf8ef979e74c87c7737d7a3c4`), `bin/win32-x64/ags.exe` (`df127c8286e25e6f6126f80643ff9fbaae8f3ad8ed7961ed213d4d8d1088f48c`), and `SHA256SUMS.txt`.
  - Clean VS Code profile smoke install (`code.cmd --user-data-dir <temp> --extensions-dir <temp> --install-extension ... --force`): installed `agent-studios.agent-studios-vscode@1.1.0`, verified `agsd.exe`, `ags.exe`, `SHA256SUMS.txt` extracted inside `bin\win32-x64\` and `ags.exe --version` -> `ags 0.1.0`.
  - Re-verified preserved immutable archives: RC1 (`3ce6d4e8fd307c78042bb297171af0c174382ef01f88435dd11c483e4c8c3eb9`) and RC2 (`49a7a6c45affb88e5f3f11cba43b3f36a392af9b617ecc0df50cd5e31425dbda`).

### 9. MASTER MISSION 05 — Real Coding Execution & Parity (`fix/extension-real-execution-and-parity`, Verified SHA `f87e284915`)
- **Rust Quality & Workspace Verification (`task-2489`, `$0.00` live spend guard)**:
  - `cargo fmt --check`: `exit 0`
  - `cargo clippy -j 1 --workspace --all-targets -- -D warnings`: `exit 0`
  - `cargo test --workspace -j 1`: `exit 0` — **`121 suites: 1026 passed, 0 failed, 0 ignored`** (includes `real_codex_ipc_execution_tests`: `4/4` passed)
  - `cargo build --release --bin agsd --bin ags -j 1`: `exit 0`
- **Frontend Extension Verification (`task-2452`, `AGSD_BIN_PATH=C:\AgentStudiosBuild\cargo-target-rc2\release\agsd.exe`)**:
  - `npm run typecheck`, `npm run lint`, `npm run build`: `exit 0`
  - `npm run test:unit`: `exit 0` — **`706 passing (3s)`**
  - `npm run test:e2e`: `exit 0` — **`93 passing (23s)`** (includes `RealCodexExecutionAcceptanceE2E` `J1`–`J4` against release `agsd.exe` + `DeterministicInferenceServer`)
  - `npm run test:integration`: `exit 0` — **`25 passing (14s)`** in real VS Code Extension Host
- **Self-Contained VSIX `v1.1.1-f87e284915` & Clean Isolated Profile Extension Host Acceptance (`task-2504`, `task-2525`)**:
  - `Build-ExtensionFirstBundle.ps1 -Profile release -Version 1.1.1 -SkipCargoBuild`: `exit 0` — packaged `C:\AgentStudiosBuild\release-bundle\agent-studios-vscode-1.1.1-f87e284915-win32-x64.vsix` (`71.08 MB`, `101 files`, SHA-256 `1da4a7ac5d393768c3ae391df8a5e9e0bb5d0b76d9e56c6fcb3fc42bdef23e51`).
  - Clean isolated VS Code profile + Extension Host (`AGSD_BIN_PATH` unset): installed `agent-studios.agent-studios-vscode@1.1.1`, resolved bundled `bin/win32-x64/agsd.exe`, booted daemon, rejected 401 wrong key, discovered `codex-clean-vsix` via 200 `/v1/models`, and completed streaming Codex chat turn (`ThreadId 01a21425-1a25-71c1-9e40-6c1495fe7211`) with `AGENTS.md` and `$audit` `SKILL.md` verified in HTTP request payload (`exit 0`).
  - Preserved artifact SHA-256 hashes re-confirmed unchanged: RC1 (`3ce6d4e8fd307c78042bb297171af0c174382ef01f88435dd11c483e4c8c3eb9`), RC2 (`49a7a6c45affb88e5f3f11cba43b3f36a392af9b617ecc0df50cd5e31425dbda`), and `v1.1.0` VSIX (`c5d33695419db6028a35adc9ece00d939185d36806e306596b473ce4a560a622`).
