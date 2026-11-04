# M15 UI-05A — Report 02: Shared State Consistency Architecture

## Metadata
- **Milestone**: M15 UI-05A: Frontend Reliability, Shared State Consistency & Integration Readiness
- **Focus**: Chat & Mission Control Synchronization, Race Elimination, In-Flight Coalescing
- **Target Worktree**: `D:\Projects\agent-studios-ide-ui\apps\agent-studios-vscode`
- **Target PR**: Draft PR #27 (`feat/m15-real-agent-execution-ui`)
- **Author**: Claude 2 (Frontend / VSCode Extension Integration Lead)
- **Date**: 2026-11-02

---

## 1. Problem Statement & Divergence Hazards

In previous milestones, the VS Code extension maintained state across two primary webviews:
1. **Chat Webview** (`ChatWebviewViewProvider`): Interacts with active task execution, streaming tool calls, and operator confirmation dialogs.
2. **Mission Control Webview** (`MissionControlViewProvider`): Displays studio overview, task history, pending approvals, provider metrics, and diagnostic health.

Without coordinated synchronization:
- **Unbounded Refresh Storms**: A burst of timeline events (e.g. rapid tool execution logs) triggered simultaneous `projection.refresh()` queries, firing dozens of concurrent IPC roundtrips across Named Pipes / UDS.
- **Out-of-Order Overwrite Race**: If a slow IPC query for an old task resolved after a rapid user studio switch or a newer timeline event arrived, the stale snapshot overwrote the authoritative live state.
- **State Divergence**: Chat view displaying a task as `failed` (e.g. daemon restart) while Mission Control still showed `running` due to lagging poll intervals.

---

## 2. Hardened Architecture & Synchronization Invariants

```
┌──────────────────────────────────────────────────────────┐
│              Authoritative Rust Daemon (agsd)            │
└──────────────────────────┬───────────────────────────────┘
                           │ IPC Streaming Events & Queries
                           ▼
              ┌─────────────────────────┐
              │ NativeIpcTransport.ts   │
              └────────────┬────────────┘
                           │ Monotonic Sequence Stream
                           ▼
             ┌───────────────────────────┐
             │ DaemonConnectedBridge.ts  │
             └──────┬─────────────┬──────┘
                    │             │
      Domain Events │             │ Queries (Coalesced)
                    ▼             ▼
┌─────────────────────────┐   ┌───────────────────────────────┐
│ TaskExecutionStore.ts   │◄──┤ MissionControlProjection.ts   │
│ (Active Task & Stream)  │   │ (Studio & Task Aggregations)  │
└───────────┬─────────────┘   └───────────────┬───────────────┘
            │                                 │
            ▼                                 ▼
┌─────────────────────────┐   ┌───────────────────────────────┐
│   Chat Webview View     │   │    Mission Control Webview    │
└─────────────────────────┘   └───────────────────────────────┘
```

### A. In-Flight Query Coalescing & Debouncing
In `MissionControlProjection.ts`:
- **Debouncing**: `refresh()` calls within a 50ms window are collapsed into a single execution.
- **In-Flight Reuse**: If an authoritative refresh query batch is already executing over the wire, subsequent callers receive the pending `Promise<void>`, guaranteeing that at most one query batch runs simultaneously.
- **Zero Query Duplication**: Prevents IPC channel saturation during rapid task execution bursts.

### B. Epoch / Generation Guarding
- Each `refresh()` cycle captures an incremented generation integer: `const currentGen = ++this._generation;`.
- Upon resolution of the 6 parallel queries (`getCurrentSession`, `listTasks`, `listPendingApprovals`, `listArtifacts`, `listProviderSummaries`, `getHealth`), the result is committed **if and only if** `this._generation === currentGen`.
- Any response arriving after a newer refresh cycle or after a studio switch is cleanly discarded.

### C. Cross-Store State Reconciliation
- `TaskExecutionStore` remains the single source of truth for active task lifecycles.
- When `task_started` or `task_state_transitioned` fires:
  1. `TaskExecutionStore` authoritatively records the state transition.
  2. `MissionControlProjection` updates the in-memory task entry and overview counters immediately, without waiting for the next polling interval.
  3. Stale sequence numbers are rejected fail-closed: `if (event.sequence <= task.lastSequence) return;`.

---

## 3. Verification & Regressions Handled

- **Test Suite**: `src/test/unit/ChatMissionControlConsistency.test.ts`.
- **Assertions Verified**:
  - 10 concurrent `projection.refresh()` calls trigger exactly 1 IPC query batch.
  - Superseded queries arriving out-of-order do not overwrite updated state.
  - Rapid task status transitions (`running` -> `failed`) reflect identically in both Chat and Mission Control views.
