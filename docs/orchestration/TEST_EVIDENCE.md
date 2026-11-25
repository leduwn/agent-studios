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
