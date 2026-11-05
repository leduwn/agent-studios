# Agent Studios — M15 UI-06 Release Hardening
## Document 03: Frontend Regression Matrix & Verification Evidence

**Milestone:** M15 UI-06 (Agent-First Experience, Shared Session Continuity & Real Daemon Integration)  
**Branch:** `feat/m15-ui-06-agent-ide-continuity`  
**Target Base:** `integration/m15-unified-codebase`  
**Baseline Test Count:** 755 tests  
**Current Test Count:** 788 tests (679 unit + 84 real daemon E2E + 25 extension host integration)  
**Status:** 100% PASSING  
**Date:** 2026-11-06  

---

### 1. Verification Summary

Every frontend verification gate has been executed in the active worktree (`D:\Projects\agent-studios-active\m15-ui-06`):

| Gate / Command | Target / Scope | Result | Details |
| :--- | :--- | :--- | :--- |
| `npm run compile` | TypeScript compiler (`tsc -p ./`) | **PASS (0 errors)** | Clean transpilation to `out/` |
| `npm run typecheck` | Strict typechecker (`tsc --noEmit`) | **PASS (0 errors)** | Zero type violations |
| `npm run lint` | ESLint TypeScript ruleset | **PASS (0 errors)** | 0 errors, 336 naming warnings |
| `npm run test:unit` | Unit test suite (Mocha) | **PASS (679/679)** | In-memory mocks, bridge, state projection |
| `npm run test:e2e` | Real compiled daemon suite (agsd) | **PASS (84/84)** | Real PID, Named Pipe / UDS, SQLite WAL |
| `npm run test:integration` | Real VS Code Host (@vscode/test) | **PASS (25/25)** | Activation, command registry, mode continuity |
| **Total Automated** | **Full Frontend Verification Suite** | **788 / 788 (100%)**| Zero test failures |

---

### 2. Regression Protection Matrix by Feature Area

#### Area 1: Agent-First Welcome Surface & Progressive Disclosure
- **Welcome Surface:** Displays active goals, session timeline, and agent status prior to code explorer.
- **Progressive Disclosure:** Status header, task progress breakdown, pending approval banners, provider instance indicator.
- **Verification:**
  - `ChatHtmlBuilder.test.ts` (DOM structure, telemetry markers).
  - `ChatWebviewViewProvider.test.ts` (lifecycle, state propagation).
  - `activation.test.ts` (sidebar view registration).

#### Area 2: 1-Click Restore & Session Continuity
- **Command:** `agent-studios.continueSession` (restores latest or specified session).
- **Invariants Verified:**
  - Preserves `session_id` and active workspace across mode transitions.
  - Zero daemon process restarts (`daemonPid` remains constant).
  - Zero task re-submissions (prevents duplicate execution on reconnect).
  - State restored from persistent SQLite WAL across bridge reconnection.
- **Verification:**
  - `AgentIdeContinuity.test.ts` (unit invariants).
  - `RealAgentIdeContinuityE2E.test.ts` (live daemon process, SQLite WAL).
  - `modeContinuity.test.ts` (unmocked extension host dispatch).

#### Area 3: Mode Transitions & Navigation
- **Commands:**
  - `agent-studios.openInIde` (switches to IDE workbench explorer and active editor).
  - `agent-studios.switchToAgentMode` (focuses Agent Studios chat interface).
- **Verification:**
  - Verified command existence in `package.json` contributes and real `vscode.commands` catalog.
  - Unit mocks verify command dispatch targeting `workbench.view.explorer` and `agent-studios.chat.focus`.
  - Extension host integration tests execute commands without throwing unhandled rejections.

#### Area 4: Shared Session State Projection
- **Component:** `SessionStateProjection.ts`
- **Guarantees:**
  - Single source of truth for webviews, status bars, and tree views.
  - Fail-closed initial state when bridge is disconnected.
  - Hydration from live daemon events (`studio_created`, `task_updated`, `approval_requested`).
- **Verification:**
  - `SessionStateProjection.test.ts`
  - `BridgeManager.test.ts`
  - `LiveMissionControlE2E.test.ts`

#### Area 5: Real Daemon Provenance & Crash Recovery
- **Component:** `AgsdBinaryResolver.ts`
- **Guarantees:**
  - Strictly rejects obsolete historical worktrees (`m15-04`, `m15-03`, `m15-02`, `m15`).
  - Requires checked-out workspace binary under CI.
  - Provides actionable `cargo build` remediation instructions.
- **Verification:**
  - `AgsdBinaryResolver.test.ts` (7 unit tests).
  - `RealAgentIdeContinuityE2E.test.ts` (6 real process E2E tests).
  - `CrashRecoveryE2E.test.ts` (SIGKILL recovery, SQLite WAL replay backfill).
