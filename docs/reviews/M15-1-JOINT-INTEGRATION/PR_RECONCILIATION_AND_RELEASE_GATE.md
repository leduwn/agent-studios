# PR Reconciliation & Release Gate Package (`M15-1-JOINT-INTEGRATION`)

> **Branch**: `integration/m15-1-joint-validation` (Isolated Worktree: `D:\Projects\agent-studios-active\m15-1-joint-validation`)
> **Base Target**: `integration/m15-unified-codebase` (PR `#35`) → `main` (PR `#32`)
> **Canonical Safety Boundary**: `D:\Projects\agent-studios` untouched at `8525ec228c37e38c35ee514dc886af295fbc4e00`; `codex-rs/` unmodified.
> **Zero-Billable Invariant**: Strictly `$0.00` external LLM API spend (`AGENT_STUDIOS_LIVE_CLI_SMOKE` unset).

---

## 1. Open Pull Request Reconciliation Matrix

| PR | Branch | Target | Disposition & Ancestry Status | Operator Action |
|---|---|---|---|---|
| **#35** | `integration/m15-1-joint-validation` | `integration/m15-unified-codebase` | Active consolidated branch (`mergeable: True`); supersedes `#31`, `#33`, `#34` cleanly and adds Wave 1–7 engineering & release gates | **1. Review & Merge `#35` → `integration/m15-unified-codebase`** |
| **#33** | `fix/m15-unified-cross-platform-ci` | `main` | Tip `65397d709d` merged cleanly into `integration/m15-1-joint-validation` (`5f7b5f525b`); `0` unmerged commits | Close as superseded once `#35` merges (or fast-forward base first) |
| **#31** | `feat/m15-1-provider-cli-unification` | `main` | Ancestor already integrated (`c98a8a3572`); `0` unmerged commits | Close as superseded once `#35` merges |
| **#34** | `feat/m15-ui-06-agent-first-ux` | `main` | Ancestor already integrated (`f57bc20c74`); `0` unmerged commits | Close as superseded once `#35` merges |
| **#32** | `integration/m15-unified-codebase` | `main` | Canonical M15/M15.1–M19 integration PR targeting protected `main` (`GATE-01`) | **2. Final Review & Merge `#32` → `main`** after green CI on `#35`/`#32` |

### Recommended Operator Merge / Close Sequence (`GATE-01`)
1. Confirm GitHub Actions `Agent Studios Unified Full CI` passes on **both** `Unified CI (ubuntu-latest)` and `Unified CI (windows-latest)` for `integration/m15-1-joint-validation` (see Section 3 for the Run ID & status after Wave 7 push).
2. Convert PR `#35` from Draft to Ready and merge `#35` into `integration/m15-unified-codebase`.
3. Close superseded PRs `#31`, `#33`, and `#34` referencing PR `#35` / `#32`.
4. Review and merge PR `#32` (`integration/m15-unified-codebase` → `main`) under `GATE-01`.

---

## 2. Verified Implementation & Quality Gate Summary

### Backend (`agent-studios-rs`)
- **Full Workspace Suite (`402312516a`, `RUST_MIN_STACK=8388608`)**: `1014 passed; 0 failed; 0 ignored` across `114` test targets (`cargo fmt --check`, `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings` all exit `0`).
- **Wave 6 Hardening & Daemon-Connected TUI (`ace0f6b026`)**:
  - `cargo test -p agent-studios-runtime-session --test thread_e2e_tests` **without** `RUST_MIN_STACK`: `7 passed; 0 failed` (`7.89s`).
  - `cargo test -p agent-studios-cli`: `6` unit tests + `7` CLI acceptance tests passed.
  - `cargo build --release --bin agsd --bin ags`: exit `0`.

### Frontend (`apps/agent-studios-vscode`)
- Wave 7 (`41480cfa02`): `npm run lint` (`0` errors), `npm run typecheck` (`0` errors), `npm run build` (`0` errors), `npm run test:unit` (**`700 passing (3s)`**), `npm run test:e2e` with live `agsd.exe` (**`84 passing (7s)`**), and real VS Code Extension Development Host GUI `npm run test:integration` (**`25 passing (19s)`**).
- Wave 7 CI Fix: `AgsdBinaryResolver.ts` (`normalizePathForComparison` / `isSubpathOrEqual`) updated to select `path.win32` when `isWindows === true`, eliminating the `Unified CI (ubuntu-latest)` Step 19 unit failure observed on Run `45548370010`.

### Windows x64 RC1 Release Bundle (`scripts/packaging/`)
- Output directory: `C:\AgentStudiosBuild\release-bundle\AgentStudios-v1.0.0-rc1-win-x64`
- Distributable archive: `C:\AgentStudiosBuild\release-bundle\AgentStudios-v1.0.0-rc1-win-x64.zip` (`SHA-256: 3ce6d4e8fd307c78042bb297171af0c174382ef01f88435dd11c483e4c8c3eb9`)
- Includes `--release` `agsd.exe` and `ags.exe` (source commit `41480cfa025e41df105a3c01ecda2307b51fb180`), `agent-studios-vscode-0.1.0.vsix`, canonical `product.json` / `config.default.toml`, `AgentStudios.nsi`, `Install-AgentStudios.ps1`, `Uninstall-AgentStudios.ps1`, `RELEASE_MANIFEST.json`, and `SHA256SUMS.txt` (`9` verified file checksums + `.zip.sha256`).

---

## 3. Cross-Platform GitHub Actions CI Status

- **Run `45548370010` (`2777c3d465`)**: `Unified CI (windows-latest)` = `success`; `Unified CI (ubuntu-latest)` = `failure` at Step 19 (`AgsdBinaryResolver.test.ts` Windows path normalization on Linux).
- **Run `45590072851` (`pull_request` on `41480cfa02`, https://github.com/leduwn/agent-studios/actions/runs/45590072851)**:
  - `Unified CI (windows-latest)`: **`completed / success`** (`job/139121602810`)
  - `Unified CI (ubuntu-latest)`: **`completed / success`** (`job/139121603348`)
- **Run `45590064391` (`push` on `41480cfa02`, https://github.com/leduwn/agent-studios/actions/runs/45590064391)**:
  - `Unified CI (ubuntu-latest)`: **`completed / success`** (`job/139121568833`)
  - `Unified CI (windows-latest)`: **`completed / success`** (`job/139121569523`)
- **PR `#35` Status**: `state=open`, `draft=True`, `mergeable=True`, `mergeable_state=clean`.

---

## 4. Remaining Human Approval Gates (`GATE-01`..`GATE-04`)

- [ ] **`GATE-01`**: Authorize PR `#35` → `integration/m15-unified-codebase` and PR `#32` → `main` merge, plus closing superseded `#31`/`#33`/`#34`.
- [ ] **`GATE-02`**: Approve destructive cleanup of legacy `HOLD`/`APPROVED_CANDIDATE` worktrees documented in `09_SAFE_WORKTREE_CLEANUP_PLAN.md`.
- [ ] **`GATE-03`**: Authorize non-zero live LLM spend cap (`$1.00`–`$5.00`) and real provider credentials (`AGENT_STUDIOS_LIVE_CLI_SMOKE=1`).
- [ ] **`GATE-04`**: Provide production Authenticode certificate to sign `--release` executables and NSIS installer (bundle currently flagged `UNSIGNED_RC1_REQUIRES_OPERATOR_CERT`).
