# M13 IPC-02.2.2 Approval Delivery Consistency & Concurrency Audit

**Date**: 2026-10-30  
**Target Branch**: `feat/m13-ipc-02-control-plane` (Pull Request #19)  
**Status**: VERIFIED & SECURED  

---

## 1. Architectural Challenge

In Agent Studios, human-in-the-loop approvals gate sensitive agent actions (file system mutations, command execution, and network requests). 
Two critical consistency guarantees must be maintained:
1. **Two-Phase Delivery Ordering**: An approval resolution decision must be successfully transmitted to the external agent runtime *before* the Control Plane state transitions from `Pending` to `Approved`/`Rejected`. If the agent runtime process has terminated or rejects the decision, the Control Plane must not transition to `Approved`.
2. **Concurrent Resolution Serialization**: If multiple operators or automated supervisors attempt to resolve the same approval simultaneously, exactly one resolution must execute; duplicate or concurrent attempts must fail closed without race conditions or partial state commits.

---

## 2. Technical Remediation

### 2.1 Two-Phase Delivery Protocol (2PC)
In `crates/ipc-transport/src/connection.rs`, `ResolveApproval` implements the following ordering:
1. **Pre-Check**: Verify approval exists in Control Plane and is in `Pending` state.
2. **Authorization**: Verify requesting client is `ClientRole::Admin` or matches the authenticated `key_id` associated with the approval origin.
3. **Runtime Delivery**: Check `server_state.pending_runtime_approvals`. If mapped to an active agent runtime session, dispatch `RuntimeInput::ApprovalResponse` via `runtime.send(&session_ref, input)`.
4. **Delivery Failure Fail-Closed**: If `runtime.send()` returns an error:
   - The Control Plane mutation is aborted.
   - The pending mapping is retained in `pending_runtime_approvals` to allow client retries once connectivity is restored.
   - A descriptive error response is returned to the client.
5. **Control Plane Commit**: Only upon successful delivery is `control_plane.resolve_approval()` invoked, emitting linearizable audit events.

### 2.2 Concurrency Serialization via RAII Guards
Concurrent resolution commands on the same `approval_id` are serialized using `server_state.in_flight_approvals`:
```rust
let _approval_guard = {
    let mut in_flight = server_state.in_flight_approvals.lock().unwrap();
    if in_flight.contains(&approval_id) {
        tx.send(IpcResponse::Error {
            message: format!("Approval resolution already in progress for approval {}", approval_id),
        })
        .await
        .map_err(|_| TransportError::ConnectionClosed)?;
        continue;
    }
    in_flight.insert(approval_id.clone());
    InFlightApprovalGuard {
        approval_id: approval_id.clone(),
        in_flight: server_state.in_flight_approvals.clone(),
    }
};
```
The RAII `InFlightApprovalGuard` guarantees that the lock entry is removed upon function exit, error returns, or client disconnects, preventing lock leaks.

### 2.3 Task Origin Inheritance for Runtime Approvals
When an agent runtime requests approval inside `execute_task_lifecycle`, the approval inherits the owning task's cryptographic origin (`task_origins`):
```rust
if let Some((s_id, k_id)) = server_state
    .task_origins
    .lock()
    .unwrap()
    .get(&task_id)
    .cloned()
{
    server_state.register_approval_origin(app.id.to_string(), s_id, k_id);
}
```
This ensures that the client who created the task (or a reconnected session with the same HMAC key) is authorized to resolve runtime approval requests without requiring an admin token.

---

## 3. Verification & Evidence

The following integration tests in `crates/ipc-transport/tests/m13_ipc_02_2_integration_tests.rs` verify these properties:
1. `test_approval_runtime_delivery_failure_fails_closed_without_modifying_control_plane`:
   - Simulates delivery network failure during approval resolution.
   - Confirms Control Plane state remains `Pending`.
   - Confirms retry after restoring network succeeds and transitions state to `Approved`.
2. `test_concurrent_approval_resolution_serialized_and_deduplicated`:
   - Fires two concurrent `resolve_approval` requests across separate client connections.
   - Proves exactly one resolution succeeds.
   - Proves the losing attempt fails closed with a clean serialization or terminal state message (`already in terminal state Approved`).
   - Confirms Control Plane state commits cleanly with zero deadlocks.
3. `test_runtime_approval_delivery_and_cancellation`:
   - Validates end-to-end event subscription, runtime delivery, and subsequent task cancellation.
