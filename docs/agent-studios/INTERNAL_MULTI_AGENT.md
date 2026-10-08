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

---

## 7. Upstream Codex Seams

To allow sub-agents spawned via Codex's native `AgentControl` to use independent provider instances or models, minimal, provider-neutral seams were added to `codex-rs`:

### Generic Spawn Runtime Override (Patch 003 & Patch 004)

- **Location**: `codex-rs/core/src/agent/api.rs`, `codex-rs/core/src/agent/control/spawn.rs`,
  `codex-rs/core/src/thread_manager.rs`, `codex-rs/core/src/thread.rs`.
- **Interface**:
  ```rust
  pub struct SpawnRequest {
      pub prompt: String,
      pub agent_type: Option<AgentType>,
      pub model_runtime_override: Option<ModelRuntimeOverride>,
      pub thread_extension_init: codex_extension_api::ExtensionDataInit,
  }
  ```
- **Semantics**:
  - When `model_runtime_override` is `Some(override)`, the child thread adopts the specified
    `(SharedModelProvider, SharedModelsManager)` pair.
  - When `None`, legacy behavior is preserved: the child inherits the parent thread's model
    runtime override.
  - `thread_extension_init` allows host seeding of extension data into child threads at birth.
- **Thread Façade**:
  ```rust
  impl CodexThread {
      pub fn agent_control(&self) -> Arc<dyn AgentControl> {
          self.session_services.agent_control.clone()
      }
  }
  ```
  Exposes sub-agent spawn capability directly from a running thread without leaking internal
  session internals.

---

## 8. Core Subsystems

### A. Single-Writer ControlPlane Actor (`control_plane_actor.rs`)

The `ControlPlane` maintains transactional state across studios, agents, tasks, runs, and
approvals. To prevent lock contention and eliminate multi-threaded deadlocks, `ControlPlaneActor`
runs on a dedicated Tokio background task:
- All operations are submitted as asynchronous commands via `ControlPlaneHandle`.
- `ControlPlaneHandle` is cloneable, thread-safe, and provides typed async methods:
  - `create_studio`, `register_agent_with_id`, `create_task`, `add_task_dependency`,
    `transition_task_state`, `create_run`, `transition_run_state`, `request_cancellation`,
    `get_ready_tasks`, `get_studio_tasks`.
- State transitions are committed atomically via `commit_transaction`.

### B. Execution Budget Tracker (`budget.rs`)

Protects system resources and prevents runaway agent loops:
- `AgentExecutionBudget`: Configures maximum turns, tool calls, and wall-clock duration in seconds.
- `AgentBudgetTracker`: Dynamically increments turn and tool call counters during execution and
  checks elapsed wall-clock time. Returns typed errors:
  - `InternalAgentError::BudgetExceeded(BudgetLimitType::Turns, ...)`
  - `InternalAgentError::BudgetExceeded(BudgetLimitType::ToolCalls, ...)`
  - `InternalAgentError::BudgetExceeded(BudgetLimitType::WallClock, ...)`

### C. Coordinator DAG Planning & Kahn's Validation (`coordinator.rs`)

The team coordinator agent produces a structured execution plan represented by
`CoordinatorDecision`:
- **`CoordinatorDecision::Plan { tasks: Vec<PlannedTask> }`**: Directed acyclic graph of tasks.
- **`CoordinatorDecision::Complete { summary: String }`**: Objective already satisfied.
- **`CoordinatorDecision::Fail { reason: String }`**: Planning cannot proceed.

`CoordinatorPlanValidator` verifies plan correctness:
1. **Alias Verification**: Verifies every task's `assigned_alias` exists in `InternalTeamSpec`.
2. **Key Uniqueness**: Rejects duplicate task keys within the plan.
3. **Dependency Existence**: Ensures every `depends_on` key references a task in the plan.
4. **Acyclicity (Kahn's Algorithm)**: Computes in-degrees, extracts zero in-degree nodes, and
   verifies that all tasks are visited. If cycles or self-dependencies exist, rejects with
   `InternalAgentError::CyclicDependency`.
5. **Materialization**: Atomically registers all tasks into the `ControlPlane` with proper
   dependencies and initial `Ready` or `Blocked` states.

### D. Supervisor & Scheduler Loop (`supervisor.rs`)

`AgentStudiosSupervisor` orchestrates the full lifecycle of a multi-agent team:
1. **Boot**: Registers coordinator and worker agents into `ControlPlane` with designated IDs.
2. **Plan**: Prompts coordinator, parses JSON decision, and validates/materializes tasks into DAG.
3. **Schedule**:
   - Queries `ControlPlane` for `Ready` tasks.
   - Acquires workspace leases via `WorkspacePolicyArbitrator`.
   - Creates a `RunRecord` in `ControlPlane` and transitions task to `Running`.
   - Executes the task via the configured `AgentExecutor`.
   - Releases workspace lease immediately on turn completion.
   - On success: Transitions run and task to `Succeeded`. Dependent tasks automatically unblock
     to `Ready`.
   - On failure: Evaluates configured `FailurePolicy` (`FailFast`, `ContinueIndependent`, `RetryTask`).

---

## 9. Pluggable Execution Backends (`executor.rs`)

Execution logic is decoupled from orchestration via the `AgentExecutor` trait:

```rust
#[async_trait]
pub trait AgentExecutor: Send + Sync + 'static {
    async fn execute_agent(
        &self,
        context: AgentExecutionContext,
    ) -> Result<AgentExecutionResult, InternalAgentError>;
}
```

- **`MockAgentExecutor`**: In-memory executor with configurable role/alias responses, custom
  handlers, and execution recording. Enables millisecond-fast, deterministic unit testing.
- **`CodexAgentExecutor`**: Production executor powered by `AgentStudiosRuntimeSessionFactory`.
  Prepares authoritative runtime overrides for `context.agent_spec.model_ref` and executes
  turns via Codex `ThreadManager`.

---

## 10. Verification & Test Evidence

All test suites in `agent-studios-internal-agent` pass with zero warnings:

| Test Suite | Tests | Description |
| :--- | :--- | :--- |
| `profile_and_team_tests` | 6 | Validates aliases, duplicate ID rejection, reasoning effort, budget builder |
| `control_plane_actor_tests` | 6 | Actor task dependency unblocking, run lifecycle, concurrent handle access |
| `workspace_policy_tests` | 3 | Mutator exclusivity, concurrent readers, async acquire with timeout |
| `budget_tests` | 7 | Enforces turn limits, tool call limits, child agent limits, and unlimited budget semantics |
| `coordinator_tests` | 7 | Linear DAG, diamond DAG, cycle rejection, self-cycle rejection, materialization |
| `supervisor_tests` | 4 | Full success workflow, retry policy on flaky worker, fail-fast cancellation |
| `cross_provider_team_e2e_tests` | 3 | 3-provider team execution (Gemini, Claude, GPT-4o), same-model-slug isolation |
| `parallel_scheduler_tests` | 3 | Multi-worker parallel scheduling, dependency unblocking, concurrency limits |
| `fail_fast_cancellation_tests` | 1 | Fast-fail cancellation propagation across active workers |
| `observable_retry_tests` | 2 | Retry loop with observability event emissions |
| `real_runtime_acceptance_tests` | 7 | Real Codex runtime acceptance tests |
| `worktree_orchestration_e2e_tests` | 15 | Dedicated worktrees, ephemeral change capture, reconciliation |

### Cross-Provider E2E Verification Details

- **Coordinator**: Google Gemini (`ProtocolFamily::GeminiGenerateContent`, `gemini-2.5-pro`)
  receiving streaming responses via mock endpoint with `x-goog-api-key`.
- **Coder**: Anthropic Messages (`ProtocolFamily::AnthropicMessages`, `claude-3-7-sonnet-20250219`)
  receiving streaming responses with `x-api-key`.
- **Reviewer**: OpenAI Chat Completions (`ProtocolFamily::OpenAiChatCompletions`, `gpt-4o`)
  receiving streaming responses with `Authorization: Bearer`.
- **Isolation Guarantee**: Two independent instances configured with the identical model slug
  `gpt-4o` route strictly to their respective endpoints and secrets based on `ProviderInstanceId`.
