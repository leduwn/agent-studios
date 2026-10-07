# Agent Studios — Deterministic Control Plane Architecture

> **Status**: Core Architecture Specification
> **Precedence**: Subservient to `MASTER_VISION.md` and `PRODUCT_PRINCIPLES.md`.

---

## 1. Overview & Architectural Role

The Agent Studios Control Plane is the deterministic orchestration kernel responsible for managing multi-agent collaboration, task dependency execution graphs (DAGs), human-in-the-loop approvals, physical worktree allocation, artifact registries, tool budgets, and event-sourced state transitions.

```text
               Cognitive Coordinator
              (Probabilistic Planner)
                         │
                         ▼
             ControlPlaneActorHandle
                         │
                         ▼ mpsc channel
             ┌───────────────────────┐
             │   ControlPlaneActor   │
             │ (Single-Writer Loop)  │
             └───────────┬───────────┘
                         │
           ┌─────────────┴─────────────┐
           ▼                           ▼
   Deterministic State           Durable EventStore
  (TaskGraph, Agents, Runs)    (Monotonic Commit)
                                       │
                                       ▼ commit success
                                broadcast::Sender
                                       │
                                       ▼
                             Reactive Projections
                            (Agent Mode & IDE Mode)
```

---

## 2. The Single-Writer Actor Model (`ControlPlaneActor`)

To eliminate multi-threaded deadlocks, race conditions, and lock contention across parallel agents, the Control Plane executes as a **single-writer actor**:

1. **Dedicated Tokio Task**: The core `ControlPlane` state machine resides within an isolated actor loop.
2. **Handle-Based Concurrency**: External callers (supervisors, executors, UI RPC handlers) interact exclusively through the cloneable, thread-safe `ControlPlaneActorHandle`.
3. **Serialized State Mutations**: All commands (`create_task`, `transition_task_state`, `register_agent_batch`, etc.) are serialized over an unbounded `mpsc` queue and processed sequentially.
4. **Non-Blocking Reads**: Queries read consistent snapshots or query the actor asynchronously without holding long-lived mutexes.

---

## 3. UI as Projection: The Control Plane Owns Authoritative Truth

A foundational principle of Agent Studios is that **the UI is strictly a reactive presentation layer**:

- The UI (Agent Mode or IDE Mode) **never** owns orchestration state.
- State is **never** inferred by scraping terminal text, parsing raw stdout/stderr, or heuristics from chat messages.
- The UI renders read-model projections (`TaskGraphSnapshot`, `AgentSummary`, `RunSummary`, `WorktreeSnapshot`) constructed deterministically from the Control Plane event stream.
- If the desktop frontend crashes or reconnects, it queries the current snapshot and catches up on missed events gaplessly.

---

## 4. Durable Commit Before Broadcast Invariant

To guarantee data safety and prevent ephemeral state corruption, the Control Plane enforces the **commit-before-broadcast rule**:

$$\text{Validate State} \;\longrightarrow\; \text{Stage Event} \;\longrightarrow\; \textbf{Durable Commit to EventStore} \;\longrightarrow\; \text{Broadcast to Subscribers}$$

1. An incoming command is validated against domain state machine rules.
2. The domain mutation is applied in-memory and an `EventEnvelope` is staged.
3. The event is synchronously appended to the persistent `EventStore`.
4. **Only after the store confirms persistence** is the envelope published to the per-studio `broadcast::Sender<EventEnvelope>`.
5. If writing to disk/store fails, the in-memory mutation is rolled back, no event is broadcast, and an error is returned to the caller.

---

## 5. Event Families

Every state transition produces a typed, immutable `ControlPlaneEvent` categorized into distinct domain families:

| Event Family | Examples | Responsibilities |
| :--- | :--- | :--- |
| **Studio Lifecycle** | `StudioCreated`, `StudioConfigUpdated` | Studio boundaries and settings. |
| **Agent Lifecycle** | `AgentRegistered`, `AgentStateChanged`, `AgentSpawned` | Registration, worker hierarchy, active state. |
| **Task & DAG** | `TaskCreated`, `TaskDependencyAdded`, `TaskStateChanged` | Dependency graph, topological readiness, task status. |
| **Run & Execution** | `RunCreated`, `RunStateChanged`, `RunOutcomeRecorded` | Turn attempts, failure classifications, timeouts. |
| **Approvals** | `ApprovalRequested`, `ApprovalResolved` | Human-in-the-loop permission gating. |
| **Tool Execution** | `ToolStarted`, `ToolCompleted`, `ToolFailed` | Auditable tool call timing, tool names, safe error codes. |
| **Budgets** | `BudgetUsageUpdated`, `BudgetExceeded` | Turn counts, tool counts, wall-clock tracking. |
| **Workspace & Worktrees** | `WorktreeAllocated`, `WorktreeReleased`, `WorktreeRetained` | Physical directory assignments and retention status. |
| **Artifact Registry** | `ArtifactRegistered`, `ArtifactVersionAllocated`, `PatchReconciled` | SHA-256 patch diffs, deliverables, reconciliation. |

---

## 6. Authoritative State Machines: Blocked is NOT Cancelled

### Task State Machine
```text
                  ┌─────────┐
                  │ Pending │
                  └──┬───┬──┘
        deps exist   │   │ no deps
        unfulfilled  │   │
                     ▼   ▼
               ┌─────────┐   ┌───────┐
               │ Blocked ├──►│ Ready │
               └────┬────┘   └───┬───┘
                    │            │ execution starts
                    │            ▼
                    │      ┌─────────┐
                    │      │ Running │◄──────┐
                    │      └──┬─┬──┬─┘       │ resume
                    │         │ │  │ pause   │
                    │         │ │  └─────►┌──┴───┐
                    │         │ │         │Paused│
                    │         │ │         └──┬───┘
                    │         │ │            │
                    │         ▼ ▼            │
                    │      ┌───────────┐     │
                    │      │ Succeeded │     │
                    │      │  Failed   │◄────┤
                    │      └───────────┘     │
                    │                        │
                    ▼                        ▼
               ┌───────────────────────────────┐
               │           Cancelled           │
               └───────────────────────────────┘
```

### The Invariant: `Blocked != Cancelled`
- **`Blocked`**: A task is waiting for an external condition to become true (upstream task completion, human approval resolution, patch conflict resolution, or workspace lease release). Once unblocked, the task transitions to `Ready` and proceeds normally.
- **`Cancelled`**: A permanent, terminal revocation of intent by the user or supervisor. Once cancelled, a task cannot be resumed.
- **Rule**: Never mark a blocked task as cancelled for UI convenience or error simplification.

### Agent State Machine
$$\text{Registered} \longrightarrow \text{Starting} \longrightarrow \text{Idle} \longleftrightarrow \text{Busy} \longrightarrow \text{Stopping} \longrightarrow \text{Stopped}$$
Operational status (`Running`, `Waiting`, `Blocked`, `Retrying`, `Paused`, `Failed`, `Cancelled`) tracks fine-grained execution health without conflating it with task states.
