# Agent Studios: Orchestration Observability & Read Models

This document details the read models, projection architecture, timeline event sourcing, and
deterministic indexing for tasks, worktrees, artifacts, and agents in Agent Studios (Milestone M09).

---

## 1. Overview & Architecture

Agent Studios uses an event-sourced control plane where state mutations are recorded as durable
`ControlPlaneEvent` envelopes with monotonically increasing sequence numbers. The orchestration
layer builds specialized, point-in-time query projections and read models for developer dashboards,
CLI monitoring, and automated supervisor policy decisions.

```text
┌────────────────────────────────────────────────────────────────────────┐
│                        ControlPlane Event Stream                       │
│    (Monotonically ordered SequenceTracker, Per-Studio Isolation)       │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
          ┌─────────────────────────┼─────────────────────────┐
          │                         │                         │
          ▼                         ▼                         ▼
┌──────────────────┐      ┌──────────────────┐      ┌──────────────────┐
│ TaskGraphSnapshot│      │ WorktreeSnapshot │      │  ArtifactIndex   │
│  - Task DAG      │      │  - Active/Retain │      │  - Blob Metadata │
│  - TaskWorktrees │      │  - Thread Binding│      │  - By Kind/Task/ │
│  - TaskArtifacts │      │  - By Task Map   │      │    Worktree/Agent│
│  - TaskReconciles│      │                  │      │  - Total Bytes   │
└──────────────────┘      └──────────────────┘      └──────────────────┘
          │                         │                         │
          └─────────────────────────┼─────────────────────────┘
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                         TaskTimelineProjection                         │
│                                                                        │
│   Unified chronological timeline of:                                   │
│   - Task lifecycle & retries                                           │
│   - Worktree allocation, thread binding, release & retention           │
│   - Ephemeral change capture & artifact registration                   │
│   - Safe patch reconciliation checks, conflicts & merges               │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Worktree & Artifact Read Models

### `WorktreeSnapshot`
Captures point-in-time status of all workspaces managed by `WorkspaceOrchestrator` within a Studio:
- `studio_id`: Owning Studio identifier.
- `worktrees`: Full list of `WorktreeRecord`s.
- `active_worktrees`: Worktrees in active lifecycle states (`Ready`, `InUse`, `ChangeCaptured`, `ReconcilePending`).
- `retained_worktrees`: Worktrees preserved on disk due to task failures, cancellations, or dirty state.
- `by_task`: Lookup map `HashMap<TaskId, WorktreeId>`.
- `by_thread`: Lookup map `HashMap<String, WorktreeId>` mapping Codex `ThreadId` to worktree.

### `ArtifactIndex`
Content-addressed index tracking all artifacts across tasks and workspaces:
- `studio_id`: Owning Studio identifier.
- `artifacts`: List of `ArtifactRecord` entries.
- `by_kind`: Index `HashMap<ArtifactKind, Vec<ArtifactId>>` (`File`, `Patch`, `Log`, `Report`, `Plan`, `Other`).
- `by_task`: Index `HashMap<TaskId, Vec<ArtifactId>>`.
- `by_worktree`: Index `HashMap<WorktreeId, Vec<ArtifactId>>`.
- `by_agent`: Index `HashMap<AgentId, Vec<ArtifactId>>`.
- `total_bytes`: Aggregate byte size of all stored blobs.

---

## 3. Enriched Task Graph Projection (`TaskGraphSnapshot`)

Extends the dependency DAG with correlated workspace and output metadata:
- `studio_id`: Owning Studio identifier.
- `tasks`: Full list of `TaskRecord` items.
- State lists: `ready_tasks`, `running_tasks`, `blocked_tasks`, `retrying_tasks`, `succeeded_tasks`, `failed_tasks`, `cancelled_tasks`.
- `adjacency`: Directed dependency map `HashMap<TaskId, Vec<TaskId>>`.
- `task_worktrees`: Assigned worktree per task `HashMap<TaskId, WorktreeId>`.
- `task_artifacts`: Generated artifacts per task `HashMap<TaskId, Vec<ArtifactId>>`.
- `task_reconciliations`: Associated reconciliations per task `HashMap<TaskId, Vec<ReconciliationId>>`.

---

## 4. Chronological Task Timeline (`TaskTimelineProjection`)

Reconstructs the full lifecycle narrative of a single task from historical events:

```rust
pub struct TaskTimelineProjection {
    pub task_id: TaskId,
    pub studio_id: StudioId,
    pub current_state: TaskState,
    pub assigned_agent_id: Option<AgentId>,
    pub assigned_worktree_id: Option<WorktreeId>,
    pub bound_thread_id: Option<String>,
    pub artifact_ids: Vec<ArtifactId>,
    pub reconciliation_ids: Vec<ReconciliationId>,
    pub items: Vec<TimelineItem>,
}
```

### Timeline Item Kinds (`TimelineItemKind`)
1. `TaskCreated`: Task registered in dependency graph.
2. `TaskStateChanged`: Transition across `Pending`, `Ready`, `Running`, `Succeeded`, `Failed`, etc.
3. `TaskRetryScheduled`: Task failed with remaining retry budget.
4. `RunCreated` / `RunStateChanged`: Execution attempt initialized or updated.
5. `WorktreeAssigned`: Managed worktree allocated for task.
6. `WorktreeThreadBound`: Worker `ThreadId` bound to worktree.
7. `WorktreeChangeCaptured`: Ephemeral Git index diff captured.
8. `WorktreeReleased`: Worktree removed or retained.
9. `ArtifactRegistered`: Content-addressed artifact blob persisted.
10. `ReconciliationCreated`: Integration worktree patch operation queued.
11. `ReconciliationStateChanged`: Progression through `Checking` or `Applying`.
12. `ReconciliationConflictDetected`: Dry-run `git apply --check` failed; conflict recorded.
13. `ReconciliationApplied`: Patch cleanly applied and committed to integration branch.

---

## 5. Event Ordering & Gapless Replay Guarantees

All projections enforce strict replay and subscription invariants via `SequenceTracker`:
- **Monotonic Sequences**: Sequence numbers are strictly strictly increasing with zero duplicate or skipped events.
- **Gap Detection**: If an event arrives with `sequence > expected`, `SequenceTracker` returns `SequenceError::GapDetected` and triggers caught-up stream replay.
- **Fail-Closed Validation**: Projections reject invalid transitions or corrupted studio ownership.
