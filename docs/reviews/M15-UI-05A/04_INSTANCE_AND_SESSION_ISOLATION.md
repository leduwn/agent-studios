# M15 UI-05A — Report 04: Instance & Session Isolation Architecture

## Metadata
- **Milestone**: M15 UI-05A: Frontend Reliability, Shared State Consistency & Integration Readiness
- **Focus**: Multi-Tenant Task, Studio, and Runtime Instance Isolation
- **Target Worktree**: `D:\Projects\agent-studios-ide-ui\apps\agent-studios-vscode`
- **Target PR**: Draft PR #27 (`feat/m15-real-agent-execution-ui`)
- **Author**: Claude 2 (Frontend / VSCode Extension Integration Lead)
- **Date**: 2026-11-02

---

## 1. Executive Summary

Milestone M15 UI-05A enforces strict isolation invariants across multiple dimensions:
1. **Task Execution Isolation**: Active task state, streaming logs, and tool executions must not bleed between distinct task IDs.
2. **Studio Isolation**: Cross-studio contamination is prevented during rapid studio switching or concurrent panel operations.
3. **Runtime Instance Isolation**: Distinct runtime adapters (Claude Code vs OpenCode) are identified by composite instance keys (`${instance_id}|${implementation}`), preventing collision or accidental cross-talk.
4. **Lifecycle & View Destruction Safety**: Panel closures, webview reloads, and extension deactivate events must cleanly dispose of listeners, cancel in-flight queries, and invalidate out-of-order responses.

---

## 2. Invariant Specifications

### A. Task Execution Isolation (`TaskExecutionStore`)
- **Scoped Map Storage**: State for each task is isolated in `_tasks: Map<string, TaskExecutionState>`.
- **Active Task Switching**: Switching `activeTaskId` immediately stops emitting log streams for the prior task to the view, preventing interleaved or corrupted terminal outputs.
- **Generation Guarding**: Each task transition increments an internal revision counter `revision: number`. Events bearing an older sequence number or mismatched task ID are discarded fail-closed.

### B. Studio Context Boundaries (`SessionStateProjection`)
- **Studio ID Binding**: All IPC queries and event subscriptions carry explicit `studio_id`.
- **Mismatch Rejection**: If an event or artifact arrives with a `studio_id` differing from the active session studio, it is logged and rejected (`STUDIO_ID_MISMATCH`).
- **Clean Teardown on Studio Switch**: When an operator switches studios:
  1. Active task streams are unsubscribed.
  2. In-flight projection promises for the old studio are invalidated via generation epoch increment.
  3. Mission Control and Chat UI reset to empty/loading state before hydrating new studio state.

### C. Composite Runtime Instance Identifiers (`CapabilityMatrix`)
- Multiple instances of the same runtime implementation or multiple implementations on the same machine cannot collide.
- Key format:
  ```typescript
  export type CompositeInstanceId = `${string}|${string}`; // e.g. "inst-default-01|claude-code"
  ```
- Capability lookups and health statuses are strictly indexed by this composite key.

### D. Webview Boundary & Memory Isolation
- Webview panels run in sandboxed iframe contexts.
- No extension host references, cryptographic keys, or raw file descriptors are ever passed to webviews.
- All communication crosses `postMessage` with strongly-typed JSON message protocols.
- When a Webview panel is disposed (`onDidDispose`), all event listeners and polling intervals are cleared immediately.

---

## 3. Rapid Panel Destruction & Reload Invariants

| Action | Hazard Without Isolation | Guard Implemented in M15 UI-05A |
|---|---|---|
| **Rapid Studio Switch** | Stale studio query overwrites new studio | Epoch counter in `MissionControlProjection`; stale promise results discarded |
| **Panel Reload (F5 / DevTools)** | Duplicate event listeners created | `dispose()` cleans up old event listeners before re-registering |
| **Background Task Completion** | Completion event overwrites active task in another view | Events routed strictly by `task_id`; inactive task updates background counters only |
| **Webview Close during Stream** | Memory leak / detached promise rejection | `postMessage` wrapped in `if (this._panel && this._panel.visible)` safety guards |

---

## 4. Verification & Testing

- Unit tests in `src/test/unit/ChatMissionControlConsistency.test.ts` verify generation epoch rejection.
- Unit tests in `src/test/unit/OperatorConfirmationSafety.test.ts` verify instance isolation.
- E2E tests in `src/test/e2e/RealArtifactIpcE2E.test.ts` (Tests 10 & 11) verify `STUDIO_ID_MISMATCH` and `TASK_ID_MISMATCH` fail-closed rejection.
