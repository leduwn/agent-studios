# M13 ConnectedClient & IPC Interoperability Architectural Specification

- **Milestone**: `M13 IPC-01 / M13-CLI-01`
- **Component**: `crates/cli-client` (`ConnectedClient`), `crates/ipc-protocol`, `crates/app-server` (`agsd`)
- **Status**: **APPROVED ARCHITECTURAL SPECIFICATION**
- **Date**: `2026-10-29`
- **Target Interface**: `AgentStudiosClient` trait (`crates/cli-client/src/client.rs`)

---

## 1. Executive Summary & Core Design Principles

Agent Studios CLI operates under a decoupled architecture where all UI surfaces (interactive TUI, headless scripts, and CI runners) interface exclusively with the `AgentStudiosClient` trait. 

```
                                    +-----------------------------------------------+
                                    |              CLI UI / TUI / REPL              |
                                    +-----------------------------------------------+
                                                           |
                                                DynAgentStudiosClient
                                                           |
                               +---------------------------+---------------------------+
                               |                                                       |
                               v                                                       v
                 +---------------------------+                           +---------------------------+
                 |      EmbeddedClient       |                           |      ConnectedClient      |
                 |   (In-Process Control)    |                           |   (Remote IPC Client)     |
                 +---------------------------+                           +---------------------------+
                               |                                                       |
                               | (Direct In-Memory)                        IpcEnvelope<IpcCommand>
                               v                                           Length-Prefixed Framing
                 +---------------------------+                                         |
                 |     ControlPlane /        |                                         v
                 |     InMemoryStore         |                           +---------------------------+
                 +---------------------------+                           |    Local Named Pipe /     |
                                                                         |    Unix Domain Socket     |
                                                                         +---------------------------+
                                                                                       |
                                                                                       v
                                                                         +---------------------------+
                                                                         |        agsd Daemon        |
                                                                         |    (crates/app-server)    |
                                                                         +---------------------------+
```

### 1.1. Core Architectural Tenets

1. **Zero-Drift Parity**:
   The interactive TUI, command handlers, and state projections must function identically regardless of whether backed by `EmbeddedClient` or `ConnectedClient`. No TUI code may inspect execution mode for operational branching beyond telemetry labeling.
2. **Fail-Closed Security**:
   Local IPC connections must strictly enforce OS-level identity validation (Windows DACLs, Unix UID permission mode `0600`), HMAC-SHA256 frame integrity signatures, and timestamp skew/nonce replay rejection. Unauthorized or malformed frames drop the transport connection immediately.
3. **Linearizable Continuity & Gap Recovery**:
   Event streaming over IPC must be strictly ordered. Disconnections and buffer lag must be recovered automatically via monotonic sequence watermarks (`from_sequence = last_seen + 1`) querying the server's `ReplayBuffer`.
4. **Multiplexed Multi-Surface Coexistence**:
   The daemon (`agsd`) supports multiple concurrent local clients (e.g., CLI TUI and Code-OSS VSCode Extension) operating on the same studio workspace with synchronized task graphs and collaborative approval resolution.

---

## 2. `AgentStudiosClient` Remote Protocol Mapping

`ConnectedClient` implements `AgentStudiosClient` by serializing method calls into `IpcEnvelope<IpcCommand>` frames, transmitting them across the local authenticated transport, and awaiting typed `IpcResponse` envelopes or managing asynchronous streaming tasks.

### 2.1. Trait Method Matrix & IPC Translation

| `AgentStudiosClient` Method | Remote IPC Command | Expected Response / Stream | Failure Semantics |
|---|---|---|---|
| `studio_id(&self) -> StudioId` | Cached locally from Handshake | Immediate return (`StudioId`) | Infallible |
| `workspace_name(&self) -> &str` | Cached locally from Handshake | Immediate return (`&str`) | Infallible |
| `execution_mode(&self) -> ExecutionMode` | Local constant (`ExecutionMode::Connected`) | Immediate return | Infallible |
| `runtime_mode(&self) -> RuntimeMode` | Cached or queried via daemon status | Immediate return (`RuntimeMode`) | Infallible |
| `get_workspace_info(&self)` | `IpcCommand::Custom { action: "get_workspace_info", .. }` | `IpcResponse::Success { result: WorkspaceInfo }` | Falls back to local git inspect if daemon offline |
| `shutdown(&self)` | Closes IPC transport connection actor | Disconnects pipe/socket cleanly | Does not terminate daemon process |
| `submit_goal(&self, prompt, options)` | `IpcCommand::SubmitGoal { goal, access_mode, workspace_root, options }` | `IpcResponse::Success { result: { "task_id": "..." } }` | Maps `IpcErrorResponse` to `ClientError` |
| `cancel_active_run(&self, reason)` | `IpcCommand::CancelTask { task_id, reason }` | `IpcResponse::Success { .. }` | Maps `NotFound` or `Conflict` |
| `subscribe_events(&self, from_sequence)` | `IpcCommand::SubscribeEvents { from_sequence }` | `ClientSubscription` backed by mpsc stream & watch sync | Automatic ReplayBuffer backfill |
| `get_task_graph(&self)` | `IpcCommand::Custom { action: "get_task_graph", .. }` | `IpcResponse::Success { result: TaskGraphSnapshot }` | Serialized projection snapshot |
| `get_agent_summaries(&self)` | `IpcCommand::Custom { action: "get_agent_summaries", .. }` | `IpcResponse::Success { result: Vec<AgentSummary> }` | Projected agent roster state |
| `get_task_timeline(&self, task_id)` | `IpcCommand::Custom { action: "get_task_timeline", .. }` | `IpcResponse::Success { result: TaskTimelineProjection }` | Returns `ClientError::NotFound` if invalid |
| `resolve_approval(&self, id, decision)` | `IpcCommand::ResolveApproval { approval_id, decision, details }` | `IpcResponse::Success { .. }` | Fails closed on timeout or expiry |
| `list_worktrees(&self)` | `IpcCommand::Custom { action: "list_worktrees", .. }` | `IpcResponse::Success { result: Vec<WorktreeRecordSummary> }` | Daemon queries WorkspaceOrchestrator |
| `get_worktree_diff(&self, wt_id)` | `IpcCommand::Custom { action: "get_worktree_diff", .. }` | `IpcResponse::Success { result: CapturedChanges }` | Supported via daemon (Embedded is stubbed) |
| `reconcile_patch(&self, rec_id)` | `IpcCommand::Custom { action: "reconcile_patch", .. }` | `IpcResponse::Success { result: ReconciliationOutcome }` | Supported via daemon (Embedded is stubbed) |

### 2.2. Detailed Protocol Framing Specifications

#### 2.2.1. `submit_goal`
```json
// Request Frame
{
  "version": { "major": 1, "minor": 0, "patch": 0 },
  "id": "req_01JG5G0X...",
  "timestamp": "2026-10-29T12:00:00.000Z",
  "payload": {
    "type": "submit_goal",
    "payload": {
      "goal": "Refactor token authentication parser",
      "access_mode": "Mutating",
      "workspace_root": "D:/Projects/agent-studios",
      "options": {
        "provider": "anthropic",
        "model": "claude-3-7-sonnet-20250219",
        "runtime": "claude-code",
        "auto_approve_policy": "on-miss",
        "workspace_access_mode": "Mutating"
      }
    }
  },
  "auth": {
    "key_id": "key_cli_duwn",
    "nonce": "nonce_7f8a9b",
    "timestamp_millis": 1793275200000,
    "signature": "3a4f89..."
  }
}

// Server Response Frame
{
  "version": { "major": 1, "minor": 0, "patch": 0 },
  "id": "req_01JG5G0Y...",
  "correlation_id": "req_01JG5G0X...",
  "timestamp": "2026-10-29T12:00:00.045Z",
  "payload": {
    "status": "success",
    "result": {
      "task_id": "task_01JG5G10ABCDEF1234567890"
    }
  }
}
```

#### 2.2.2. `subscribe_events` & Streaming Channel Architecture
The subscription maps directly to length-prefixed streaming envelopes containing `IpcEvent` payloads.

```
       Client Request: IpcCommand::SubscribeEvents { from_sequence: 142 }
                                      |
                                      v
  [Server: ReplayBuffer::replay_from(142)] -> Delivers Backfill Vector [142..150]
                                      |
                                      v
  [Server: Live Broadcast Channel]       -> Delivers Stream Envelopes [151, 152, ...]
                                      |
                                      v
+-----------------------------------------------------------------------------------+
|                            ConnectedClient Subscription Loop                      |
|                                                                                   |
|  1. Ingests raw framed byte stream from pipe/socket.                              |
|  2. Deserializes into IpcEnvelope<IpcEvent>.                                      |
|  3. Validates sequence continuity: candidate.sequence == last_seen + 1.          |
|  4. Injects into tokio::sync::mpsc::channel(512).                                 |
|  5. Updates watch::Sender<StreamSyncState> to StreamSyncState::Healthy.           |
+-----------------------------------------------------------------------------------+
```

#### 2.2.3. Error Taxonomy Mapping (`IpcErrorResponse` -> `ClientError`)
```rust
impl From<IpcErrorResponse> for ClientError {
    fn from(err: IpcErrorResponse) -> Self {
        match err.code {
            ErrorCode::InvalidRequest => ClientError::OperationFailed(err.message),
            ErrorCode::Unauthorized | ErrorCode::Forbidden => {
                ClientError::PermissionDenied(err.message)
            }
            ErrorCode::NotFound => ClientError::NotFound(err.message),
            ErrorCode::VersionMismatch => ClientError::Configuration(format!(
                "Protocol version mismatch: {}",
                err.message
            )),
            ErrorCode::CapabilityMissing => ClientError::Unsupported(err.message),
            ErrorCode::Timeout | ErrorCode::ApprovalExpired => ClientError::Timeout,
            ErrorCode::InternalError | ErrorCode::Conflict => {
                ClientError::Internal(err.message)
            }
        }
    }
}
```

---

## 3. ConnectedClient Lifecycle & Connection State Machine

### 3.1. Connection State Machine

```
      +------------------------+
      |      Disconnected      |
      +------------------------+
                  |
                  | [Init / Retry Trigger]
                  v
      +------------------------+
      |  DiscoveringEndpoint   | <----+ (Read %LOCALAPPDATA% / XDG manifest)
      +------------------------+      |
                  |                   |
                  | [Found Socket]    |
                  v                   |
      +------------------------+      |
      |  ConnectingTransport   |      |
      +------------------------+      |
                  |                   |
                  | [Transport Open]  |
                  v                   |
      +------------------------+      |
      |     Authenticating     |      | (HMAC Challenge / Nonce verification)
      +------------------------+      |
                  |                   |
                  | [Signature OK]    |
                  v                   |
      +------------------------+      |
      | NegotiatingCapabilities|      | (SemVer + Required/Optional caps)
      +------------------------+      |
                  |                   |
                  | [Caps Agreed]     |
                  v                   |
      +------------------------+      |
      |       Connected        |      |
      +------------------------+      |
                  |                   |
                  | [Pipe Drop / Err] |
                  v                   |
      +------------------------+      |
      |      Reconnecting      | -----+ (Backoff: 100ms, 200ms, 500ms, 1s..5s)
      +------------------------+
                  |
                  | [Explicit Shutdown / Max Retries Exceeded]
                  v
      +------------------------+
      |       Terminated       |
      +------------------------+
```

### 3.2. Endpoint Discovery Mechanism

The client determines the active daemon IPC rendezvous point using deterministic platform conventions and discovery manifests.

#### 3.2.1. Discovery Manifest (`daemon.json`)
The daemon writes a restricted permission discovery file upon binding:
- **Windows**: `%LOCALAPPDATA%\agent-studios\daemon.json`
- **Linux / macOS**: `$XDG_RUNTIME_DIR/agent-studios/daemon.json` (fallback: `~/.agent-studios/daemon.json`)

```json
{
  "version": "1.0.0",
  "pid": 48210,
  "endpoint": "\\\\.\\pipe\\agent-studios-duwn-default",
  "auth_key_id": "key_local_daemon",
  "auth_secret_path": "C:\\Users\\Duwn\\AppData\\Local\\agent-studios\\.secret",
  "started_at": "2026-10-29T08:15:22.100Z"
}
```

#### 3.2.2. Platform Endpoint Paths
1. **Windows Platform**:
   - Pipe Path: `\\.\pipe\agent-studios-<username>-<studio_or_session>`
   - Security: Windows Security Descriptor (DACL) granting full control solely to current user SID (`WinLocalSystemSid` / current user token), denying network access.
2. **Linux & macOS**:
   - Socket Path: `$XDG_RUNTIME_DIR/agent-studios/agsd.sock`
   - Permissions: `0600` (read/write user only). Parent directory set to `0700`.

### 3.3. Authentication Handshake & Capability Negotiation

```sequence
Client                          Transport                         agsd Daemon
  |                                 |                                  |
  |-------- Connect Pipe ---------->|                                  |
  |                                 |-------- Accept Connection ------>|
  |                                 |<------- Send Auth Challenge -----|
  |                                 |   (ServerNonce, KeyId, SkewMax)  |
  |<------ Auth Challenge ----------|                                  |
  |                                                                    |
  | 1. Read secret from .secret                                        |
  | 2. Compute canonical digest                                        |
  | 3. HmacSigner::sign(...)                                           |
  |                                                                    |
  |------ CapabilityNegotiationRequest (with HMAC Header) ------------>|
  |       - client_id: "ags-cli-4912"                                  |
  |       - supported_versions: [1.0.0]                                |
  |       - required_capabilities: [EventStreaming, ApprovalRouting]  |
  |       - optional_capabilities: [EventReplay, DiffInspection]       |
  |                                                                    |
  |                                 | 4. Verify HMAC & Nonce           |
  |                                 | 5. ServerCapabilityNegotiator    |
  |                                 |                                  |
  |<----- CapabilityNegotiationResponse -------------------------------|
  |       - selected_version: 1.0.0                                    |
  |       - agreed_capabilities: [EventStreaming, ApprovalRouting, ..] |
  |       - max_payload_bytes: 16777216                                |
  |                                                                    |
  | [Connection Established -> State::Connected]                       |
```

### 3.4. Resilient Reconnection & Watermark Recovery

When the transport drops unexpectedly (daemon restart, transient pipe stall):
1. Client enters `Reconnecting` state; marks `StreamSyncState::Recovering`.
2. Connects to discovery endpoint using exponential backoff with jitter (`100ms * 2^attempt`, max 5s).
3. Re-authenticates and negotiates capabilities.
4. Immediately sends:
   ```rust
   IpcCommand::SubscribeEvents {
       from_sequence: Some(last_seen_sequence.load(Ordering::SeqCst) + 1),
   }
   ```
5. Daemon queries internal `ReplayBuffer`:
   - **Case A: Watermark Available**: Daemon streams backfill events starting exactly at `last_seen + 1`. Client catches up, restores `StreamSyncState::Healthy`. Zero missed frames.
   - **Case B: Unrecoverable Gap (`SequenceGapUnrecoverable`)**: Daemon's circular ring buffer evicted the requested watermark. Server returns `ProtocolError::SequenceGapUnrecoverable { expected, found }`.
     Client transitions to `StreamSyncState::GapUnrecoverable { expected, found }`.
     `ConnectedClient` triggers authoritative state resynchronization:
     - Calls `get_task_graph()` to replace corrupted task tree projection.
     - Calls `get_agent_summaries()` to restore team read models.
     - Subscribes with `from_sequence: Some(found)` to resume live tracking.

---

## 4. Approval Routing & Interactive Terminal Integration

In Connected mode, agent actions requiring user consent (arbitrary shell commands, file modifications, network access) originate from the daemon's runtime engine and must be routed across the IPC boundary to the CLI's interactive TUI modal.

```
       Daemon Runtime Engine                   ConnectedClient                    Interactive TUI
                 |                                    |                                  |
    [Approval Contract Created]                       |                                  |
    - approval_id: "appr_99"                          |                                  |
    - risk_level: High                                |                                  |
    - action: CommandExecution("rm -rf target")       |                                  |
                 |                                    |                                  |
                 |--- IpcEvent::ApprovalRequested --->|                                  |
                 |    (Contract, Timeout=60s)         |                                  |
                 |                                    |--- Emit on Subscription -------->|
                 |                                    |                                  |
                 |                                    |                     [Render Security Modal]
                 |                                    |                     - Display Risk Badge
                 |                                    |                     - Display Command / Diff
                 |                                    |                     - Start Countdown Timer
                 |                                    |                                  |
                 |                                    |                     [User Presses 'Allow']
                 |                                    |<-- resolve_approval(Allow) ------|
                 |<-- IpcCommand::ResolveApproval ----|
                 |    - approval_id: "appr_99"        |
                 |    - decision: "Approved"          |
                 |                                    |
    [Authoritative Validation]                        |
    1. Verify approval status == Pending              |
    2. Verify Utc::now() <= expires_at                |
    3. Transition ApprovalState::Approved             |
    4. Unblock Runtime Execution Task                 |
    5. Emit IpcEvent::ApprovalResolved                |
                 |                                    |
                 |--- IpcEvent::ApprovalResolved ---->|--------------------------------->|
                 |                                    |                     [Dismiss Modal]
```

### 4.1. Contract Definition & Data Model
The approval interaction uses the strongly-typed `ApprovalContract` from `crates/ipc-protocol/src/approval.rs`:

```rust
pub struct ApprovalContract {
    pub approval_id: ApprovalId,
    pub session_id: SessionId,
    pub task_id: String,
    pub run_id: String,
    pub risk_level: RiskLevel,
    pub action: ApprovalAction,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub status: ApprovalStatus,
}
```

### 4.2. Multi-Client Approval Arbitration
When multiple clients are connected to `agsd` (e.g. CLI TUI and VSCode extension):
1. **First-Response Wins**: The first valid `IpcCommand::ResolveApproval` submitted transitions the server contract from `Pending` to `Approved` or `Rejected`.
2. **Subsequent Conflicts Rejected**: Any slower response from another client is rejected with `ErrorCode::Conflict` ("Approval already resolved").
3. **Broadcast Cancellation**: The server immediately broadcasts `IpcEvent::ApprovalResolved` to all subscribed clients, causing all UI surfaces to dismiss the prompt cleanly.
4. **Fail-Closed Expiry**: If `expires_at` is reached without resolution, the daemon transitions status to `TimedOut`, halts the runtime step, and notifies all clients.

---

## 5. Daemon Ownership & Lifecycle Management

The `agsd` daemon operates as an autonomous, user-scoped background process managing long-running agent workflows, isolated git worktrees, and external runtime containers.

```
                           +------------------------------------+
                           |             User CLI               |
                           +------------------------------------+
                                      |
                     +----------------+----------------+
                     |                                 |
              [ags start]                         [ags --prompt]
                     |                                 |
                     v                                 v
         +------------------------+       +------------------------+
         | Spawn Detached Daemon  |       | Check daemon.json      |
         +------------------------+       +------------------------+
                     |                                 |
                     v                                 +--- [Running] ---> Connect IPC
         +------------------------+                    |
         | agsd Process Active    |                    +--- [Not Running]
         | - Binds Named Pipe     |                               |
         | - Writes daemon.json   |                               v
         | - Sets 30m Idle Timer  |                    +------------------------+
         +------------------------+                    | Autospawn Enabled?     |
                     |                                 +------------------------+
         +-----------+-----------+                                |
         |                       |                     +----------+----------+
         v                       v                     |                     |
   [Client Count > 0]    [Client Count == 0]       [Yes]                   [No]
         |                       |                     |                     |
   Reset Idle Timer      Countdown 30m Idle            v                     v
                                 |             Spawn Daemon &        Fallback to
                                 v             Connect IPC           EmbeddedClient
                         [Shutdown & Unlink]
```

### 5.1. CLI Lifecycle Commands
- `ags daemon start`: Explicitly launches `agsd` as a detached daemon process.
- `ags daemon status`: Queries daemon health, PID, active client count, and managed worktree allocations.
- `ags daemon stop`: Transmits administrative termination signal, cleanly unbinds pipes, checkpoints event logs, and terminates the daemon.

### 5.2. On-Demand Autospawn Flow
When an interactive user runs `ags` without an active daemon:
1. `ConnectedClient::connect_or_autospawn()` checks for active pipe and valid `daemon.json`.
2. If unreachable and `daemon.autospawn = true` (default in configuration):
   - Spawns `agsd --daemon` as a detached process without parent process group inheritance.
   - Polls for `daemon.json` creation with a 3.0-second timeout.
   - Connects and executes authentication handshake.
3. If autospawn fails or is disabled via `--no-daemon` flag, client cleanly activates fallback logic.

### 5.3. Idle Timeout & Cleanup Policy
- The daemon maintains a client reference counter.
- When connected client count drops to zero and no active agent runs are executing, an idle timer starts (default: `1800s` / 30 minutes).
- If no client connects before timer expiration:
  1. Daemon invokes `WorkspaceOrchestrator::cleanup_idle_worktrees()`.
  2. Flushes and commits event store history to disk.
  3. Deletes `daemon.json` manifest.
  4. Removes pipe/socket endpoint.
  5. Process exits with code 0.

---

## 6. Seamless Fallback Behavior

To guarantee uninterrupted developer workflows during offline environments or restricted system permissions, the CLI implements deterministic fallback orchestration.

```rust
pub async fn resolve_client(
    workspace_name: &str,
    force_embedded: bool,
    runtime_mode: RuntimeMode,
) -> Result<DynAgentStudiosClient, ClientError> {
    if force_embedded {
        tracing::info!("Offline or embedded mode explicitly forced; bootstrapping EmbeddedClient");
        let client = EmbeddedClient::bootstrap_with_mode(workspace_name, runtime_mode).await?;
        return Ok(Arc::new(client));
    }

    match ConnectedClient::connect(workspace_name).await {
        Ok(connected) => {
            tracing::info!("Successfully connected to active agsd daemon over IPC");
            Ok(Arc::new(connected))
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                "Unable to connect to agsd daemon; falling back seamlessly to in-process EmbeddedClient"
            );
            let fallback = EmbeddedClient::bootstrap_with_mode(workspace_name, runtime_mode).await?;
            Ok(Arc::new(fallback))
        }
    }
}
```

### 6.1. Standalone vs Connected Parity Matrix

| Feature / Subsystem | In-Process `EmbeddedClient` | Daemon `ConnectedClient` |
|---|---|---|
| **Goal Execution** | Supported (Runs in CLI process) | Supported (Runs in daemon background) |
| **Session Persistence** | Terminated when CLI exits | Persists across CLI restarts / disconnects |
| **Multi-Agent Task Graph** | In-memory projection | Durable multi-client projection |
| **Security Approvals** | Direct in-memory channel | IPC-routed `ApprovalContract` |
| **Worktree Isolation** | Local orchestrator instance | Centrally managed worktree pool |
| **Worktree Diff Inspection** | Stubbed (`Unsupported`) | Full patch & diff generation |
| **Patch Reconciliation** | Stubbed (`Unsupported`) | Full 3-way merge & conflict resolution |
| **Secondary UI / IDE Client** | Not possible (single process) | Full real-time synchronization |

---

## 7. Future Code-OSS (VSCode Extension) Integration Surface

The `agsd` daemon serves as the central orchestration bus for both the terminal CLI and the Code-OSS / VSCode desktop IDE extension (`agent-studios-vscode`).

```
+-----------------------------------------------------------------------------------+
|                                 Developer Machine                                 |
|                                                                                   |
|   +--------------------------+                     +--------------------------+   |
|   |   Agent Studios CLI      |                     |    VSCode Extension      |   |
|   |   (crates/cli-client)    |                     | (agent-studios-vscode)   |   |
|   +--------------------------+                     +--------------------------+   |
|                 |                                                |                |
|        IpcEnvelope (HMAC)                               IpcEnvelope (HMAC)        |
|                 |                                                |                |
|                 +-----------------------+------------------------+                |
|                                         |                                         |
|                                         v                                         |
|                    +------------------------------------------+                   |
|                    | Local Transport: Named Pipe / Unix Socket|                   |
|                    +------------------------------------------+                   |
|                                         |                                         |
|                                         v                                         |
|                    +------------------------------------------+                   |
|                    |           agsd Daemon Core               |                   |
|                    |                                          |                   |
|                    |  - Multiplexed Client Connection Pool    |                   |
|                    |  - Shared EventStore & ReplayBuffer      |                   |
|                    |  - Authoritative ControlPlane Engine     |                   |
|                    |  - Multi-Client Approval Coordinator     |                   |
|                    |  - Workspace & Worktree Orchestrator     |                   |
|                    +------------------------------------------+                   |
+-----------------------------------------------------------------------------------+
```

### 7.1. Shared State & Cross-Surface Interaction Patterns

1. **Simultaneous Task Monitoring**:
   A prompt submitted via CLI (`ags "Build auth endpoints"`) immediately streams progress events into VSCode. The extension's Task Graph webview renders new nodes without manual page refresh.
2. **Cross-Surface Approval Resolution**:
   When an agent requests execution of a sensitive command, both the CLI terminal prompt and a VSCode native editor modal appear simultaneously. Resolving in either interface unblocks the agent and instantly updates the sibling interface.
3. **Editor File Synchronization & Diffs**:
   The VSCode extension queries `get_worktree_diff(worktree_id)` to display side-by-side git diffs of uncommitted agent edits directly within the VSCode diff editor before reconciliation.
4. **Reconciliation Triggering**:
   Developers can review changes in VSCode diff view, then invoke `reconcile_patch` via CLI or VSCode "Apply Patch" button, unifying CLI and IDE workflows into a single coherent system.

---

## 8. Verification Strategy & Acceptance Criteria

Implementation of `ConnectedClient` in `crates/cli-client` must pass the following verifiable criteria:

1. **Protocol Compliance**: 100% test coverage against `crates/ipc-protocol` test suites (envelopes, HMAC signing, replay buffer recovery).
2. **Parity Integration Tests**: Execute existing test suites (`tests/worktree_isolation_tests.rs`, `tests/subscription_tests.rs`, `tests/client_tests.rs`) parameterized over both `EmbeddedClient` and `ConnectedClient` with identical behavioral assertions.
3. **Resilience Benchmark**: Inject 10 consecutive connection kills during active goal execution; client must reconnect within 200ms and reconstruct event stream with 0 dropped sequence frames.
4. **Multi-Client Verification**: Concurrently connect two `ConnectedClient` instances, trigger goal submission from Client A, verify Client B receives complete linear event sequence and identical final `TaskGraphSnapshot`.
