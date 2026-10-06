# Agent Studios Internal Multi-Agent Runtime

This document describes the design, architecture, and verification of the Agent Studios
internal multi-agent runtime (Milestone M08), implemented in `agent-studios-internal-agent`
and integrated with `agent-studios-control-plane`, `agent-studios-provider`,
`agent-studios-runtime-session`, and the upstream Codex engine (`codex-rs`).

---

## 1. Overview & Core Architecture

Agent Studios enables multiple AI agents to collaborate within a single Studio workspace.
Unlike external CLI orchestrators, the internal runtime runs directly inside the Agent Studios
process, building upon the foundations established across prior milestones:

```text
┌────────────────────────────────────────────────────────────────────────┐
│                        AgentStudiosSupervisor                          │
│                                                                        │
│   ┌──────────────────────┐              ┌──────────────────────────┐   │
│   │ Coordinator Planning │              │  Workspace Arbitrator    │   │
│   │   (Kahn DAG Sort)    │              │ (Exclusive Mutating,     │   │
│   └──────────┬───────────┘              │  Concurrent ReadOnly)    │   │
│              │                          └────────────┬─────────────┘   │
│              ▼                                       ▼                 │
│   ┌──────────────────────┐              ┌──────────────────────────┐   │
│   │  ControlPlaneActor   │              │   AgentBudgetTracker     │   │
│   │   (Single-Writer)    │              │  (Turns, Tools, Duration)│   │
│   └──────────┬───────────┘              └────────────┬─────────────┘   │
└──────────────┼───────────────────────────────────────┼─────────────────┘
               │                                       │
               ▼                                       ▼
┌────────────────────────────────────────────────────────────────────────┐
│                       AgentExecutor Backend                            │
│                                                                        │
│   ┌──────────────────────┐              ┌──────────────────────────┐   │
│   │  MockAgentExecutor   │              │    CodexAgentExecutor    │   │
│   │ (Deterministic Unit) │              │(RuntimeSessionFactory)   │   │
│   └──────────────────────┘              └────────────┬─────────────┘   │
└──────────────────────────────────────────────────────┼─────────────────┘
                                                       │
                                                       ▼
┌────────────────────────────────────────────────────────────────────────┐
│                   Codex ThreadManager & AgentControl                   │
│                                                                        │
│   - Patch 003: Explicit SpawnRequest.model_runtime_override            │
│   - Full Codex feature surface: tools, sandbox, MCP, skills, compaction│
└────────────────────────────────────────────────────────────────────────┘
```

### Key Principles

1. **Independent Agent Specifications**:
   Each agent in an `InternalTeamSpec` possesses an independent:
   - **Role & Display Name**: Defined in `InternalAgentSpec`.
   - **Provider Instance & Model**: Authoritative `ModelRef(ProviderInstanceId, ModelId)`.
   - **Reasoning Configuration**: `AgentReasoningSelection` (`None`, `Low`, `Medium`, `High`,
     `XHigh`, `Max`).
   - **Execution Budget**: `AgentExecutionBudget` limiting turns, tool calls, and wall-clock time.
   - **Workspace Access Mode**: `WorkspaceAccessMode::ReadOnly` vs `WorkspaceAccessMode::Mutating`.

2. **Strict Codex Feature Inheritance**:
   Sub-agents execute as first-class Codex threads. They inherit the complete Codex agent loop,
   native tool dispatch (`exec_command`, `apply_patch`), approvals, sandbox containment,
   MCP server connections, Skills (`SKILL.md`), Plugins, hooks, `AGENTS.md`, context history
   compaction, and session persistence without reducing or rewriting Codex primitives.

3. **Zero Plaintext Secrets**:
   Credentials never touch disk or logs in plaintext. Secret references resolve in-memory via
   `InMemorySecretResolver` directly to ephemeral HTTP transport headers or query parameters.

---

## 2. Upstream Codex Seam: Patch 003

To allow sub-agents spawned via Codex's native `AgentControl` to use a different provider or
model from their parent thread, a third minimal, provider-neutral seam was added to `codex-rs`:

### Generic Spawn Runtime Override

- **Location**: `codex-rs/core/src/agent/api.rs`, `codex-rs/core/src/agent/control.rs`,
  `codex-rs/core/src/thread_manager.rs`, `codex-rs/core/src/thread.rs`.
- **Interface**:
  ```rust
  pub struct SpawnRequest {
      pub prompt: String,
      pub agent_type: Option<AgentType>,
      pub model_runtime_override: Option<ModelRuntimeOverride>,
  }
  ```
- **Semantics**:
  - When `model_runtime_override` is `Some(override)`, the child thread adopts the specified
    `(SharedModelProvider, SharedModelsManager)` pair.
  - When `None`, legacy behavior is preserved: the child inherits the parent thread's model
    runtime override.
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

## 3. Core Subsystems

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

### B. Workspace Policy Arbitrator & Isolated Worktrees (`workspace_policy.rs`)

Multi-agent coordination requires deterministic access control over workspace files:
- **`WorkspaceAccessMode::ReadOnly`**: Tasks execute against shared source without exclusive lease.
- **`WorkspaceAccessMode::Mutating`**: Tasks require dedicated isolated worktrees. In Milestone M09,
  the global studio lock is replaced by per-worktree leases (`worktree-{worktree_id}`). Multiple
  mutating agents execute concurrently in parallel when allocated distinct dedicated worktrees.
- **RAII Leases (`WorkspaceLease`)**: Automatically decrements reader count or clears mutating
  ownership when dropped, guaranteeing that crashes or cancellations never leave dangling locks.

### B.1 Worktree Orchestration & Non-Teleporting Workers (Milestone M09)

Integrated with `agent-studios-workspace` and `codex_worktree::WorktreeManager`:

- **Thread-Worktree 1:1 Affinity**: Enforces `codex_worktree::bind_thread` semantics with `codex-thread.json`.
- **Non-Teleporting Worker Invariant**: A Codex thread's working directory (`Config.cwd`) is immutable.
  Reusing a worker thread across tasks is allowed ONLY when `existing.bound_workspace == requested.workspace_path`.
  If the workspace differs, the old worker is retired and a new thread is spawned with a fresh `ThreadId`
  pointing to the new worktree path.
- **Ephemeral Change Capture**: Staged diffs are computed using isolated temporary Git index files (`GIT_INDEX_FILE`),
  leaving active `.git/index` files unaffected.
- **Safe Patch Reconciliation**: Two-phase patch application on a dedicated integration worktree
  (`git apply --check --binary` dry-run first). Conflicts are recorded as `Conflicted` without failing
  the task (`TaskState = Succeeded`, `ReconciliationState = Conflicted`).
- **Dirty Worktree Retention**: Crashed, failed, or cancelled worktrees are retained on disk for inspection.

### C. Execution Budget Tracker (`budget.rs`)

Protects system resources and prevents runaway agent loops:
- `AgentExecutionBudget`: Configures maximum turns, tool calls, and wall-clock duration in seconds.
- `AgentBudgetTracker`: Dynamically increments turn and tool call counters during execution and
  checks elapsed wall-clock time. Returns typed errors:
  - `InternalAgentError::BudgetExceeded(BudgetLimitType::Turns, ...)`
  - `InternalAgentError::BudgetExceeded(BudgetLimitType::ToolCalls, ...)`
  - `InternalAgentError::BudgetExceeded(BudgetLimitType::WallClock, ...)`

### D. Coordinator DAG Planning & Kahn's Validation (`coordinator.rs`)

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

### E. Supervisor & Scheduler Loop (`supervisor.rs`)

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
   - On failure: Evaluates configured `FailurePolicy`:
     - `FailFast`: Cancels all pending tasks and issues studio-wide cancellation.
     - `ContinueIndependent`: Marks failed task as `Failed`; independent tasks continue.
     - `RetryTask(max)`: Requeues task to `Ready` up to `max` retry attempts.

---

## 4. Pluggable Execution Backends (`executor.rs`)

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

## 5. Verification & Test Evidence

All 26 tests in `agent-studios-internal-agent` pass with zero warnings:

| Test Suite | Tests | Description |
| :--- | :--- | :--- |
| `profile_and_team_tests` | 5 | Validates aliases, duplicate ID rejection, reasoning effort, budget builder |
| `control_plane_actor_tests` | 3 | Actor task dependency unblocking, run lifecycle, concurrent handle access |
| `workspace_policy_tests` | 3 | Mutator exclusivity, concurrent readers, async acquire with timeout |
| `budget_tests` | 3 | Enforces turn limits, tool call limits, and unlimited budget semantics |
| `coordinator_tests` | 7 | Linear DAG, diamond DAG, cycle rejection, self-cycle rejection, materialization |
| `supervisor_tests` | 3 | Full success workflow, retry policy on flaky worker, fail-fast cancellation |
| `cross_provider_team_e2e_tests` | 2 | 3-provider team execution (Gemini, Claude, GPT-4o), same-model-slug isolation |

### Cross-Provider E2E Verification Details

- **Coordinator**: Google Gemini (`ProtocolFamily::GeminiGenerateContent`, `gemini-2.5-pro`)
  receiving streaming responses via mock endpoint with `x-goog-api-key`.
- **Coder**: Anthropic Messages (`ProtocolFamily::AnthropicMessages`, `claude-3-7-sonnet-20250219`)
  receiving streaming responses with `x-api-key`.
- **Reviewer**: OpenAI Chat Completions (`ProtocolFamily::OpenAiChatCompletions`, `gpt-4o`)
  receiving streaming responses with `Authorization: Bearer`.
- **Isolation Guarantee**: Two independent instances configured with the identical model slug
  `gpt-4o` route strictly to their respective endpoints and secrets based on `ProviderInstanceId`.
