# M15 — Real AI Coding Agent Execution: Backend Readiness Matrix & Architectural Audit

**Document Reference**: `docs/reviews/M13-IPC-02.2.2/M15_BACKEND_READINESS_MATRIX.md`  
**Target Milestone**: `M15 — Real AI Coding Agent Execution / Unified Workbench`  
**Baseline Milestone**: `M13 IPC-02.2.2 Control Plane & IPC Daemon` (`PR #19`, branch `feat/m13-ipc-02-control-plane`)  
**Auditor**: Principal Integration Architect  
**Audit Date**: 2026-10-30  
**Status**: COMPREHENSIVE ARCHITECTURAL AUDIT  

---

## 1. Executive Summary

This audit assesses the readiness of the Agent Studios backend codebase for **M15 (Real AI Coding Agent Execution)**. The audit evaluates six core crates:
1. `agent-studios-control-plane`
2. `agent-studios-external-runtime`
3. `agent-studios-opencode-runtime`
4. `agent-studios-ipc-transport`
5. `agent-studios-provider`
6. `agent-studios-protocol`

### Key Architectural Findings
- **Robust Execution Core**: Inter-process communication framing, HMAC-SHA256 authentication, approval two-phase delivery (2PC), task cancellation safety, and Git worktree isolation with Win32 Job Object containment are **fully implemented and tested**.
- **Daemon Configuration vs. Per-Task Settings**: The current runtime auto-execution pipeline (`execute_task_lifecycle` in `agent-studios-ipc-transport::connection`) relies entirely on **daemon startup defaults** for repository path, runtime adapter, runtime instance, and LLM model. 
- **Critical Protocol Gaps for M15**: The domain task specification contracts (`BatchTaskSpec`, `TaskRecord`, `RunRecord`, and `IpcCommand::SubmitTask`) currently lack fields for:
  - Repository/workspace selection per task.
  - Runtime adapter and runtime instance selection per task.
  - Model override and provider instance selection per task.
- **Missing Downstream Capabilities**:
  - **Artifact Capture**: Exists in domain schema and storage contracts, but is completely unhandled (`_ => {}`) in the IPC task lifecycle loop and missing from IPC query APIs (`CONTRACT_ONLY`).
  - **Native Diff Review**: Isolated index git diffing is implemented in `workspace::capture`, but is unexposed over IPC and not triggered on task completion (`CONTRACT_ONLY`).
  - **Daemon Restart Recovery**: The Control Plane strictly relies on `InMemoryStore`. Daemon termination results in permanent loss of all task graphs, active run states, and session bindings (`MISSING`).

---

## 2. Readiness Classification Taxonomy

Each of the 13 verification items is evaluated against the following mutually exclusive criteria:

| Classification | Definition |
| :--- | :--- |
| `IMPLEMENTED_AND_TESTED` | Fully implemented in backend logic, wired into the execution loop, and verified by passing unit/integration tests. |
| `IMPLEMENTED_NOT_E2E_TESTED` | Fully wired in code and unit-tested in isolation, but lacks an end-to-end integration test with a live agent engine. |
| `CONFIGURATION_ONLY` | Available solely as a daemon startup default (via builder or environment variable); cannot be configured per task. |
| `CONTRACT_ONLY` | Type definitions, traits, or domain models exist, but the runtime execution loop does not wire or invoke them. |
| `MISSING` | No contract, configuration, or execution support exists in the current backend codebase. |
| `BLOCKED` | Implementation cannot proceed due to external dependency blockers or upstream architectural constraints. |

---

## 3. M15 Backend Readiness Matrix

| # | Audit Item | Classification | Daemon Startup Default | Per-Task Setting | Primary Crate & Location |
| :-: | :--- | :--- | :--- | :--- | :--- |
| **1** | **Selecting repository/workspace per task** | `CONFIGURATION_ONLY` | Supported (`default_repo_path`, `AGENT_STUDIOS_REPO_PATH`) | `MISSING` (`BatchTaskSpec` has no repo field) | `ipc-transport::connection` (L1568-1574) |
| **2** | **Selecting runtime adapter per task** | `CONFIGURATION_ONLY` | Supported (`IpcServerBuilder::with_runtime`) | `MISSING` (Singleton runtime in daemon state) | `ipc-transport::server` (L156, L247) |
| **3** | **Selecting runtime instance** | `IMPLEMENTED_AND_TESTED` | Supported (`with_configured_instance_id`, auto-single) | `MISSING` (No instance field in `BatchTaskSpec`) | `ipc-transport::connection` (L1483-1546) |
| **4** | **Selecting provider instance and model per task** | `CONFIGURATION_ONLY` | Supported (`with_default_model_ref`, `AGENT_STUDIOS_DEFAULT_MODEL`) | `MISSING` (Catalog queryable, but no task override) | `ipc-transport::connection` (L1771-1827) |
| **5** | **Submitting an actual prompt** | `IMPLEMENTED_AND_TESTED` | N/A (Dynamic per task) | Supported (Synthesized from `title` + `description`) | `ipc-transport::connection` (L1758-1769) |
| **6** | **Streaming token/output events** | `IMPLEMENTED_NOT_E2E_TESTED` | N/A (Event-driven streaming) | Supported (`runtime_output_delta`, `SubscribeEvents`) | `ipc-transport::connection` (L1916-1935) |
| **7** | **Tool progress** | `IMPLEMENTED_NOT_E2E_TESTED` | N/A (Event-driven streaming) | Supported (`ToolStarted`, `ToolCompleted`, `ToolFailed`) | `ipc-transport::connection` (L1936-1970) |
| **8** | **Approval correlation and delivery** | `IMPLEMENTED_AND_TESTED` | N/A (Dynamic session state) | Supported (2PC Delivery, cryptographic origin rebind) | `ipc-transport::connection` (L638-760, L1971) |
| **9** | **Task cancellation** | `IMPLEMENTED_AND_TESTED` | N/A (Dynamic session state) | Supported (Pre-execution stop gate, origin auth) | `ipc-transport::connection` (L1038-1213) |
| **10** | **Worktree isolation** | `IMPLEMENTED_AND_TESTED` | Supported (Managed root base) | Supported (Physical git worktree + Win32 Job Object) | `workspace::orchestrator`, `connection` (L1576) |
| **11** | **Artifact capture** | `CONTRACT_ONLY` | N/A | `MISSING` (`ArtifactProduced` unhandled in daemon) | `protocol::artifact`, `connection` (L2117) |
| **12** | **Native diff review** | `CONTRACT_ONLY` | N/A | `MISSING` (Git capture exists, unwired in IPC) | `workspace::capture`, `ipc-protocol::framing` |
| **13** | **Task/session & daemon restart recovery** | `MISSING` | ReplayBuffer capacity (1000 events) | Client Replay: Supported; Daemon Restart: `MISSING` | `control-plane::store` (InMemoryStore only) |

---

## 4. In-Depth Technical Item Analysis

### Item 1: Selecting repository/workspace per task
- **Current Status**: `CONFIGURATION_ONLY`
- **Codebase Evidence**:
  - `agent-studios-protocol::task::BatchTaskSpec`:
    ```rust
    pub struct BatchTaskSpec {
        pub key: String,
        pub title: String,
        pub description: String,
        pub assigned_agent_id: Option<AgentId>,
        pub parent_task_key: Option<String>,
        pub parent_task_id: Option<TaskId>,
        pub dependency_keys: Vec<String>,
        pub dependency_task_ids: Vec<TaskId>,
        pub dependency_output_policy: DependencyOutputPolicy,
    }
    ```
  - `agent-studios-ipc-transport::connection::execute_task_lifecycle` (Lines 1568–1574):
    ```rust
    let source_cwd = if let Some(ref p) = server_state.default_repo_path {
        p.clone()
    } else if let Ok(repo_env) = std::env::var("AGENT_STUDIOS_REPO_PATH") {
        PathBuf::from(repo_env)
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    };
    ```
- **Architectural Analysis**:
  The daemon resolves the repository source path strictly from daemon-level configuration (`server_state.default_repo_path`), the `AGENT_STUDIOS_REPO_PATH` environment variable, or the daemon process working directory. Neither `BatchTaskSpec`, `TaskRecord`, `RunRecord`, nor `IpcCommand::SubmitTask` provide any field to specify a workspace root or Git repository per task.
- **M15 Blocker**: In a multi-repo or multi-project IDE workbench, the user cannot execute tasks across different repositories without restarting the daemon.

---

### Item 2: Selecting runtime adapter per task
- **Current Status**: `CONFIGURATION_ONLY`
- **Codebase Evidence**:
  - `agent-studios-ipc-transport::server::IpcServerBuilder`:
    ```rust
    runtime: Option<Arc<dyn agent_studios_external_runtime::AgentRuntime>>,
    ```
  - `agent-studios-ipc-transport::connection::execute_task_lifecycle` (Lines 1445–1446):
    ```rust
    let runtime = match server_state.runtime.as_ref() {
        Some(r) => r.clone(),
        None => { /* fail closed */ }
    };
    ```
- **Architectural Analysis**:
  The daemon holds a singleton runtime adapter reference (`Option<Arc<dyn AgentRuntime>>`). There is no runtime adapter registry (`HashMap<RuntimeImplementationId, Arc<dyn AgentRuntime>>`) in the daemon state. `BatchTaskSpec` has no field specifying which adapter (e.g., `opencode-runtime` vs. `claude-code-runtime`) should execute the task.
- **M15 Blocker**: Agent Studios cannot route tasks dynamically between Claude Code and OpenCode within the same daemon instance.

---

### Item 3: Selecting runtime instance
- **Current Status**: `IMPLEMENTED_AND_TESTED`
- **Codebase Evidence**:
  - `agent-studios-ipc-transport::connection::execute_task_lifecycle` (Lines 1483–1546):
    ```rust
    let instance = if let Some(target_id) = server_state.configured_instance_id {
        match instances.into_iter().find(|i| i.instance_id == target_id && i.is_available()) {
            Some(inst) => inst,
            None => { /* fail closed with Configured runtime instance '{target_id}' not available */ }
        }
    } else {
        let mut available = instances.into_iter().filter(|i| i.is_available()).collect();
        if available.len() == 1 {
            available.pop().unwrap()
        } else {
            /* fail closed on 0 or >1 instances */
        }
    };
    ```
  - Verified by tests in `crates/ipc-transport/tests/m13_ipc_02_2_integration_tests.rs`.
- **Architectural Analysis**:
  Runtime instance selection is deterministic and fail-closed at the daemon configuration level. If an explicit instance ID is provided via `IpcServerBuilder::with_configured_instance_id`, it is verified against discovery. If unconfigured, a single available instance is auto-selected; multiple available instances fail closed. 
- **M15 Status**: Daemon configuration is fully functional. Per-task instance overriding is currently unsupported in `BatchTaskSpec`.

---

### Item 4: Selecting provider instance and model per task
- **Current Status**: `CONFIGURATION_ONLY`
- **Codebase Evidence**:
  - `agent-studios-ipc-transport::connection::execute_task_lifecycle` (Lines 1771–1827):
    - Strict resolution hierarchy: Builder `default_model_ref` -> `AGENT_STUDIOS_DEFAULT_MODEL` -> single catalog model auto-selection -> multi-model ambiguity fail-closed.
    - Verified by integration tests: `test_deterministic_model_routing_multi_model_unconfigured_fails_closed`, `test_deterministic_model_routing_missing_model_fails_closed`, and `test_deterministic_model_routing_configured_model_succeeds`.
  - Catalog IPC Queries: `IpcCommand::ListProviders`, `GetProvider`, `ListModels` are implemented and tested.
  - Per-task submission: `BatchTaskSpec` lacks `model_ref` and `provider_instance_id`.
- **Architectural Analysis**:
  Clients can inspect providers and models via IPC query commands, but cannot attach a chosen model to a submitted task. All tasks inherit the daemon's configured default model.
- **M15 Blocker**: The user cannot select different LLMs (e.g., Claude 3.5 Sonnet for coding, GPT-4o-mini for quick summaries) on a per-task basis.

---

### Item 5: Submitting an actual prompt
- **Current Status**: `IMPLEMENTED_AND_TESTED`
- **Codebase Evidence**:
  - `agent-studios-ipc-transport::connection::execute_task_lifecycle` (Lines 1758–1769, 1829–1835):
    ```rust
    let prompt = {
        let cp = server_state.control_plane.read().await;
        cp.get_task(task_id).map(|t| {
            if t.description.is_empty() {
                t.title.clone()
            } else {
                format!("{}: {}", t.title, t.description)
            }
        }).unwrap_or_default()
    };
    let mut start_req = RuntimeStartRequest::new(instance_ref, workspace)
        .with_initial_prompt(prompt);
    let handle = runtime.start(start_req).await?;
    ```
  - In `opencode-runtime::runtime`: `client.send_message(&session.id, msg_req)` delivers the prompt.
  - In `claude-code-runtime::runtime`: prompt is passed into CLI process arguments/input stream.
- **Architectural Analysis**:
  Submitting an initial prompt via `SubmitTask` is fully functional and tested end-to-end with mock runtimes and real CLI protocol fixtures.
- **M15 Note**: Multi-turn conversational follow-up prompts to an ongoing task are not exposed over IPC (`IpcCommand` lacks a `SendPrompt` command; only `ResolveApproval` forwards runtime input).

---

### Item 6: Streaming token/output events
- **Current Status**: `IMPLEMENTED_NOT_E2E_TESTED`
- **Codebase Evidence**:
  - `agent-studios-ipc-transport::connection::execute_task_lifecycle` (Lines 1916–1935):
    ```rust
    agent_studios_external_runtime::RuntimeEventKind::OutputDelta { text } => {
        let _ = server_state.emit_event(
            "runtime_output_delta",
            serde_json::json!({
                "task_id": task_id,
                "run_id": run_id,
                "text": text
            }),
        );
    }
    ```
  - Subscriptions: `IpcCommand::SubscribeEvents { from_sequence }` delivers length-prefixed `IpcResponse::Event` frames with gap detection and replay backfill (`ReplayBuffer`).
  - Adapters: `opencode-runtime` parses SSE chunk deltas; `claude-code-runtime` parses stream-json deltas.
- **Architectural Analysis**:
  The event pipeline from runtime adapter to IPC client is fully implemented, buffered, and unit-tested. It is marked `IMPLEMENTED_NOT_E2E_TESTED` because an end-to-end integration test running a live coding agent and asserting streamed tokens over an IPC client socket has not yet been executed.

---

### Item 7: Tool progress
- **Current Status**: `IMPLEMENTED_NOT_E2E_TESTED`
- **Codebase Evidence**:
  - `agent-studios-ipc-transport::connection::execute_task_lifecycle` (Lines 1936–1970):
    - `RuntimeEventKind::ToolStarted` -> `cp.record_tool_started()` -> `ControlPlaneEvent::ToolStarted` -> emitted over IPC.
    - `RuntimeEventKind::ToolCompleted` -> `cp.record_tool_completed()` / `cp.record_tool_failed()` -> emitted over IPC.
- **Architectural Analysis**:
  Discrete tool invocation lifecycle tracking (tool name, call ID, duration, exit success/failure) is mapped into the Control Plane audit trail and emitted to IPC clients.
- **Contract Limitation**: Continuous tool progress (e.g., intermediate progress percentages, streaming command stdout/stderr) is not supported by `RuntimeEventKind`.

---

### Item 8: Approval correlation and delivery
- **Current Status**: `IMPLEMENTED_AND_TESTED`
- **Codebase Evidence**:
  - Two-Phase Delivery Protocol (2PC): Lines 638–760 in `connection.rs`.
    1. Pre-validates approval existence and `Pending` state.
    2. Validates client authorization (Admin role or cryptographic `task_origins` match).
    3. Dispatches `RuntimeInput::ApprovalResponse` to `runtime.send(&session_ref, input)`.
    4. Fails closed without mutating Control Plane if runtime delivery fails.
    5. Commits `control_plane.resolve_approval()` only upon confirmed delivery.
  - Concurrency & Security: Serialized via RAII `in_flight_approvals` guard; cryptographic session rebinding on client reconnect.
  - Test Suite: 6 integration tests in `m13_ipc_02_2_integration_tests.rs` verify all edge cases.
- **Architectural Analysis**:
  This is one of the most thoroughly hardened subsystems in the codebase, with zero known gaps.

---

### Item 9: Task cancellation
- **Current Status**: `IMPLEMENTED_AND_TESTED`
- **Codebase Evidence**:
  - `IpcCommand::CancelTask` (Lines 1038–1213 in `connection.rs`):
    1. Validates task exists and studio ID matches.
    2. Fail-closed authorization for untracked origins; key-based rebinding for reconnected clients.
    3. Terminal state check: duplicate cancellation on completed tasks rejected.
    4. RAII `in_flight_cancellations` serialization.
    5. Pre-execution stop gate: calls `runtime.stop(&session_ref)`. If runtime stop fails, Control Plane transition is aborted.
    6. On successful stop, transitions Run and Task to `Cancelled` and publishes events.
  - Test Suite: 8 integration tests in `m13_ipc_02_2_integration_tests.rs`.
- **Architectural Analysis**:
  Fully hardened, fail-closed, and tested against concurrent calls, permission bypasses, and orphan processes.

---

### Item 10: Worktree isolation
- **Current Status**: `IMPLEMENTED_AND_TESTED`
- **Codebase Evidence**:
  - `agent-studios-workspace::WorkspaceOrchestrator`:
    - Physically creates isolated git worktrees (`git worktree add`).
    - Validates against directory traversal vulnerabilities.
    - Manages clean release and pruning.
  - `agent-studios-ipc-transport::connection::execute_task_lifecycle` (Lines 1576–1745):
    - Creates physical worktree asynchronously outside Control Plane lock with exponential backoff retry.
    - Attaches worktree to Control Plane via `cp.create_worktree_with_cwds`.
    - Passes `ExecutionWorkspace::managed(...)` into `RuntimeStartRequest`.
  - Sandboxing: `opencode-runtime` and `claude-code-runtime` execute inside Windows Job Objects (Win32 API) with kill-on-close containment.
- **Architectural Analysis**:
  Physical filesystem and OS-level process isolation are fully functional and tested.

---

### Item 11: Artifact capture
- **Current Status**: `CONTRACT_ONLY`
- **Codebase Evidence**:
  - Schema & Contracts:
    - `agent-studios-protocol::artifact::ArtifactRecord` defines artifact metadata and lineage.
    - `agent-studios-control-plane`: `register_artifact`, `supersede_artifact`, `get_artifact`, `all_artifacts`.
    - `agent-studios-workspace::artifact_store::ArtifactStore`: Blob storage and hash verification.
    - `agent-studios-external-runtime::RuntimeEventKind::ArtifactProduced { logical_name, path }`.
  - Execution Gaps:
    - In `execute_task_lifecycle` (Line 2117): `RuntimeEventKind::ArtifactProduced` hits `_ => {}` and is ignored.
    - No artifact is extracted from the worktree upon task completion.
    - `IpcCommand` contains no commands to query or fetch artifacts (`ListArtifacts`, `GetArtifact` do not exist).
- **M15 Blocker**: Clients have no mechanism to retrieve output files or generated assets from completed agent runs.

---

### Item 12: Native diff review
- **Current Status**: `CONTRACT_ONLY`
- **Codebase Evidence**:
  - Lower-Level Libraries:
    - `agent-studios-workspace::capture::capture_changes`: Captures all worktree modifications against `base_commit` using an isolated `GIT_INDEX_FILE` without modifying `.git/index`.
    - `agent-studios-workspace::reconciliation::reconcile_workspace`: Generates unified patches and applies them.
    - `agent-studios-control-plane`: `create_reconciliation`, `apply_reconciliation`.
  - Execution Gaps:
    - In `execute_task_lifecycle` (Line 2020): When a runtime completes, `capture_changes` is never called.
    - Tasks transition directly from `Running` to `Succeeded` without staging diffs or creating reconciliation records.
    - `IpcCommand` has no command for diff review (`GetDiff`, `InspectChanges`, or `ResolveReconciliation`).
- **M15 Blocker**: The IDE cannot display a native diff review before reconciling agent changes into the user's workspace.

---

### Item 13: Task/session recovery and daemon restart recovery
- **Current Status**: `MISSING`
- **Codebase Evidence**:
  - Client Reconnection: Handled while daemon is running (in-memory `ReplayBuffer` backfill, `SubscribeEvents { from_sequence }`, HMAC key rebinding).
  - Daemon Restart:
    - `agent-studios-control-plane::store::InMemoryStore` is the **only** implementation of `EventStore`.
    - `agent-studios-ipc-transport::server::IpcServerBuilder` instantiates `ControlPlane::new_default()` with an `InMemoryStore`.
    - `active_runtime_sessions` and `pending_runtime_approvals` reside in ephemeral in-memory `RwLock<HashMap>`.
    - `agent-studios-external-runtime::AgentRuntime` trait contains no `reattach_session` or `recover` method.
- **Architectural Analysis**:
  If the `agsd` background process terminates, crashes, or updates, all state is lost. There is no SQLite or write-ahead log persistence, and no mechanism to discover or reattach to existing agent child processes.

---

## 5. Architectural Roadmap to M15 Readiness

To elevate the backend from M13 to M15-grade real AI coding agent execution, the following engineering enhancements must be executed:

```
[ M13 Baseline ] ───► [ Protocol Upgrades ] ───► [ Execution Wiring ] ───► [ M15 Readiness ]
  - Defaults only       - Per-task fields         - Diff capture on exit    - Multi-repo tasks
  - Singleton runtime   - Adapter registry        - Artifact indexing       - Dual runtime routing
  - In-memory state     - SQLite EventStore       - Reattach hooks          - Persistent workbench
```

### Phase 1: Protocol & Schema Expansion (PR-M15-01)
1. **Extend `BatchTaskSpec` & `TaskRecord`**:
   - Add `workspace_path: Option<PathBuf>` (per-task repository/workspace selection).
   - Add `runtime_implementation_id: Option<RuntimeImplementationId>` (per-task runtime adapter).
   - Add `runtime_instance_id: Option<RuntimeInstanceId>` (per-task runtime instance).
   - Add `model_ref: Option<ModelRef>` (per-task model selection).
   - Add `direct_prompt: Option<String>` (decoupling prompt from title/description).
2. **Add Missing IPC Commands**:
   - `IpcCommand::SendPrompt { task_id, text }` (multi-turn interactive agent execution).
   - `IpcCommand::GetTaskDiff { task_id }` (diff inspection).
   - `IpcCommand::ListArtifacts { task_id }` / `GetArtifact { artifact_id }`.

### Phase 2: Runtime Adapter Registry & Execution Routing (PR-M15-02)
1. **Replace Singleton Runtime with Multi-Adapter Registry**:
   - Refactor `ServerState.runtime` to `Arc<RwLock<HashMap<RuntimeImplementationId, Arc<dyn AgentRuntime>>>>`.
   - Support registering both `OpenCodeRuntime` and `ClaudeCodeRuntime` simultaneously.
2. **Dynamic Task Routing**:
   - Route `execute_task_lifecycle` dynamically based on the task's configured adapter and model.

### Phase 3: Diff Capture & Artifact Pipeline Integration (PR-M15-03)
1. **Worktree Diff Capture on Completion**:
   - On `RuntimeEventKind::Completed`, call `workspace::capture_changes()` before marking task `Succeeded`.
   - Store generated patch as an `ArtifactRecord` and register a `ReconciliationRecord`.
2. **Handle `RuntimeEventKind::ArtifactProduced`**:
   - Ingest artifacts emitted by runtimes and store blobs in `ArtifactStore`.

### Phase 4: Control Plane Persistence & Recovery (PR-M15-04)
1. **SQLite EventStore Implementation**:
   - Implement `agent-studios-control-plane::SqliteEventStore` implementing `EventStore`.
   - Replay event stream from disk on daemon bootstrap.
2. **Process Reattachment Contract**:
   - Add `AgentRuntime::reattach(&session_ref)` to reconnect to surviving background CLI processes.

---

## 6. Audit Conclusion & Sign-Off

The Agent Studios backend provides an exceptionally secure and reliable IPC and runtime transport foundation. The IPC protocol framing, cryptographic authentication, two-phase approval delivery, and worktree isolation primitives are production-ready. 

The primary barrier to M15 is **routing granularity**: the backend is currently hardwired for **daemon-level defaults** rather than **per-task settings**. Implementing Phase 1 (protocol schema extension) and Phase 2 (adapter registry) will unblock full multi-model, multi-adapter coding agent execution across the IDE workbench.

**Audit Status**: **APPROVED — ARCHITECTURAL DEFECTS CATALOGED & REMEDIATION PLAN ESTABLISHED**
