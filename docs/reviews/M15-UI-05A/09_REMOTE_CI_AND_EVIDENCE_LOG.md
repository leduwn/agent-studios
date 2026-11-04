# M15 UI-05A — Report 09: Remote CI & Verification Evidence Log

## Metadata
- **Milestone**: M15 UI-05A: Frontend Reliability, Shared State Consistency & Integration Readiness
- **Focus**: Multi-Platform CI Runs, Remote Verification Parity & Packaging Artifacts
- **Target Worktree**: `D:\Projects\agent-studios-ide-ui\apps\agent-studios-vscode`
- **Target PR**: Draft PR #27 (`feat/m15-real-agent-execution-ui`)
- **Author**: Claude 2 (Frontend / VSCode Extension Integration Lead)
- **Date**: 2026-11-02

---

## 1. Remote CI Baseline & Historic Runs

The Agent Studios VS Code Extension CI is verified across both Linux (`ubuntu-latest`) and Windows (`windows-latest`) in workflow `.github/workflows/agent-studios-m14-ui-ci.yml`.

### Historic CI Verification Record

| Run ID | Commit SHA | Trigger Reason | Ubuntu Job | Windows Job | Verdict |
|---|---|---|---|---|---|
| `41381953764` | `0ae79a6b55` | Initial M15-04 release | Failed (stale pin & CLI check) | Failed (stale pin & CLI check) | **Remediated** |
| `41383213765` | `1a081f3717` | Backend pin update to `dcbcb39b` & fail-soft CLI | Job `125078295909`: **SUCCESS** | Job `125078295548`: **SUCCESS** | **100% GREEN** |
| `41392216006` | `854eab9a5c` | Replay sequence gap recovery & E2E taxonomy | Job `125105871471`: **SUCCESS** | Job `125105871602`: **SUCCESS** | **100% GREEN** |

---

## 2. Platform Parity Architecture

The test matrix enforces strict cross-platform compatibility:

### A. Linux (`ubuntu-latest`, POSIX Kernel 6.8)
- **IPC Mechanism**: Unix Domain Sockets (UDS) located in `/tmp/ags-test-.../agsd.sock`.
- **Permission Boundary**: Strict `0700` (`rwx------`) directory permissions enforced by `crates/ipc-transport/src/unix/permissions.rs`. The test harness explicitly sets `chmod(stateDir, 0o700)` prior to daemon spawn.
- **Signal Handling**: POSIX `SIGKILL` (`kill -9`) tested for abnormal termination and SQLite WAL recovery.

### B. Windows (`windows-latest`, Windows Server 2022 / Windows 11)
- **IPC Mechanism**: Windows Named Pipes (`\\.\pipe\ags-test-...`).
- **Security Boundary**: DACL security descriptors assigned via Win32 API.
- **Process Termination**: Win32 `TerminateProcess` verified for clean SQLite recovery without database locking errors.
- **Transient Connection**: Bounded retry loop handles pipe initialization latency.

---

## 3. Remote CI Step Pipeline

Every remote CI execution enforces the following 16-step pipeline:
1. `Checkout repository`
2. `Checkout pinned Rust backend (M15 IPC agsd)` at SHA `dcbcb39bf61225a4660be5b533cf1ffd5de14b29`
3. `Install Rust toolchain` (`dtolnay/rust-toolchain@stable`)
4. `Cache cargo dependencies`
5. `Build agsd native daemon` (`cargo build --bin agsd -p agent-studios-ipc-transport`)
6. `Setup Node.js 22`
7. `Install dependencies` (`npm ci`)
8. `Lint source files` (`npm run lint` — must maintain 0 errors, <= 324 warnings)
9. `Typecheck TypeScript` (`npm run typecheck` — 0 errors)
10. `Build extension` (`npm run build` — 0 errors)
11. `Run pure unit test suite` (`npm run test:unit`)
12. `Run real daemon E2E tests (Linux)` (Ubuntu runner)
13. `Run real daemon E2E tests (Windows)` (Windows runner)
14. `Run VS Code extension host integration tests`
15. `Validate VSIX packaging` (`npx @vscode/vsce package --no-dependencies`)
16. `Upload VSIX artifact`

---

## 4. Verification Evidence & Quality Target for M15 UI-05A

Upon completing code hardening and test suite expansions:
1. Local suites (`compile`, `lint`, `test:unit`, `test:e2e`) will be verified 100% green.
2. Changes will be committed to `feat/m15-real-agent-execution-ui`.
3. Pushed to GitHub `origin`.
4. Remote CI run will be monitored to conclusion across Ubuntu and Windows runners.
5. Exact run ID and job IDs will be appended to this log.
