# Agent Studios — Multi-Agent Orchestration & Control Plane Specification

> **Status**: Authoritative Orchestration & Control Plane Specification  
> **Milestone**: M15 Consolidation & Canonical Master Reconstruction  
> **Target Branch**: `integration/m15-unified-codebase`  
> **Reference**: Implements `MASTER_VISION.md` §4, `PRODUCT_PRINCIPLES.md`, `INTERNAL_MULTI_AGENT.md`, and `WORKTREE_TASK_ARTIFACT.md`

---

## 1. Architectural Foundation: Separation of Cognition and Determinism

`[EXISTING_CANONICAL_DECISION]`
Agent Studios enforces an absolute architectural boundary between probabilistic cognitive reasoning and deterministic state orchestration:

```text
┌────────────────────────────────────────────────────────────────────────┐
│                   PROBABILISTIC COGNITIVE LAYER                        │
│                 (Cognitive Coordinator & Worker LLMs)                  │
├────────────────────────────────────────────────────────────────────────┤
│  - Goal understanding & decomposition                                  │
│  - Architecture planning & task formulation                            │
│  - Code generation, syntax analysis, refactoring heuristics            │
│  - Review feedback generation & qualitative validation                 │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ Proposes Actions / Submits DAG
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                   DETERMINISTIC CONTROL PLANE LAYER                    │
│                      (Rust Kernel: agent-studios-control-plane)        │
├────────────────────────────────────────────────────────────────────────┤
│  - Single-writer actor model (ControlPlaneActor)                       │
│  - Kahn's algorithm DAG cycle detection and topological scheduling     │
│  - State machine invariants (TaskState, AgentState, RunState)          │
│  - Workspace isolation leases (WorkspacePolicyArbitrator)              │
│  - Pre-execution tool budgets (AgentStudiosToolLifecycleContributor)   │
│  - Content-addressed SHA-256 artifact storage & version allocation     │
│  - Monotonic, gapless SQLite WAL event log                             │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Canonical Product Workflow

`[EXISTING_CANONICAL_DECISION]`
Every multi-agent execution follows this strict six-stage lifecycle:

$$\text{Goal} \longrightarrow \text{Run} \longrightarrow \text{Task Graph (DAG)} \longrightarrow \text{Assigned Agents} \longrightarrow \text{Execution} \longrightarrow \text{Result}$$

1. **Goal Formulation**: Operator submits a high-level technical objective via Agent Mode or IDE Mode.
2. **Run Initialization**: A unique `RunId` is generated with pre-execution token, step, and timeout budgets.
3. **DAG Generation & Validation**: The Coordinator decomposes the goal into a task graph. The Control Plane verifies acyclicity via Kahn's algorithm.
4. **Agent Allocation**: Specialized worker roles (`Architect`, `Coder`, `Tester`, `Reviewer`, `Security`) are bound to specific tasks with assigned `ModelRef` targets.
5. **Isolated Execution**: Workers execute within dedicated Git worktrees. File changes are captured via ephemeral Git index files.
6. **Result Reconciliation**: Patch artifacts are verified via `git apply --check` and merged into the integration branch. Compiler and test outcomes determine final status.

---

## 3. Task Graph (DAG) State Machine & Invariants

`[IMPLEMENTED_BEHAVIOR]`
### 3.1 Task Lifecycle States
```text
               ┌───────────────┐
               │    Pending    │
               └───────┬───────┘
                       │ Upstream Dependencies Unmet
                       ▼
               ┌───────────────┐
         ┌────►│    Blocked    │◄────────┐
         │     └───────┬───────┘         │
         │             │ Dependencies Met│ Reconciliation Conflict /
         │             ▼                 │ Waiting Approval
         │     ┌───────────────┐         │
         │     │     Ready     │         │
         │     └───────┬───────┘         │
         │             │ Worker Dispatched
         │             ▼                 │
         │     ┌───────────────┐         │
         │     │    Running    │─────────┘
         │     └───────┬───────┘
         │             │
    ─────┴─────────────┼─────────────────────────
      Terminal States: │
                       ├──────────────────┬──────────────────┐
                       ▼                  ▼                  ▼
               ┌───────────────┐  ┌───────────────┐  ┌───────────────┐
               │   Succeeded   │  │    Failed     │  │   Cancelled   │
               └───────────────┘  └───────────────┘  └───────────────┘
```

### 3.2 Canonical State Invariants
- `[EXISTING_CANONICAL_DECISION]` **Blocked != Cancelled**: A task paused for an approval, dependency, or patch conflict is `Blocked`. Cancellation requires explicit operator termination.
- `[EXISTING_CANONICAL_DECISION]` **Agent State != Task State**: An agent may be `Idle` waiting for work while its assigned task is `Blocked`. Never conflate these two lifecycles.
- `[IMPLEMENTED_BEHAVIOR]` **Truthful Success**: A task transitions to `Succeeded` only when compilers, linters, and tests return exit code 0.

---

## 4. Worktree Isolation & Ephemeral Index Change Capture

`[IMPLEMENTED_BEHAVIOR]`
### 4.1 Physical Directory Isolation
To eliminate file contention and race conditions during concurrent coding runs:
- Dedicated worktree path: `.git/agent-studios/worktrees/<session_id>/<task_id>/`
- Working tree checkout: Bound to a task-specific temporary branch `agent-studios/task-<task_id>`.
- Invariant: A thread's execution identity is permanently bound to its allocated worktree `cwd`. Mutating `cwd` out-of-band is prohibited.

### 4.2 Ephemeral Index Capture
Instead of dirtying the main Git index, Agent Studios isolates index operations:
```bash
export GIT_INDEX_FILE=".git/agent-studios/indices/idx_<task_id>"
git read-tree HEAD
git add -A
git diff --cached --binary > patch.diff
```
This guarantees that uncommitted operator changes in the primary workspace are completely isolated and immune to agent mutation.

### 4.3 Safe Worktree Retention
`[EXISTING_CANONICAL_DECISION]`
Standard automated lifecycle cleanup is strictly prohibited from running:
1. `git reset --hard`
2. `git clean -fdx`
3. `git worktree remove --force`
Conflicted, dirty, or failed worktrees are retained for manual developer inspection.

---

## 5. Single-Writer Lease & Deterministic Patch Reconciliation

`[IMPLEMENTED_BEHAVIOR]`
### 5.1 Workspace Access Arbitrator
- **ReadOnly Mode**: Multiple read-only inspections permitted concurrently.
- **Mutating Mode**: Enforces single-writer mutual exclusion via RAII `WorkspaceLease` guard. If a lease is held, subsequent mutating tasks queue until the lease drops.

### 5.2 Patch Reconciliation Engine
When a worker completes its task:
1. Unified binary patch artifact is generated and indexed by content hash (`blobs/sha256/<hash>`).
2. Control Plane allocates the authoritative artifact version (`v1`, `v2`, etc.).
3. Dry-run verification: `git apply --check --binary patch.diff`.
4. Fallback resolution: `git apply --3way patch.diff`.
5. Clean merge: Patch applied to integration branch; task marks `Succeeded`.
6. Conflict detected: Task transitions to `TaskState::Blocked(ReconciliationConflict)`. Operator prompted in Review tab to resolve.
