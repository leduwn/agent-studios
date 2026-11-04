# M15 UI-04 — Report 07: Backend Handoff Issues & Protocol Contract Gaps

## Metadata
- **Milestone**: M15 UI-04 — System Integration, Contract Audit & Robust Lifecycle Recovery
- **Component**: Backend Daemon Handoff & Protocol Discrepancy Review
- **Source UI Branch**: PR #27 (`feat/m15-real-agent-execution-ui` @ `054dc0328f608ec39b3602fcad39e0839c4fce86`)
- **Pinned Backend Baseline**: PR #29 (`feat/m15-03-artifact-diff-review` @ `cc1c3e8f398a5acfa24cf643cfcb2b043f6e163b`)
- **Target Backend Handoff**: PR #30 (`feat/m15-04-durable-state-recovery` @ `dcbcb39bf61225a4660be5b533cf1ffd5de14b29` by Claude 1)
- **Date**: 2026-11-02
- **Auditor**: Agent A (Dependency & Contract Auditor)

---

## 1. Executive Summary

This report identifies contract mismatches, behavioral edge cases, serialization anomalies, and version discrepancies between the M15 UI client implementation (`agent-studios-ide-ui`) and the M15-04 durable state recovery backend (`agent-studios-rs` PR #30 by Claude 1).

While the UI is 100% compliant with the pinned M15-03 backend (`cc1c3e8f`), transitioning to M15-04 introduces persistence, crash recovery, and restart reconciliation dynamics that expose critical edge cases in event streaming, connection handling, and diagnostics.

---

## 2. Identified Contract Mismatches & Technical Issues

### Issue 1: Missing M15-04 Recovery Diagnostics Wire Types in UI
- **Severity**: Medium (Feature Gap / Non-breaking for M15-03)
- **Affected Components**:
  - Rust: `crates/ipc-transport/src/connection.rs:160, 260, 1753`, `crates/ipc-protocol/src/framing.rs:96, 105`
  - TypeScript: `apps/agent-studios-vscode/src/bridge/ipc/RustIpcProtocolTypes.ts`
- **Root Cause**:
  PR #30 (`dcbcb39b`) added a new read-only diagnostics command and response variant:
  ```rust
  // IpcCommand variant
  GetRecoveryStatus,

  // IpcResponse variant
  RecoveryStatus {
      status: agent_studios_ipc_protocol::RecoveryStatusReport,
  }
  ```
  Wire Command JSON:
  ```json
  {
    "type": "get_recovery_status"
  }
  ```
  Wire Response JSON:
  ```json
  {
    "type": "recovery_status",
    "status": {
      "last_recovery_at": "2026-11-02T09:36:29Z",
      "interrupted_tasks_count": 1,
      "interrupted_runs_count": 1,
      "worktrees_reconciled": 0,
      "worktrees_missing": 0,
      "worktrees_foreign": 0,
      "database_healthy": true,
      "database_path": null,
      "total_persisted_events": 42
    }
  }
  ```
  The TypeScript file `RustIpcProtocolTypes.ts` does not yet declare `{ type: 'get_recovery_status' }` in `RustIpcCommand` nor `{ type: 'recovery_status', status: RustRecoveryStatusReport }` in `RustIpcResponse`.
- **Handoff Remediation**:
  1. Add `RustRecoveryStatusReport` interface to `RustIpcProtocolTypes.ts`.
  2. Extend `RustIpcCommand` and `RustIpcResponse` union types.
  3. Expose `getRecoveryStatus()` on `ExecutionClient` and `DaemonConnectedBridge`.

---

### Issue 2: Replay Buffer Sequence Gap Trap on Daemon Restart / Crash Recovery
- **Severity**: High (Critical Reconnect Failure)
- **Affected Components**:
  - Rust: `crates/ipc-transport/src/server.rs:444-500`, `crates/ipc-protocol/src/replay.rs:115-128`, `crates/ipc-transport/src/connection.rs:526-532`
  - TypeScript: `apps/agent-studios-vscode/src/bridge/ipc/NativeIpcTransport.ts:184-197`
- **Root Cause**:
  1. In PR #30 (`server.rs:444`), when `agsd` restores state from `SqliteEventStore`, `ControlPlane` loads historical events into memory.
  2. However, the IPC server's `ReplayBuffer` is newly constructed:
     ```rust
     let replay_buffer = ReplayBuffer::new(self.replay_capacity.max(1))?;
     ```
     Its internal buffer is empty (`last_sequence: None`).
  3. When an existing UI client reconnects after daemon crash/restart, `NativeIpcTransport` issues `SubscribeEvents` using its last seen watermark:
     ```typescript
     const seq = fromSequence ?? (this.lastReceivedSequence > 0 ? this.lastReceivedSequence + 1 : undefined);
     // e.g. from_sequence: 45
     ```
  4. In `crates/ipc-protocol/src/replay.rs:120`:
     ```rust
     if self.buffer.is_empty() {
         if from_sequence > 1 {
             return Err(ProtocolError::SequenceGapRecoverable {
                 expected: 1,
                 found: from_sequence,
             });
         }
         return Ok(Vec::new());
     }
     ```
  5. The daemon catches this error and returns `{ "type": "error", "message": "Failed to replay events: Sequence gap (recoverable): expected 1, found 45" }`.
  6. The event streaming task is **aborted**, leaving the UI client without any live event updates!
- **Handoff Remediation**:
  - **Backend (Claude 1)**: During `IpcServerBuilder::build` with a SQLite store, populate the `ReplayBuffer` with the latest N committed events from the store so `replay_from` succeeds across daemon restarts.
  - **UI (PR #27)**: In `NativeIpcTransport`, detect session changes (`authResp.session_id !== previousSessionId`) or `SequenceGapRecoverable` errors, and fallback to `subscribeEvents(0)` followed by a full state hydration query (`initializeFromBridge()`).

---

### Issue 3: Strict `define_id!` Deserialization Causes Silent Socket Drops
- **Severity**: High (Silent Disconnection on Malformed ID)
- **Affected Components**:
  - Rust: `crates/protocol/src/id.rs:8-75`, `crates/ipc-transport/src/framing.rs:64`, `crates/ipc-transport/src/connection.rs:350-368`
  - TypeScript: `apps/agent-studios-vscode/src/bridge/ipc/RustIpcProtocolTypes.ts`
- **Root Cause**:
  1. Rust IDs (`StudioId`, `TaskId`, `RunId`, `ArtifactId`) are defined with:
     ```rust
     #[derive(Serialize, Deserialize)]
     #[serde(transparent)]
     pub struct TaskId(Uuid);
     ```
  2. If the UI passes a non-UUID string (such as mock IDs `'studio-m15-001'` or `'task-1'`), `serde_json::from_slice` returns `Err(TransportError::Serialization(...))`.
  3. In `crates/ipc-transport/src/connection.rs:364`:
     ```rust
     Err(e) => {
         debug!("Transport error reading command: {:?}", e);
         return Err(e);
     }
     ```
  4. The daemon terminates the client connection immediately without sending an `IpcResponse::Error` frame.
  5. The UI client only observes a raw socket `close` event with error `'IPC socket closed unexpectedly.'`, making debugging difficult.
- **Handoff Remediation**:
  - **Backend (Claude 1)**: Where possible, capture JSON deserialization errors at the framing boundary and send an explicit `IpcResponse::Error { message: "Invalid command syntax: ..." }` before dropping the connection.
  - **UI (PR #27)**: Enforce UUID v4 regex validation (`^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$`) in `ExecutionClient` and `ArtifactClient` before wire encoding.

---

### Issue 4: Dual `IpcResponse` Framing Definition Ambiguity
- **Severity**: Low (Documentation & Architecture Alignment)
- **Affected Components**:
  - `crates/ipc-protocol/src/framing.rs:142-152`
  - `crates/ipc-transport/src/connection.rs:168-266`
- **Root Cause**:
  There are two conflicting `IpcResponse` enums in the Rust codebase:
  1. `crates/ipc-protocol/src/framing.rs`:
     ```rust
     #[derive(Serialize, Deserialize)]
     #[serde(rename_all = "snake_case", tag = "status")]
     pub enum IpcResponse {
         Success { result: serde_json::Value },
         Error { #[serde(flatten)] error: IpcErrorResponse },
     }
     ```
  2. `crates/ipc-transport/src/connection.rs`:
     ```rust
     #[derive(Serialize, Deserialize)]
     #[serde(rename_all = "snake_case", tag = "type")]
     pub enum IpcResponse {
         CapabilitiesNegotiated { ... },
         TasksSubmitted { ... },
         TaskDiff { ... },
         Error { message: String },
         ...
     }
     ```
  The running daemon and client transport exclusively use the flat `connection.rs` schema. The definition in `ipc-protocol/src/framing.rs` is legacy/dormant.
- **Handoff Remediation**:
  - **Backend (Claude 1)**: Consolidate or deprecate the dormant `IpcResponse` in `ipc-protocol/src/framing.rs` to avoid misleading developers reading the protocol crate.

---

### Issue 5: Reconciled Task Failure Semantics (`daemon_interrupted`)
- **Severity**: Medium (State Transition Handling)
- **Affected Components**:
  - Rust: `crates/control-plane/src/engine.rs:reconcile_interrupted_executions`
  - TypeScript: `apps/agent-studios-vscode/src/state/TaskExecutionStore.ts`, `apps/agent-studios-vscode/src/bridge/DaemonConnectedBridge.ts`
- **Root Cause**:
  In M15-04, when `reconcile_interrupted_executions()` runs during daemon startup:
  1. Active tasks in `TaskState::Running` are transitioned to `TaskState::Failed`.
  2. Active runs in `RunState::Starting` or `RunState::Running` are transitioned to `RunState::Failed`.
  3. A `RunOutcomeRecorded` event is emitted with `classification: "daemon_interrupted"`.
  4. When the UI client reconnects, it receives tasks marked as `failed`. If the UI only checked for user-initiated failures or normal errors, the task would appear abruptly failed without context.
- **Handoff Remediation**:
  - **UI**: In `DaemonConnectedBridge::mapDaemonEvent` and `TaskExecutionStore`, parse `RunOutcomeRecorded` details. If `classification === 'daemon_interrupted'`, surface a dedicated warning banner: *"Task execution was interrupted by daemon restart/crash. Durable state was recovered."*

---

### Issue 6: Non-Admin Diagnostic Redaction on `database_path`
- **Severity**: Low (Security Invariant)
- **Affected Components**:
  - Rust: `crates/ipc-transport/src/connection.rs:1755-1758`
  - TypeScript: `apps/agent-studios-vscode/src/bridge/ipc/RustIpcProtocolTypes.ts`
- **Root Cause**:
  In PR #30, `agsd` enforces least-privilege information disclosure:
  ```rust
  IpcCommand::GetRecoveryStatus => {
      let mut status = server_state.recovery_status.lock().unwrap().clone();
      if session.role != ClientRole::Admin {
          status.database_path = None;
      }
      tx.send(IpcResponse::RecoveryStatus { status }).await?;
  }
  ```
  Ordinary UI connections authenticate with standard client credentials found in `agsd.json` (`ClientRole::Standard`), meaning `database_path` will be `null`.
- **Handoff Remediation**:
  - **UI**: Ensure TypeScript interface marks `database_path?: string | null` and UI diagnostics views do not assume the path is populated.

---

### Issue 7: `RustTaskRecord` Missing Execution Specification Field
- **Severity**: Low (Schema Incompleteness)
- **Affected Components**:
  - Rust: `crates/protocol/src/task.rs:134` (`pub execution: Option<TaskExecutionSpec>`)
  - TypeScript: `apps/agent-studios-vscode/src/bridge/ipc/RustIpcProtocolTypes.ts:122-135`
- **Root Cause**:
  `RustTaskRecord` in TypeScript omits the `execution` field:
  ```typescript
  export interface RustTaskRecord {
    id: string;
    studio_id: string;
    ...
    // execution?: RustTaskExecutionSpec | null; is missing
  }
  ```
  `RustRunRecord` already contains `execution?: RustTaskExecutionSpec | null;`.
- **Handoff Remediation**:
  - Add `execution?: RustTaskExecutionSpec | null;` to `RustTaskRecord` in `RustIpcProtocolTypes.ts`.

---

## 3. Actionable Handoff Checklist for Claude 1 & UI-04 Integration

### Backend Tasks (Claude 1 / PR #30+):
- [ ] **Replay Buffer Hydration on Restart**: In `IpcServerBuilder::build`, backfill `ReplayBuffer` with the latest events from `SqliteEventStore` so clients resuming with `from_sequence` do not encounter `SequenceGapRecoverable`.
- [ ] **Graceful Serialization Error Framing**: If `recv_msg` fails to deserialize JSON due to UUID parsing or invalid fields, attempt sending `{ "type": "error", "message": "Deserialization error: ..." }` before dropping the connection.
- [ ] **Unify Protocol Framing**: Mark `crates/ipc-protocol/src/framing.rs::IpcResponse` as deprecated or replace it with the flat internally tagged enum matching `connection.rs`.

### UI Tasks (Agent Studios VS Code / PR #27):
- [ ] **Declare Recovery Status Types**: Add `RustRecoveryStatusReport` to `RustIpcProtocolTypes.ts` and add `get_recovery_status` / `recovery_status` variants.
- [ ] **Expose Recovery Diagnostics**: Add `getRecoveryStatus()` method to `ExecutionClient` and `DaemonConnectedBridge`.
- [ ] **Handle Daemon Interrupted State**: Add handling for `classification: 'daemon_interrupted'` in `TaskExecutionStore` to inform users when runs failed due to daemon restart.
- [ ] **Handle Sequence Watermark Resets**: In `NativeIpcTransport`, if a reconnect fails with `SequenceGapRecoverable` or session ID changes, fallback to `subscribeEvents(0)` and refresh domain state via `initializeFromBridge()`.
- [ ] **Add Execution Spec to `RustTaskRecord`**: Update `RustTaskRecord` interface to include optional `execution?: RustTaskExecutionSpec | null`.
