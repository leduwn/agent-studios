# Agent Studios — Internal Multi-Agent Runtime

> **Status**: Core Architecture Specification
>
> **Precedence**: Subservient to `MASTER_VISION.md` and `PRODUCT_PRINCIPLES.md`.

---

## 1. Overview & Real Codex Hierarchy

The Agent Studios internal multi-agent runtime (`agent-studios-internal-agent`) coordinates teams of specialized AI agents within a single Studio workspace.

Unlike external multi-process CLI wrappers, Agent Studios executes agents directly within the native Codex engine process (`codex-rs`). Sub-agents are organized in a **hierarchical agent tree** spawned via Codex's native `AgentControl` interface:

### Canonical Lifecycle & Dispatch Primitives
1. **Coordinator Root Initialization**:
   `ThreadManager::start_thread()` initializes the authoritative root Coordinator thread.
2. **Worker Creation**:
   Parent Coordinator creates child workers via `AgentControl::spawn()`.
3. **Worker Reuse / Later Turns**:
   Subsequent turns or reassigned tasks dispatch to existing child agents via `AgentControl::send()`.
4. **Cancellation / Interruption**:
   Active turns are safely interrupted via `AgentControl::interrupt()`.

### Conceptual Hierarchy

```text
               Coordinator (Root)
               [ThreadManager::start_thread()]
                       │
       ┌───────────────┼───────────────┐
       ▼               ▼               ▼
     Coder          Reviewer         Tester
   [Worker A]      [Worker B]      [Worker C]
 [spawn/send]    [spawn/send]    [spawn/send]
```

### Invariant: Child Hierarchy, Not Disconnected Root Sessions
Sub-agents are genuine child threads within the single Codex thread hierarchy, sharing root extension contexts, model overrides, and budget tracking. Agent Studios rejects launching multiple disconnected root CLI processes pretending to coordinate.

*(Note on `fork`: While low-level thread-forking primitives exist within upstream Codex codebase for conversational branching, canonical Agent Studios worker creation is strictly performed via `AgentControl::spawn` and `AgentControl::send`.)*

### Native Codex Feature Inheritance
Sub-agents are first-class Codex threads inheriting the full portable capability set:
- Native agent loop and tool dispatch (`exec_command`, `apply_patch`).
- Interactive approvals and sandbox containment.
- External tool integration via MCP and Skills (`SKILL.md`).
- Context history management and compaction.
- Hierarchical instruction inheritance via `AGENTS.md`.

---

## 2. Cognitive vs. Deterministic Boundary

Agent Studios enforces an absolute separation of concerns between cognitive reasoning and deterministic control:

| Dimension | Cognitive Coordinator (LLM) | Deterministic Control Plane (Rust Kernel) |
| :--- | :--- | :--- |
| **Nature** | Probabilistic, creative, heuristic | Typed, immutable, mathematical, transactional |
| **Responsibilities** | • Decomposes high-level user goals<br>• Synthesizes architectural plans<br>• Assigns agent roles & model selections<br>• Generates code & conducts heuristic reviews | • Enforces DAG acyclicity (Kahn's algorithm)<br>• Enforces task & agent state transitions<br>• Schedules tasks by priority & readiness<br>• Allocates isolated Git worktrees<br>• Enforces pre-execution tool & turn budgets<br>• Owns artifact versioning & SHA-256 blobs<br>• Persists monotonic event log & replay<br>• Propagates hierarchical cancellation |
| **Failure Mode** | Hallucination, context drift, bad syntax | Fail-closed errors, rollback, crash retention |

The LLM proposes actions; the Control Plane validates and executes state transitions.

---

## 3. Agent State vs. Task State Invariant

Agent lifecycle and task lifecycle are strictly decoupled:

```text
AGENT LIFECYCLE:
Registered ──► Starting ──► Idle ◄────► Busy ──► Stopping ──► Stopped
                             │
            Operational: [ Running | Waiting | Blocked |
                           Retrying | Paused | Failed | Cancelled ]

TASK LIFECYCLE:
Pending ──► Blocked ◄────► Ready ──► Running ──► Succeeded
                             │          │
                             ▼          ├──► Failed
                         Cancelled      └──► Cancelled
```

### Crucial Invariants
1. **Semantic Independence**: An agent can be `Idle` while its assigned task is `Blocked` waiting for upstream dependencies or human approval.
2. **Blocked is NOT Cancelled**: A task that cannot proceed due to an unfulfilled dependency, patch reconciliation conflict, or waiting approval is `Blocked`, never `Cancelled`. Cancellation permanently revokes intent; blocked is a conditional pause awaiting resolution.

---

## 4. Workspace Access Policy

Multi-agent coordination requires strict access arbitration over workspace directories:

### Canonical Policy Modes
- **`WorkspaceAccessMode::ReadOnly`**: Multiple agents may hold concurrent read leases on the same workspace key.
- **`WorkspaceAccessMode::Mutating`**: Exactly one agent may hold a mutating lease. Concurrent read or mutate requests are rejected or queued.
- **Physical Isolation via Worktrees**: In Milestone M09, mutating agents are assigned physically isolated Git worktrees, allowing true parallel mutation without file contention.

### Implementation Detail: `WorkspacePolicyArbitrator`
Within the `agent-studios-internal-agent` crate (`src/workspace_policy.rs`), workspace access arbitration is concretely implemented by `WorkspacePolicyArbitrator`:
- Manages thread-safe in-memory slots (`WorkspaceSlot::Mutating` vs. `WorkspaceSlot::ReadOnly`).
- Allocates RAII `WorkspaceLease` handles that release locks automatically on drop, guaranteeing that thread panics or cancellations never leak access locks.

The canonical architectural concept is **workspace access policy** (`ReadOnly` vs `Mutating`), mapped to real permission and sandbox containment.

---

## 5. Pre-Execution Tool Budget Enforcement

To prevent runaway agent loops, token exhaustion, and catastrophic command execution, Agent Studios enforces tool budgets **before** tools execute:

### The `AgentStudiosToolLifecycleContributor`
Codex provides a `ToolLifecycleContributor` extension seam:
1. **Pre-Execution Check (`authorize_tool_call`)**:
   - Synchronously checks and increments tool call counters.
   - If `tool_calls_used >= max_tool_calls`, records `BudgetExceeded("tool_calls")` and returns an authorization error.
   - **Codex emits `ToolCallOutcome::Blocked` before the tool handler runs**. The tool command is never executed.
2. **Lifecycle Events (`on_tool_start` / `on_tool_finish`)**:
   - Emits structured `ControlPlaneEvent::ToolStarted`, `ToolCompleted`, or `ToolFailed` with execution duration and safe error classification.
   - Plaintext arguments, shell commands, and secrets are strictly excluded from event logs.

---

## 6. Failure Policies

When a worker task fails, the `AgentStudiosSupervisor` evaluates the configured `FailurePolicy`:

1. **`FailFast`**:
   - Terminates the run immediately.
   - Cancels all running and pending tasks via `AgentControl::interrupt`.
   - Issues studio-wide cancellation and records the failure classification.
2. **`ContinueIndependent`**:
   - Marks the failed task as `Failed`.
   - Cancels downstream tasks dependent on the failed task.
   - Allows completely independent DAG branches to continue execution to completion.
3. **`RetryTask(max_attempts)`**:
   - Re-queues the failed task back to `Ready` up to `max_attempts`.
   - Increments the retry counter and re-schedules the task with clean execution context.
   - Transitions to `Failed` if retry attempts are exhausted.
