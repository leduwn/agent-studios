# M15 UI-02.2 — Cross-Platform CI & Quality Gate Verification

## Metadata
- **Milestone**: M15 UI-02.2
- **Topic**: Cross-Platform CI (Ubuntu & Windows), Quality Gates, Test Baselines
- **Target Branch**: `feat/m15-real-agent-execution-ui` (Draft PR #27)
- **Base Branch**: `feat/m14-mission-control-connected-integration`
- **Pinned Backend Commit**: `61cb7e88e4ae0e3a0432134b77eee0c30f56c302` (PR #28 / PR #26)
- **Date**: 2026-11-02

---

## 1. Quality Gates & Verified Baseline

All quality gates enforced in M15 UI-02.2 adhere to the following metrics:

| Metric | Target / Baseline | Result |
| :--- | :--- | :--- |
| **TypeScript Compilation** | `npm run typecheck` (0 errors) | PASSED |
| **ESLint Warnings** | <= 338 warnings (0 errors) | PASSED (338 warnings) |
| **Unit Tests** | >= 534 tests | PASSED |
| **Mock Daemon E2E Tests** | 11 tests | PASSED |
| **Real Daemon IPC E2E Tests** | 20 tests | PASSED |
| **Extension Host Tests** | 18 tests | PASSED |
| **VSIX Packaging** | Clean build via `@vscode/vsce` | PASSED |

---

## 2. Cross-Platform Transport Invariants

### 2.1 Named Pipes (Windows)
- Pipe format: `\\.\pipe\ags-<uuid>`.
- Connection retry logic handles startup latency between bootstrap metadata emission and IPC listener readiness.
- Security descriptors ensure current-user-only access.

### 2.2 Unix Domain Sockets (Ubuntu / macOS)
- Socket format: `/tmp/ags-<uuid>.sock`.
- Filesystem permissions (0600) restrict access to the launching user.
- Socket cleanup handles graceful exit and SIGTERM.

### 2.3 Path Handling & Normalization
- Path comparison uses case-insensitive normalization on Windows (`win32`) and case-sensitive on POSIX (`linux`, `darwin`).
- Forward slashes are canonicalized for cross-platform IPC framing.
- Null bytes, UNC network paths, and traversal attacks (`..`) are rejected across all platforms.
