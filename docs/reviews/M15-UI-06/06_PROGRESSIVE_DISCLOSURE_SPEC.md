# M15 UI-06 — Report 06: Progressive Disclosure & Conversation UX States

## Metadata
- **Milestone**: M15 UI-06: Agent-First Experience, Shared Session Continuity & Real Daemon Integration Acceptance
- **Worktree**: `D:\Projects\agent-studios-active\m15-ui-06`
- **Component**: `ChatHtmlBuilder.ts`, `ChatWebviewViewProvider.ts`
- **Date**: 2026-11-05
- **Status**: **VERIFIED & ACCEPTED**

---

## 1. The Three Tiers of Progressive Disclosure

To prevent information overload while preserving deep engineering visibility, Agent Studios implements a **Three-Tier Progressive Disclosure Model**:

```
Tier 1: Intent & Formulation (Idle View)
┌─────────────────────────────────────────────────────────────┐
│ Clean conversation thread or Welcome Surface with starters  │
│ Streaming console hidden (display: none)                    │
│ Clean composer input ready for goal formulation             │
└─────────────────────────────────────────────────────────────┘
                             │
                             ▼ Task Dispatched
Tier 2: Execution Oversight (Active Run)
┌─────────────────────────────────────────────────────────────┐
│ Compact Execution Progress Pill:                            │
│ [● Running] Task 2/4: "Compile TypeScript" (4.2s)           │
│ Streaming logs pane (expandable / collapsible)              │
│ Quick Link: [Mission Control →]                             │
└─────────────────────────────────────────────────────────────┘
                             │
                             ▼ Operator Inspection
Tier 3: Deep Fleet & DAG Inspection (Mission Control)
┌─────────────────────────────────────────────────────────────┐
│ Full interactive dependency graph (DAG)                     │
│ Multi-agent fleet allocations and agent status              │
│ Raw bidirectional IPC streaming logs and byte-level diffs   │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. Dynamic View State Transitions

In `ChatHtmlBuilder.ts`, client-side script handles execution updates reactively:

```javascript
function renderExecutionState(execState) {
  if (!execState) return;
  const executionStreamingPane = document.getElementById('executionStreamingPane');
  if (executionStreamingPane) {
    if (execState.status && execState.status !== 'idle') {
      executionStreamingPane.style.display = 'block';
    } else {
      executionStreamingPane.style.display = 'none';
    }
  }
}
```

### Invariants:
1. When idle, the streaming console never clutters the conversation container.
2. When a task starts running, the progress pill appears immediately with live elapsed time.
3. The Mission Control button (`data-action="open-mission-control"`) is always accessible from the streaming pane header for immediate deep inspection.

---

## 3. The 14 Conversation UX States

To ensure unambiguous state communication, 14 semantic status badge styles are defined in `ChatHtmlBuilder.ts`:

| # | UX State | Badge Class | Color Token | Semantic Meaning |
|---|---|---|---|---|
| 1 | **Connecting** | `.status-badge.status-connecting` | Amber / Warning | Performing HMAC handshake with native daemon. |
| 2 | **Connected** | `.status-badge.status-connected` | Emerald / Success | IPC transport ready and authenticated. |
| 3 | **Degraded** | `.status-badge.status-degraded` | Amber / Warning | IPC connected but subscription or runtime degraded. |
| 4 | **Offline** | `.status-badge.status-offline` | Muted Gray | Daemon not running or unreachable. |
| 5 | **Session Restored**| `.status-badge.status-session_restored`| Sky Blue | Session context hydrated from SQLite WAL. |
| 6 | **Task Pending** | `.status-badge.status-task_pending` | Slate / Neutral | Task queued in DAG awaiting dependency resolution. |
| 7 | **Awaiting Approval**| `.status-badge.status-awaiting_approval`| Orange / High-Risk | Operator confirmation challenge pending. |
| 8 | **Interrupted** | `.status-badge.status-interrupted` | Red / Error | Task cancelled by operator or timeout. |
| 9 | **Completed** | `.status-badge.status-completed` | Green / Success | All DAG tasks succeeded cleanly. |
| 10 | **Verified Artifact**| `.status-badge.status-verified_artifact`| Green / Success | Diff artifact cryptographically verified via SHA-256. |
| 11 | **Unverified Artifact**| `.status-badge.status-unverified_artifact`| Amber / Warning | Artifact diff unverified or missing hash. |
| 12 | **Idle** | `.status-badge.status-idle` | Subtle Gray | No active runs or background workers. |
| 13 | **Planning** | `.status-badge.status-planning` | Purple / Info | Agent formulating DAG plan or decomposing goal. |
| 14 | **Running** | `.status-badge.status-running` | Blue / Active | Agent executing task or tool call. |

All badge classes are covered by unit regression test `AgentIdeContinuity.test.ts` (Test 6).
