# M15 UI-04.1 — Report 02: ReplayBuffer Lifecycle, Reconnect Root Cause & Crash Reconciliation

## Metadata
- **Milestone**: M15 UI-04.1: Cross-PR Integration Closure & Evidence Reconciliation
- **Focus**: Daemon Restart Reconnection, ReplayBuffer State, Sequence Gap Trap & Crash Invariants
- **Target PR**: Draft PR #27 (`feat/m15-real-agent-execution-ui`) targeting `feat/m14-mission-control-connected-integration`
- **Authoritative Backend SHA**: `dcbcb39bf61225a4660be5b533cf1ffd5de14b29` (Backend PR #30, M15-04.1)
- **Date**: 2026-11-02
- **Author**: Claude 2 (Frontend / VSCode Extension Integration Lead)

---

## 1. Executive Summary

Milestone M15-04 (`dcbcb39b`) introduced SQLite WAL event persistence (`SqliteEventStore`) and crash recovery (`reconcile_interrupted_executions`) to the Rust daemon `agsd`. However, the interaction between persistent storage and volatile IPC streaming revealed an architectural boundary condition:

When `agsd` restarts after a crash or clean termination, its in-memory `ReplayBuffer` initializes empty. A reconnecting client that issues `SubscribeEvents { from_sequence: K + 1 }` (where $K \ge 1$) encounters a `SequenceGapRecoverable` protocol error, aborting the event stream unless handled correctly by both frontend and backend.

This report documents the exact root cause, state machine lifecycle, wire trace, and the multi-layer remediation ensuring 100% causal consistency without silent sequence skipping or auto-resumption.

---

## 2. ReplayBuffer State Machine Lifecycle Across Daemon Restart

### Volatile vs Persistent Storage Discrepancy
The daemon maintains two distinct event stores:
1. **Persistent Tier (`SqliteEventStore`)**:
   - Location: `<state_dir>/agsd.db` with WAL mode enabled.
   - Durability: Survives daemon restart, SIGKILL, and OS crashes.
   - Contains: All persisted domain events (`StudioCreated`, `TaskCreated`, `TaskStatusChanged`, `RunOutcomeRecorded`).
2. **Volatile In-Memory Ring Buffer (`ReplayBuffer`)**:
   - Location: Heap-allocated ring buffer in `crates/ipc-protocol/src/replay.rs`.
   - Purpose: Low-latency backpressure and fast replay for short-lived network hiccups.
   - Initialization: Constructed afresh during `IpcServerBuilder::build` on every process spawn (`ReplayBuffer::new(capacity)`).

### The Restart Sequence Gap Mechanism

```
[Daemon Process 1]
Events emitted: Seq 1 -> 42
Persisted to SQLite: Seq 1 -> 42
Volatile ReplayBuffer: Holds Seq 1 -> 42
Client Watermark: received_seq = 42

====> ABNORMAL DAEMON TERMINATION (SIGKILL) <====

[Daemon Process 2 Spawned]
SQLite WAL: Reconciles interrupted tasks. Total persisted events = 43.
Volatile ReplayBuffer: BRAND NEW (capacity: 1000, items: 0, last_seq: None).

[Client Reconnection]
1. Handshake & Auth: Success (New Session ID generated: sess-xyz).
2. Client sends: SubscribeEvents { from_sequence: 43 }
3. Daemon checks ReplayBuffer::replay_from(43):
   - buffer.is_empty() is TRUE.
   - from_sequence (43) > 1 is TRUE.
   - Returns: Err(ProtocolError::SequenceGapRecoverable { expected: 1, found: 43 })
4. Wire Response: { "type": "error", "message": "Failed to replay events: Sequence gap..." }
5. Connection event streaming task terminates!
```

---

## 3. Protocol Wire Trace & Concrete Code Anchors

### Rust Backend: `crates/ipc-protocol/src/replay.rs`
```rust
pub fn replay_from(&self, from_sequence: u64) -> Result<Vec<VersionedEventFrame>, ProtocolError> {
    if self.buffer.is_empty() {
        if from_sequence > 1 {
            return Err(ProtocolError::SequenceGapRecoverable {
                expected: 1,
                found: from_sequence,
            });
        }
        return Ok(Vec::new());
    }
    // ...
}
```

### Rust IPC Connection Handler: `crates/ipc-transport/src/connection.rs`
```rust
IpcCommand::SubscribeEvents { from_sequence } => {
    let events = match server_state.replay_buffer.lock().unwrap().replay_from(from_sequence) {
        Ok(evts) => evts,
        Err(e) => {
            tx.send(IpcResponse::Error { message: format!("Failed to replay events: {}", e) }).await?;
            return Ok(()); // Connection handler terminates streaming loop
        }
    };
    // ...
}
```

---

## 4. Frontend Remediation: Safe Reconnect & Hydration Without Silent Skipping

### Why "Silently Skipping" Gaps is Forbidden
A naive fix would simply ignore sequence gaps or advance the watermark to the latest sequence. This is strictly rejected under Milestone M15 UI safety invariants:
- **Causal Inversion**: Silently skipping events means missing `RunOutcomeRecorded` or state mutations, leaving UI views displaying ghost `running` tasks.
- **Lost Artifact Notifications**: Artifact registrations stream as domain events; missing them breaks cryptographic verification.

### Authoritative Frontend Remediation Architecture
1. **Session ID Change Detection**:
   Upon completing the HMAC handshake, `NativeIpcTransport` compares `authResp.session_id` against `this.activeSessionId`. If the session ID changed (indicating a new daemon instance):
   - The connection watermark is marked as invalidated.
   - `onStateDegraded` is fired with reason `'DAEMON_RESTART_SESSION_RESET'`.
2. **Graceful Fallback to Full State Hydration**:
   Instead of requesting partial replay from sequence 43, the extension issues `subscribeEvents(1)` (or `subscribeEvents(0)`) to stream from inception, and immediately performs a complete parallel state re-hydration:
   - `listStudios()`
   - `listTasks(studioId)`
   - `getRecoveryStatus()`
3. **Task Status Projection Verification**:
   The `TaskExecutionStore` reconciles in-memory state against the fresh `listTasks` response. Any task previously remembered as `running` is projected to `failed` (`daemon_interrupted`).

---

## 5. Crash Reconciliation Semantics: `daemon_interrupted`

In Milestone M15-04 (`crates/control-plane/src/engine.rs`), `reconcile_interrupted_executions()` executes during startup:
1. Queries database for tasks where `state == 'running'`.
2. Sets task state to `failed`.
3. Records failure domain classification: `daemon_interrupted`.
4. Increments `interrupted_tasks_count` in the `RecoveryStatusReport`.

### Zero Auto-Resumption Invariant
The frontend bridge adheres to a non-negotiable safety invariant:
- **Zero Autonomous Execution**: Upon daemon reconnect, the extension bridge **strictly NEVER** re-submits or restarts interrupted tasks.
- **Operator Review Requirement**: Interrupted tasks remain distinctly displayed as `FAILED (Daemon Interrupted)`. The operator retains sole authority to inspect worktree state, evaluate diffs, and manually re-dispatch tasks via deliberate user confirmation.

---

## 6. Least-Privilege Role-Based Diagnostic Redaction

The `get_recovery_status` IPC command implements role-based security:

| Field | Standard Client Role (`ClientRole::Standard`) | Admin Client Role (`ClientRole::Admin`) | Security Rationale |
|---|---|---|---|
| `database_healthy` | `true` | `true` | Health check safe for all clients |
| `interrupted_tasks_count` | Exact count (e.g., `1`) | Exact count (e.g., `1`) | Operator visibility |
| `total_persisted_events` | Exact count (e.g., `42`) | Exact count (e.g., `42`) | Telemetry |
| `database_path` | **`null`** (Redacted) | Verified host path (`D:\...\agsd.db`) | **Prevents host filesystem reconnaissance from standard extension context** |

Verified by E2E test in `RealRustDaemonRecoveryE2E.test.ts` (Test 3):
```typescript
assert.strictEqual(standardStatus.database_path, null);
assert.ok(adminResp.status.database_path.includes('agsd.db'));
```
