# Agent Studios — Control Plane Architecture

## 1. Overview & Architectural Goals

The Agent Studios Control Plane is the deterministic orchestration kernel responsible for managing multi-agent collaboration, task dependency execution graphs (DAGs), human-in-the-loop approvals, artifact registries, and event-sourced state transitions.

### Key Tenets
1. **Strict Upstream Isolation**: Resides entirely within `agent-studios-rs/`, leaving upstream Codex runtime crates (`codex-rs/`) clean and unaffected.
2. **Deterministic State Machine**: Every domain state modification is recorded as an immutable, sequentially numbered `EventEnvelope` per Studio.
3. **Event-Sourced Replay**: Any state can be completely and identically reconstructed from a sequential log of domain events without side effects or duplicate event emission.
4. **Graph-Enforced Safety**: Task dependencies are validated against directed cycle introduction using iterative `O(V + E)` algorithms.
5. **Hierarchical Cancellation**: Explicit cascading cancellation guarantees consistent termination across Studios, Tasks (and transitive descendants), Agents, and Runs.

---

## 2. Workspace Structure

```text
agent-studios-rs/
├── Cargo.toml
├── crates/
│   ├── protocol/          # Pure data structures, strong IDs, state enums, transitions, and events
│   │   └── src/
│   │       ├── id.rs           # Strongly typed UUID v4 newtypes (StudioId, TaskId, etc.)
│   │       ├── studio.rs       # Studio domain model
│   │       ├── agent.rs        # AgentDescriptor, AgentKind, AgentState
│   │       ├── task.rs         # TaskRecord, TaskState transition machine
│   │       ├── run.rs          # RunRecord, RunState transition machine
│   │       ├── approval.rs     # ApprovalRequest, ApprovalKind, ApprovalState machine
│   │       ├── artifact.rs     # ArtifactRecord, ArtifactKind metadata
│   │       ├── cancellation.rs # CancellationScope, CancellationSummary
│   │       ├── event.rs        # ControlPlaneEvent enum and EventEnvelope
│   │       └── error.rs        # TransitionError, IdParseError
│   └── control-plane/     # State engine, TaskGraph DAG, store abstraction, clock, and replay
│       └── src/
│           ├── clock.rs        # Clock trait, SystemClock, and deterministic FixedClock
│           ├── task_graph.rs   # TaskGraph with iterative cycle detection & readiness logic
│           ├── store.rs        # EventStore trait and in-memory monotonic store
│           ├── engine.rs       # ControlPlane coordinator & event-sourcing replay engine
│           └── error.rs        # ControlPlaneError, TaskGraphError, StoreError, ReplayError
```

---

## 3. Strongly Typed Identifiers

All domain entities use strongly typed identifier newtypes wrapping UUID v4 rather than raw primitive strings or UUIDs. This prevents accidental identifier substitution at compile time (e.g. passing an `AgentId` where a `TaskId` is expected).

Each ID type implements:
- `Display` (standard hypenated UUID format)
- `FromStr` (validated parsing with typed `IdParseError`)
- `Serialize` and `Deserialize` (transparent serde mapping)
- Uniform `new()`, `from_uuid()`, and `into_inner()` constructors.

Types: `StudioId`, `AgentId`, `TaskId`, `RunId`, `ApprovalId`, `ArtifactId`, `EventId`.

---

## 4. State Machines & Transition Validation

### TaskState
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
- **Terminal States**: `Succeeded`, `Failed`, `Cancelled`. Terminal states reject any further transitions.
- **Readiness Invariant**: A task can only transition from `Blocked` to `Ready` when every prerequisite dependency task is in `TaskState::Succeeded`.

### RunState
```text
┌────────┐      ┌──────────┐      ┌─────────┐      ┌──────────┐
│ Queued ├─────►│ Starting ├─────►│ Running ├─────►│Succeeded │
└───┬────┘      └────┬─────┘      └──┬───┬──┘      │  Failed  │
    │                │               │   │         └──────────┘
    │                │               │   ▼
    │                │               │ ┌────────┐
    │                │               │ │ Paused │
    │                │               │ └───┬────┘
    ▼                ▼               ▼     ▼
  ┌──────────────────────────────────────────┐
  │                Cancelled                 │
  └──────────────────────────────────────────┘
```

### ApprovalState
- `Pending` transitions exclusively to one terminal resolution: `Approved`, `Denied`, `Cancelled`, or `Expired`.
- Double-resolution attempts fail with `TransitionError::ApprovalAlreadyResolved`.

---

## 5. TaskGraph & DAG Engine

The `TaskGraph` manages dependencies between tasks within a Studio.

### Cycle Detection Algorithm
- Cycles are prevented **before insertion**.
- When `add_dependency(task_id, dependency_id)` is invoked, the graph checks whether `dependency_id` can already reach `task_id` following prerequisite edges (`reaches_iterative`).
- If a path exists, adding the dependency would close a directed cycle, and the operation is rejected with `TaskGraphError::DependencyCycle`.
- Uses an **iterative BFS with an explicit queue and visited set** to prevent recursion and eliminate stack overflow risks on large graphs.

### Global Topological Validation
- `validate()` executes Kahn's algorithm over all graph vertices.
- Ensures all in-degrees match declared dependencies and verifies that the complete graph forms a single valid topological sort without cycles or orphaned references.

### Dependent-Safe Task Removal
- A task cannot be removed from the graph if other active tasks declare it as a prerequisite dependency.
- Violations produce `TaskGraphError::UnsafeTaskRemoval` indicating the count of dependent tasks blocking removal.

---

## 6. Invariant Rules

The `ControlPlane` strictly enforces the following domain invariants on every mutating call:
1. **Studio Boundary**:
   - Tasks, Agents, Runs, Approvals, and Artifacts must belong to an existing Studio.
   - Tasks can only depend on tasks belonging to the exact same `StudioId`. Cross-studio dependencies return `ControlPlaneError::StudioMismatch`.
   - Runs can only be assigned to an Agent belonging to the same Studio as the target Task.
   - Approvals and Artifacts must reference entities within the same Studio boundary.
2. **Single Parent Task**: A task optionally references a single `parent_task_id`, which must exist in the same Studio.
3. **Monotonic Event Sequencing**: Each Studio has an independent sequence counter starting at `1`. Every newly emitted event must have `sequence == latest_sequence + 1`. Sequence gaps and regressions are rejected by the store.
4. **Immutability of Terminal Entities**: Terminal tasks, runs, and approvals cannot be re-opened or transitioned to non-terminal states.

---

## 7. Hierarchical Cancellation Semantics

Cancellation is triggered through `request_cancellation(scope, reason)`.

### Scopes
- `CancellationScope::Studio(studio_id)`:
  - Cancels all non-terminal tasks belonging to the studio.
  - Cancels all active runs on tasks in the studio.
  - Cancels all pending approval requests in the studio.
- `CancellationScope::Task { task_id, include_descendants }`:
  - Cancels active runs and pending approvals associated with `task_id`.
  - If `include_descendants == true`, traverses both subtasks (`parent_task_id == task_id`) and graph dependents iteratively, cancelling all discovered non-terminal tasks, their active runs, and pending approvals.
- `CancellationScope::Agent(agent_id)`:
  - Cancels all active runs assigned to that agent and cancels any pending approvals created by that agent.
- `CancellationScope::Run(run_id)`:
  - Cancels the specific run attempt.

### Idempotency
Cancelling an entity that is already terminal (`Succeeded`, `Failed`, `Cancelled`) is a safe no-op. It will not return an error or emit duplicate cancellation events, and returns an empty cancellation summary.

---

## 8. Event Sourcing & Replay Engine

### Event Storage
The `EventStore` trait defines the persistence contract:
```rust
pub trait EventStore: Send + Sync {
    fn append(&mut self, envelope: EventEnvelope) -> Result<(), StoreError>;
    fn events_for_studio(&self, studio_id: StudioId, from_sequence: u64) -> Result<Vec<EventEnvelope>, StoreError>;
    fn latest_sequence(&self, studio_id: StudioId) -> Result<u64, StoreError>;
    fn all_events(&self) -> Result<Vec<EventEnvelope>, StoreError>;
}
```

### Deterministic Replay
```rust
let replayed_cp = ControlPlane::replay_events(&all_events, clock, store)?;
```
Replay behavior:
1. Re-validates the strict monotonic sequencing of every incoming envelope.
2. Re-applies the domain events directly to internal collections and graph indices without re-emitting new events.
3. Produces a `ControlPlane` instance identical in state to the original instance prior to export.

---

## 9. Future Extensibility Path

1. **Embedded Database Persistence**: Implement `EventStore` on top of SQLite / SQLite WAL with per-studio transaction locks for durable on-disk state.
2. **IPC / RPC Adapters**: Expose `ControlPlane` over gRPC and JSON-RPC (or WebSocket streams) for communication with the frontend IDE / web UI.
3. **Pluggable Agent Runtimes**: Connect external agent processes using JSON over stdio or HTTP transports using the `AgentKind::External` discriminator.
