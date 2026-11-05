# M15 UI-06 — Report 01: Baseline & Subagent File Ownership Audit

## Metadata
- **Milestone**: M15 UI-06: Agent-First Experience, Shared Session Continuity & Real Daemon Integration Acceptance
- **Repository**: `D:\Projects\agent-studios`
- **Worktree**: `D:\Projects\agent-studios-active\m15-ui-06`
- **Branch**: `feat/m15-ui-06-agent-ide-continuity`
- **Canonical Baseline Commit**: `ec2058256a0001f869b968f4d787b7378e413eb8`
- **Role Assignment**: Claude 2 (Principal Frontend Engineer & Extension Architect)
- **Concurrent Work**: Claude 1 developing Backend M15.1-A (Provider Discovery & AGS CLI)
- **Target Backend Binary**: Native Rust `agsd.exe` (`agent-studios-rs/target/debug/agsd.exe`)
- **Date**: 2026-11-05
- **Status**: **VERIFIED & ACCEPTED**

---

## 1. Executive Summary & Audit Scope

Milestone M15 UI-06 establishes the Agent-First workspace paradigm and bidirectional mode continuity between Agent Mode (`agent-studios.chat`) and IDE Mode (Code-OSS editor workbench) in Agent Studios.

### Strict Role & File Boundary Invariants
1. **Claude 2 Ownership**:
   - `apps/agent-studios-vscode/src/**` (Projections, Bridge, Webviews, Commands, Chat).
   - Frontend unit test suites (`src/test/unit/**`).
   - Frontend and real daemon E2E test suites (`src/test/e2e/**`).
   - Extension metadata and package definitions (`package.json`).
   - Documentation and verification deliverables under `docs/reviews/M15-UI-06/**`.
2. **Claude 1 Ownership (Strictly Read-Only to Claude 2)**:
   - `agent-studios-rs/**` (Native Rust daemon, provider discovery, CLI/TUI).
   - `codex-rs/**` (Shared runtime and agent kernels).
   - `Cargo.toml` and Cargo lockfiles.
   - Cross-language shared IPC schema contracts (`crates/`).
   - Claude 1's feature branches and worktrees.
3. **Safety & Security Invariants**:
   - Zero billable commercial model calls (Anthropic/OpenAI) during test execution.
   - Production `task.submit` gated closed with operator confirmation challenge.
   - No force-pushing, no branch resets, no synthetic or fabricated test fixtures.

---

## 2. Baseline & Verification Metrics

| Verification Phase | Metric / Command | Target Threshold | Actual Result | Verdict |
|---|---|---|---|---|
| **TypeScript Compilation** | `npm run compile` | 0 errors | 0 errors | **PASSED** |
| **Type Checking** | `npm run typecheck` | 0 errors | 0 errors | **PASSED** |
| **ESLint Analysis** | `npm run lint` | 0 errors | 0 errors, 329 warnings | **PASSED** |
| **Unit Test Suite** | `npm run test:unit` | >= 657 passing | **671 passing** | **PASSED** |
| **Real Daemon E2E Suite** | `npm run test:e2e` | >= 78 passing | **84 passing** | **PASSED** |
| **Combined Tests** | `test:unit` + `test:e2e` | >= 735 passing | **755 passing (100%)** | **PASSED** |
| **VSIX Packaging** | `npm run package` | Clean VSIX artifact | `agent-studios-vscode-0.1.0.vsix` | **PASSED** |

---

## 3. Subsystem Audit Matrix

### 3.1 Projections & State (`src/state/`)
- `SessionStateProjection.ts`: Authoritative single source of truth for active session (`session_id`), cwd, task graph, approvals, and artifacts. Hydrates asynchronously from `BridgeManager.initializeFromBridge()` without creating ghost sessions.
- `TaskExecutionStore.ts`: Local client-side in-flight task execution store. Enforces immutability and zero-mutation during read-only inspection.

### 3.2 Bridge & Transport (`src/bridge/`)
- `DaemonConnectedBridge.ts`: Manages live IPC connection over Named Pipes (`\\.\pipe\...`) on Windows and Unix Domain Sockets on Linux/macOS. Features two-phase HMAC-SHA256 handshake, monotonic watermark resync, and SQLite WAL recovery.
- `NativeIpcTransport.ts`: Framed JSON-RPC message framing with length-prefix framing and ping/pong heartbeat.
- `DaemonDiscovery.ts`: Discovers and parses `agsd.json` and `agsd-admin.json` credentials, enforcing role-based credential separation.

### 3.3 Commands & Navigation (`src/commands/`)
- `CommandsRegistry.ts`:
  - `agent-studios.openInIde`: Reveals Explorer view (`workbench.view.explorer`) and focuses active editor group.
  - `agent-studios.switchToAgentMode`: Reveals activity bar (`workbench.view.extension.agent-studios-activitybar`) and focuses Chat.
  - `agent-studios.continueSession`: Switches active session in projection with zero daemon restarts and zero task re-executions.

### 3.4 Webview Surface & Security (`src/chat/`)
- `ChatHtmlBuilder.ts`: Renders neutral/monochrome Agent-First welcome surface, goal starters, workspace status, honest model catalog notice ("Dynamic discovery pending M15.1"), and 14 conversation UX state badge classes.
- `ChatSecurityGuards.ts`: Comprehensive message validation guarding against prototype pollution, oversized payloads, unknown commands, and untrusted workspaces.
- `ChatWebviewViewProvider.ts`: Webview lifecycle host coordinating bidirectionally with `BridgeManager` and `SessionStateProjection`.
