# Agent Studios CLI (`ags`) — Backend & Runtime Integration Architecture

> **Document**: `AGS_CLI_INTEGRATION_ARCH.md`  
> **Status**: Canonical Backend & Runtime Integration Architecture Specification  
> **Target Subsystem**: `agent-studios-cli-client` / IPC Transport / Event Subscription Pipeline / Dual Execution Modes  
> **Precedence**: Aligned with `AGS_CLI_PRODUCT_VISION.md`, `MASTER_VISION.md`, and `PRODUCT_PRINCIPLES.md`.

---

## 1. Executive Architectural Overview

The Agent Studios CLI (`ags`) is designed to deliver a high-velocity, deterministic multi-agent development environment directly inside the terminal. Unlike traditional terminal wrappers that tightly couple UI rendering to command-line subprocesses or parse unstructured stdout streams, `ags` implements a strictly decoupled, event-driven architecture powered by the Agent Studios Control Plane.

```
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│                                 Ratatui TUI Front-End                                   │
│            (AgsApp · ViewportContainer · TranscriptWidget · ComposerWidget)             │
└───────────────────────────────────────────┬─────────────────────────────────────────────┘
                                            │
                                            ▼
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│                           agent-studios-cli-client (Crate)                              │
│                      pub trait AgentStudiosClient: Send + Sync                          │
├───────────────────────────────────────────┬─────────────────────────────────────────────┤
│                                           │                                             │
│       EmbeddedClient (In-Process)         │         ConnectedClient (IPC / Wire)        │
│   ┌───────────────────────────────────┐   │   ┌─────────────────────────────────────┐   │
│   │ • In-Memory ControlPlane Engine   │   │   │ • Unix Domain Socket (macOS/Linux)  │   │
│   │ • Single-Writer ControlPlaneActor │   │   │ • Named Pipe (Windows 11 Native)    │   │
│   │ • WorkspaceOrchestrator           │   │   │ • Framed Length-Delimited Protocol  │   │
│   │ • RuntimeSessionFactory           │   │   │ • Remote Session Attacher (Code-OSS)│   │
│   └───────────────────────────────────┘   │   └─────────────────────────────────────┘   │
└───────────────────────────────────────────┴─────────────────────────────────────────────┘
                                            │
                     ┌──────────────────────┴──────────────────────┐
                     ▼                                             ▼
┌───────────────────────────────────────────┐ ┌───────────────────────────────────────────┐
│      crates/control-plane & workspace     │ │          Active Daemon / Code-OSS         │
│     (EventStore, TaskGraph, Worktree)     │ │        Shared Session & Coordinator       │
└───────────────────────────────────────────┘ └───────────────────────────────────────────┘
```

### Core Architectural Invariants
1. **Unidirectional Event Flow**: The TUI is a pure projection of the immutable domain event stream emitted by the Control Plane. Screen state is never derived by screen scraping or mutating internal UI state out-of-band.
2. **Execution Mode Agnosticism**: The entire TUI codebase (`AgsApp`, widgets, modals) interacts solely with the abstract `AgentStudiosClient` trait. It has zero awareness of whether it is running against an embedded in-process engine or connecting over a local IPC socket to an external daemon.
3. **Decoupled Render Loop**: The Ratatui rendering tick (16ms / 60 FPS) never blocks on disk I/O, network RPCs, or Control Plane transactional locks. All Control Plane events pass through a non-blocking asynchronous boundary.
4. **Deterministic Worktree Isolation**: All file-mutating agents operate inside isolated Git worktrees (`.agent-studios/wt/task-XXX`). Diffs are captured using ephemeral Git index structures without altering the working directory index until explicit user reconciliation.
5. **Fail-Closed Security Approvals**: Privileged actions (shell execution, file writes, network calls) are gated by an interactive approval protocol with watchdog timeouts and two-phase commit tokens.

---

## 2. Crate Architecture & Dependencies

The client integration layer is encapsulated in a dedicated crate: `crates/cli-client` (`agent-studios-cli-client`).

### 2.1 Workspace Placement

```text
agent-studios-rs/
 └── crates/
      ├── protocol/               # Canonical domain types (Agent, Task, Worktree, Event)
      ├── control-plane/          # Engine, EventStore, TaskGraph, Replay
      ├── orchestration/          # Read models, Projections, SequenceTracker
      ├── provider/               # Provider definitions, Catalog, Auth, Models
      ├── runtime-session/        # Session factory, PreparedRuntimeSession
      ├── workspace/              # WorktreeManager, capture_changes, reconcile_patch
      ├── external-runtime/       # AgentRuntime trait, process sandboxing, lifecycle
      ├── opencode-runtime/       # OpenCode CLI/server adapter
      ├── claude-code-runtime/    # Claude Code adapter
      ├── cli-client/             # [NEW] agent-studios-cli-client (This Specification)
      └── ags/                    # [NEW] CLI binary entrypoint & Ratatui TUI
```

### 2.2 `Cargo.toml` Specification

```toml
[package]
name = "agent-studios-cli-client"
version = "0.12.0"
edition = "2024"
license = "Apache-2.0"
authors = ["Agent Studios Contributors"]
description = "Typed client library connecting ags CLI to embedded and remote Control Plane instances"

[dependencies]
# Agent Studios Domain Crates
agent-studios-protocol = { path = "../protocol" }
agent-studios-control-plane = { path = "../control-plane" }
agent-studios-orchestration = { path = "../orchestration" }
agent-studios-workspace = { path = "../workspace" }
agent-studios-provider = { path = "../provider" }
agent-studios-runtime-session = { path = "../runtime-session" }
agent-studios-internal-agent = { path = "../internal-agent" }
agent-studios-external-runtime = { path = "../external-runtime" }

# Async & Networking
tokio = { version = "1.43", features = ["full"] }
tokio-util = { version = "0.7", features = ["codec"] }
async-trait = "0.1"
futures = "0.3"

# Inter-Process Communication
interprocess = { version = "2.2", features = ["tokio"] }

# Serialization & Protocols
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
bytes = "1.10"
uuid = { version = "1.13", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }

# Error Handling & Telemetry
thiserror = "2.0"
tracing = "0.1"

[dev-dependencies]
tempfile = "3.17"
```

---

## 3. The `AgentStudiosClient` Abstraction

The `AgentStudiosClient` trait defines the uniform asynchronous contract between the CLI / TUI front-end and the backend execution engine.

### 3.1 Rust Trait Definition

```rust
use std::path::{Path, PathBuf};
use async_trait::async_trait;
use tokio::sync::mpsc;

use agent_studios_protocol::id::{
    AgentId, ApprovalId, ArtifactId, ReconciliationId, RunId, StudioId, TaskId, WorktreeId,
};
use agent_studios_protocol::approval::{ApprovalKind, ApprovalState};
use agent_studios_protocol::event::EventEnvelope;
use agent_studios_protocol::task::TaskRecord;
use agent_studios_protocol::worktree::WorktreeRecord;
use agent_studios_orchestration::read_models::{
    AgentSummary, TaskGraphSnapshot, TaskTimelineProjection,
};
use agent_studios_workspace::capture::CapturedChanges;
use agent_studios_workspace::reconciliation::ReconciliationOutcome;

pub use crate::error::ClientError;
pub use crate::types::*;

#[async_trait]
pub trait AgentStudiosClient: Send + Sync {
    // ------------------------------------------------------------------------
    // Session & Studio Lifecycle
    // ------------------------------------------------------------------------
    /// Returns the active Studio ID managed by this client session.
    fn studio_id(&self) -> StudioId;

    /// Returns the execution mode: Embedded (in-process) or Connected (daemon/IDE).
    fn execution_mode(&self) -> ExecutionMode;

    /// Gracefully closes client connection or shuts down embedded engine.
    async fn shutdown(&self) -> Result<(), ClientError>;

    // ------------------------------------------------------------------------
    // User Prompts & Run Orchestration
    // ------------------------------------------------------------------------
    /// Submits a high-level natural language prompt or task to the Coordinator.
    async fn submit_goal(
        &self,
        prompt: &str,
        options: GoalSubmissionOptions,
    ) -> Result<TaskId, ClientError>;

    /// Requests cancellation of the active run or specific task scope.
    async fn cancel_active_run(
        &self,
        scope: CancellationScopeSpec,
        reason: Option<String>,
    ) -> Result<(), ClientError>;

    // ------------------------------------------------------------------------
    // Event Subscription Pipeline
    // ------------------------------------------------------------------------
    /// Subscribes to real-time Control Plane event envelopes starting from a given sequence.
    /// Returns initial backfilled envelopes plus an active async receiver stream.
    async fn subscribe_events(
        &self,
        from_sequence: u64,
    ) -> Result<ClientSubscription, ClientError>;

    // ------------------------------------------------------------------------
    // Read Model Queries (Projections)
    // ------------------------------------------------------------------------
    /// Retrieves current TaskGraph snapshot including dependencies and states.
    async fn get_task_graph(&self) -> Result<TaskGraphSnapshot, ClientError>;

    /// Retrieves status summary of all active and idle agents in the team.
    async fn get_agent_summaries(&self) -> Result<Vec<AgentSummary>, ClientError>;

    /// Retrieves chronological timeline events for a given task.
    async fn get_task_timeline(&self, task_id: TaskId) -> Result<TaskTimelineProjection, ClientError>;

    // ------------------------------------------------------------------------
    // Security & Approvals
    // ------------------------------------------------------------------------
    /// Resolves an interactive tool execution approval request.
    async fn resolve_approval(
        &self,
        approval_id: ApprovalId,
        decision: ClientApprovalDecision,
    ) -> Result<(), ClientError>;

    // ------------------------------------------------------------------------
    // Worktree & Diff Inspection
    // ------------------------------------------------------------------------
    /// Lists all managed Git worktrees allocated for current Studio tasks.
    async fn list_worktrees(&self) -> Result<Vec<WorktreeRecord>, ClientError>;

    /// Captures the uncommitted diff from a managed worktree against its base commit.
    async fn get_worktree_diff(&self, worktree_id: WorktreeId) -> Result<CapturedChanges, ClientError>;

    /// Reconciles a staged patch artifact into the integration worktree or workspace.
    async fn reconcile_patch(
        &self,
        reconciliation_id: ReconciliationId,
    ) -> Result<ReconciliationOutcome, ClientError>;
}
```

### 3.2 Supporting Data Structures

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionMode {
    /// In-process ControlPlane + Engine running directly inside CLI.
    Embedded,
    /// Client connected via IPC (Unix Domain Socket / Windows Named Pipe) to daemon/IDE.
    Connected,
}

#[derive(Clone, Debug)]
pub struct GoalSubmissionOptions {
    pub assigned_agent: Option<String>,
    pub runtime_override: Option<String>,
    pub provider_override: Option<String>,
    pub model_override: Option<String>,
}

#[derive(Clone, Debug)]
pub enum CancellationScopeSpec {
    Studio,
    Task(TaskId),
    Run(RunId),
    Agent(AgentId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientApprovalDecision {
    AllowOnce,
    Deny,
    AllowForSession,
}

pub struct ClientSubscription {
    pub backfill: Vec<EventEnvelope>,
    pub stream: tokio::sync::mpsc::Receiver<EventEnvelope>,
}
```

---

## 4. Dual Execution Modes

`ags` supports two distinct operational modes designed for maximum flexibility:

### 4.1 Embedded Mode (In-Process Execution)

When launched standalone (`ags [PROMPT]` without an existing daemon), `ags` boots an in-process Control Plane directly within the CLI process memory space.

```
┌────────────────────────────────────────────────────────────────────────┐
│                        ags Process (Embedded)                          │
│                                                                        │
│   ┌─────────────────────┐             ┌────────────────────────────┐   │
│   │   AgsApp (TUI)      │             │ EmbeddedClient             │   │
│   │   • Render Loop     ├────────────►│ • Implements Client Trait  │   │
│   │   • Grapheme Input  │◄────────────┤ • Holds Actor Handle       │   │
│   └─────────────────────┘   UiEvents  └─────────────┬──────────────┘   │
│                                                     │ Commands / Tokio │
│                                                     ▼                  │
│   ┌────────────────────────────────────────────────────────────────┐   │
│   │                 ControlPlaneActor (Single-Writer)              │   │
│   │   • Mutates ControlPlaneState                                  │   │
│   │   • Persists to InMemoryStore                                  │   │
│   │   • Broadcasts committed EventEnvelopes                        │   │
│   └────────────────────────────────┬───────────────────────────────┘   │
│                                    │                                   │
│            ┌───────────────────────┴───────────────────────┐           │
│            ▼                                               ▼           │
│   ┌─────────────────────────────┐         ┌────────────────────────┐   │
│   │ AgentStudiosSupervisor      │         │ WorkspaceOrchestrator  │   │
│   │ • Multi-Agent Team Loop     │         │ • Git WorktreeManager  │   │
│   │ • Runtime Session Overrides │         │ • Ephemeral Index Diff │   │
│   └─────────────────────────────┘         └────────────────────────┘   │
└────────────────────────────────────────────────────────────────────────┘
```

#### Lifecycle & Startup Protocol:
1. **Studio Initialization**: Creates an in-memory `ControlPlane<SystemClock, InMemoryStore>`.
2. **Actor Spawning**: Spawns the single-writer `ControlPlaneActor` onto the Tokio runtime, returning a `ControlPlaneHandle`.
3. **Workspace Initialization**: Allocates a `WorkspaceOrchestrator` pointing to the repository's `.agent-studios/wt` directory.
4. **Studio Record**: Emits `ControlPlaneEvent::StudioCreated` with the active repository path as metadata.
5. **Supervisor Bootstrap**: Instantiates the multi-agent team (Coordinator, Coder, Tester, Reviewer) and runtime session factory.
6. **Zero External Dependencies**: Embedded mode operates without requiring any background daemons, system services, or open network ports.

### 4.2 Connected Mode (Daemon / Code-OSS IDE Inter-Process Bridge)

When an existing Agent Studios daemon or Code-OSS IDE session is active for the repository, `ags` connects to it over local IPC.

```
┌────────────────────────────────┐            ┌────────────────────────────────┐
│          ags Process           │            │    Agent Studios Daemon /      │
│                                │            │     Code-OSS Workbench         │
│  ┌──────────────────────────┐  │    IPC     │  ┌──────────────────────────┐  │
│  │ ConnectedClient          │  │ (UDS /     │  │ IpcServerListener        │  │
│  │ • Framed Transport       ├──┼───Pipe)────┼─►│ • Dispatcher             │  │
│  │ • Heartbeat Monitor      │◄─┼────────────┼──┤ • ControlPlaneHandle     │  │
│  │ • Ingestion Task         │  │            │  │ • WorkspaceOrchestrator  │  │
│  └──────────────────────────┘  │            │  └──────────────────────────┘  │
└────────────────────────────────┘            └────────────────────────────────┘
```

#### Platform Transport Bindings:
- **macOS / Linux**: Unix Domain Socket at `$XDG_RUNTIME_DIR/agent-studios/<repo-hash>.sock` (fallback: `/tmp/agent-studios-<uid>/<repo-hash>.sock`). Permissions are strictly clamped to `0700` (`S_IRWXU`).
- **Windows 11 Native**: Local Named Pipe at `\\.\pipe\agent-studios-<repo-hash>`. Configured with `PipeMode::Byte` and discretionary access control lists (DACL) restricting connection to current logon SID.

#### IPC Wire Protocol Specification:
Communications use a framed length-delimited JSON-RPC 2.0-inspired binary protocol.

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|       Magic: 'A' 'G' 'S' '1'  (0x41 0x47 0x53 0x31)           |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                       Payload Length (u32)                    |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                         Message Type                          |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                    JSON Payload (UTF-8)                       +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

```rust
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IpcMessageType {
    HandshakeRequest = 0x0001,
    HandshakeResponse = 0x0002,
    CommandRequest = 0x0010,
    CommandResponse = 0x0011,
    EventEnvelopePush = 0x0020,
    HeartbeatPing = 0x00F0,
    HeartbeatPong = 0x00F1,
}

#[derive(Serialize, Deserialize)]
pub struct IpcHandshakeRequest {
    pub client_version: String,
    pub repo_root: PathBuf,
    pub session_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct IpcHandshakeResponse {
    pub studio_id: StudioId,
    pub session_id: String,
    pub server_version: String,
    pub latest_sequence: u64,
}
```

---

## 5. Event Subscription Pipeline & TUI Projection Engine

To maintain a fluid 60 FPS user interface during intensive multi-agent execution, the event pipeline isolates the Ratatui render loop from asynchronous ingestion.

### 5.1 Pipeline Topology

```
┌────────────────────────────────────────────────────────────────────────┐
│                   ControlPlaneActor (Single-Writer)                    │
│                                                                        │
│        commits transaction ──► drains envelopes ──► broadcast(4096)   │
└──────────────────────────────────────┬─────────────────────────────────┘
                                       │
                                       ▼ tokio::sync::broadcast
┌────────────────────────────────────────────────────────────────────────┐
│                     Client Ingestion Background Task                   │
│                                                                        │
│   • SequenceTracker::observe(envelope) -> verifies monotonic sequence  │
│   • On sequence gap -> requests resync / replay from EventStore        │
│   • Formats into UiEvent and forwards to bounded channel               │
└──────────────────────────────────────┬─────────────────────────────────┘
                                       │
                                       ▼ tokio::sync::mpsc (bounded: 512)
┌────────────────────────────────────────────────────────────────────────┐
│                     AgsApp Event Consumer & Redraw Tick                │
│                                                                        │
│   tokio::select! {                                                     │
│       Some(ui_event) = ui_rx.recv() => {                               │
│           state.apply_event(&ui_event);                                │
│           request_draw();                                              │
│       }                                                                │
│       Some(crossterm_event) = crossterm_stream.next() => {             │
│           handle_input(crossterm_event);                               │
│           request_draw();                                              │
│       }                                                                │
│       _ = render_interval.tick() => {                                  │
│           if needs_draw { terminal.draw(|f| ui(f, &state)); }          │
│       }                                                                │
│   }                                                                    │
└────────────────────────────────────────────────────────────────────────┘
```

### 5.2 Sequence Tracking & Gap Recovery

The client ingestion task utilizes `SequenceTracker` from `crates/orchestration/src/subscription.rs`:

```rust
use agent_studios_orchestration::subscription::{SequenceError, SequenceTracker};

pub struct EventPipelineWorker {
    tracker: SequenceTracker,
    event_rx: tokio::sync::broadcast::Receiver<EventEnvelope>,
    ui_tx: tokio::sync::mpsc::Sender<UiEvent>,
}

impl EventPipelineWorker {
    pub async fn run(mut self) {
        loop {
            match self.event_rx.recv().await {
                Ok(envelope) => {
                    if let Err(SequenceError::GapOrOutdated { expected, actual }) =
                        self.tracker.observe(&envelope)
                    {
                        tracing::warn!(
                            expected = expected,
                            actual = actual,
                            "Sequence gap detected in event stream. Triggering resync."
                        );
                        // Forward ResyncRequired to TUI to show loading/reconnecting indicator
                        let _ = self.ui_tx.send(UiEvent::ResyncRequired).await;
                        break;
                    }

                    // Map domain envelope to UI projection event
                    let ui_event = UiEvent::from_domain(envelope);
                    if self.ui_tx.send(ui_event).await.is_err() {
                        break; // TUI has shut down
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(missed_count)) => {
                    tracing::error!(
                        missed = missed_count,
                        "Broadcast buffer lagged behind writer. Forcing complete state replay."
                    );
                    let _ = self.ui_tx.send(UiEvent::ResyncRequired).await;
                    break;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    let _ = self.ui_tx.send(UiEvent::ConnectionClosed).await;
                    break;
                }
            }
        }
    }
}
```

### 5.3 UI State Projection

The TUI maintains an in-memory projection model (`AgsUiState`) updated by `UiEvent`:

```rust
pub struct AgsUiState {
    pub header: HeaderState,
    pub agents: AgentActivityBarState,
    pub transcript: TranscriptState,
    pub active_modal: Option<ActiveModal>,
    pub task_graph: TaskGraphSnapshot,
    pub token_counter: TokenMetrics,
    pub needs_redraw: bool,
}

impl AgsUiState {
    pub fn apply_envelope(&mut self, envelope: &EventEnvelope) {
        match &envelope.event {
            ControlPlaneEvent::AgentRegistered { agent } => {
                self.agents.register(agent);
            }
            ControlPlaneEvent::AgentStateChanged { agent_id, new_state, .. } => {
                self.agents.update_state(*agent_id, *new_state);
            }
            ControlPlaneEvent::ToolStarted { agent_id, tool_name, call_id, .. } => {
                self.transcript.add_tool_start(*agent_id, tool_name, call_id);
            }
            ControlPlaneEvent::ToolCompleted { call_id, duration_ms, .. } => {
                self.transcript.mark_tool_completed(call_id, *duration_ms);
            }
            ControlPlaneEvent::ApprovalRequested { approval } => {
                self.active_modal = Some(ActiveModal::Approval(approval.clone().into()));
            }
            ControlPlaneEvent::ApprovalResolved { approval_id, .. } => {
                if let Some(ActiveModal::Approval(req)) = &self.active_modal {
                    if req.approval_id == *approval_id {
                        self.active_modal = None;
                    }
                }
            }
            ControlPlaneEvent::WorktreeChangeCaptured { worktree_id, files_changed, .. } => {
                self.transcript.add_system_notice(format!(
                    "Worktree {worktree_id} captured: {files_changed} files modified"
                ));
            }
            _ => {}
        }
        self.needs_redraw = true;
    }
}
```

---

## 6. Worktree Lifecycle & Workspace Coordination

Multi-agent coordination requires strict isolation of modifying agents from each other and the host repository.

### 6.1 Worktree Directory Topology

All task-specific worktrees are quarantined under `.agent-studios/wt/`:

```text
<repo-root>/
 ├── .git/
 ├── .agent-studios/
 │    ├── config.toml
 │    ├── sessions/
 │    │    └── s_9a8f21.json
 │    └── wt/                               # Managed by WorkspaceOrchestrator
 │         ├── wt_4b1a0e_task_101/          # Coder Agent Worktree
 │         │    └── src/...
 │         └── wt_7c2d9f_task_102/          # Tester Agent Worktree
 │              └── tests/...
 └── src/                                   # Shared Source (Integration Target)
```

### 6.2 Change Capture & Ephemeral Index Invariant

When an agent finishes code generation in a worktree, `ags` coordinates with `crates/workspace/src/capture.rs`:

```rust
// Invariant: The real .git/index in the worktree is NEVER mutated during capture.
// An ephemeral index file in a temporary directory is used for change staging.

let captured = agent_studios_workspace::capture_changes(
    worktree_record.worktree_path(),
    &worktree_record.base_commit,
)?;

// Returns CapturedChanges:
// - patch_bytes: Unified binary git diff
// - files_changed: Count of modified files
// - changed_files: List of file paths
// - stats_summary: Git diffstat output
```

The resulting patch bytes are registered into the `ArtifactStore` as a content-addressed artifact and emitted via `ControlPlaneEvent::WorktreeChangeCaptured`.

### 6.3 Patch Reconciliation Flow

```
1. Coordinator / User triggers patch review: `/diff` or `/patch apply`
2. WorkspaceOrchestrator calls `reconciliation::check_patch(target_worktree, patch_bytes)`
   ├── Clean? -> Ok(None) -> UI shows "Clean Apply Ready"
   └── Conflict? -> Ok(Some(Conflicted { conflicted_files, reason }))
3. On user approval (`[y] Accept Patch` in DiffViewerWidget):
   └── WorkspaceOrchestrator calls `reconciliation::apply_patch(target_worktree, patch_bytes)`
4. Emits `ControlPlaneEvent::ReconciliationApplied`
```

---

## 7. Zero-Interruption Session Mobility (`Ctrl+O`)

Per Product Principle #18, a developer can seamlessly transfer an active session from `ags` to the desktop IDE.

### 7.1 Handoff Mechanics

```text
Developer presses [Ctrl+O] in ags TUI
                   │
                   ▼
1. Client flushes in-memory transaction buffer and commits active event sequence.
2. If in Embedded Mode:
   a. ags starts the background daemon listener on local IPC.
   b. Writes session metadata to `.agent-studios/sessions/last.json`:
      { "session_id": "s_9a8f21", "studio_id": "std_01...", "ipc_endpoint": "\\\\.\\pipe\\..." }
3. ags launches Code-OSS child process:
   code-oss --folder-uri <repo-path> --agent-studios-attach <session_id>
4. ags displays status bar notification: "Handoff complete. Code-OSS attached to session."
5. ags transitions to Connected Mode attached to its own background daemon.
```

---

## 8. Error Handling & Invariant Enforcement

| Component | Invariant | Error Handling Strategy |
| :--- | :--- | :--- |
| **ControlPlaneActor** | Single-writer serialized transactions | If actor drops or panics, client receives `ClientError::ControlPlaneUnavailable`. |
| **Event Stream** | Strictly monotonic sequences | Gap detection triggers immediate historical replay via `store.events_for_studio()`. |
| **Worktree** | Containment inside `.agent-studios/wt` | `WorkspaceOrchestrator::validate_worktree_path` rejects any path traversal escaping root. |
| **Approval** | Fail-closed security gate | Watchdog timeout (60s) or disconnect automatically evaluates to `ApprovalDecision::Deny`. |
| **Terminal TUI** | Pristine terminal restoration | Panic hook and `Drop` implementation guarantee alternate screen exit and cursor show. |
