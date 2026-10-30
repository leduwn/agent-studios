# M13 IPC-02.2.2 CancelTask Authorization & Session Security Review

**Date**: 2026-10-30  
**Target Branch**: `feat/m13-ipc-02-control-plane` (Pull Request #19)  
**Status**: VERIFIED & SECURED  

---

## 1. Threat Model & Security Perimeter

In multi-tenant or multi-client environments, task cancellation is a disruptive control-plane action:
- It terminates active operating-system child processes running external agent engines (`runtime.stop()`).
- It permanently transitions tasks and runs in the Control Plane to terminal `Cancelled` states.
- It aborts in-progress git worktree operations and discards uncommitted state.

If authorization fails open, a compromised or unprivileged client could terminate foreign tasks across the studio, causing denial of service (DoS) and data loss.

---

## 2. Flaw Analysis: The `None => false` Inversion

In the initial implementation of `CancelTask` authorization, the origin lookup contained an implicit fail-open flaw:
```rust
// FLAWED IMPLEMENTATION:
let is_foreign = {
    let origins = server_state.task_origins.lock().unwrap();
    match origins.get(&task_id) {
        Some((s_id, _)) => s_id != &session.session_id,
        None => false, // FLAW: Untracked task origin defaulted to NOT foreign (allowed!)
    }
};
```
If a task lacked an explicit origin record (for example, if submitted via background automation, internal scheduler, or across connection drops), `None => false` caused standard clients to be considered authorized owners.

---

## 3. Remediation & Hardened Security Architecture

The authorization boundary was redesigned with a strict **fail-closed** model and **cryptographic principal rebinding**:

### 3.1 Fail-Closed Authorization Logic
```rust
if session.role != ClientRole::Admin {
    let is_authorized = {
        let mut origins = server_state.task_origins.lock().unwrap();
        if let Some((s_id, k_id)) = origins.get_mut(&task_id) {
            if let Some(ref caller_kid) = session.key_id {
                if caller_kid == k_id {
                    // Rebind origin to reconnected session if necessary
                    if s_id != &session.session_id {
                        tracing::info!(
                            task_id = %task_id,
                            old_session = %s_id,
                            new_session = %session.session_id,
                            "Rebinding task origin to reconnected session for authenticated key"
                        );
                        *s_id = session.session_id.clone();
                    }
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false // Untracked origin strictly fails closed for standard clients!
        }
    };
    if !is_authorized {
        tx.send(IpcResponse::Error {
            message: "Permission denied: cross-session cancellation requires admin role (untracked or foreign task origin)"
                .to_string(),
        })
        .await
        .map_err(|_| TransportError::ConnectionClosed)?;
        continue;
    }
}
```

### 3.2 Key Security Properties
1. **Untracked Origin Fail-Closed**: Any task missing an entry in `task_origins` strictly requires `ClientRole::Admin` to cancel. Standard clients are denied immediately.
2. **Cryptographic Identity Rebinding**: When a client disconnects and reconnects, it receives a new ephemeral `SessionId`. Because authentication verifies HMAC signatures against the client's `key_id`, ownership is tied to the cryptographic principal (`k_id`). If `caller_kid == k_id`, ownership is preserved and `s_id` is securely rebound to the active session.
3. **Studio Ownership Enforcement**: Cancellation checks that `task.studio_id == request.studio_id`. Cross-studio cancellations are rejected before runtime termination.
4. **Terminal State Gating**: Tasks already in terminal states (`Succeeded`, `Failed`, `Cancelled`) reject cancellation attempts with an explicit error, preventing duplicate termination side effects.
5. **In-Flight Cancellation Deduplication**: Concurrent cancellation requests on the same `task_id` are serialized using `server_state.in_flight_cancellations` with RAII guard release on all return paths.
6. **Pre-Execution Stop Gate**: `runtime.stop()` is invoked only after all authorization, studio matching, and terminal state checks succeed. If `runtime.stop()` fails, the Control Plane state transition is aborted.

---

## 4. Test Evidence

The following integration tests in `crates/ipc-transport/tests/m13_ipc_02_2_integration_tests.rs` verify this security perimeter:
- `test_cancel_task_missing_origin_fails_closed_for_standard_allowed_for_admin`: Verifies untracked origin rejects standard clients with admin requirement error, and succeeds for admin clients.
- `test_cancel_task_reconnected_owner_session_rebinding_succeeds`: Verifies that a client dropping connection and reconnecting with the same authenticated `key_id` successfully rebinds session ownership and cancels the task.
- `test_cross_session_cancellation_rejected_for_standard_clients`: Verifies foreign clients are denied cross-session cancellation.
- `test_cancel_task_with_wrong_studio_id_fails_closed_without_stopping_runtime`: Verifies studio ID mismatch fails closed without terminating the agent runtime.
- `test_cancel_task_duplicate_on_terminal_state_rejected`: Verifies duplicate cancellation on terminal tasks is rejected.
- `test_concurrent_cancellation_serialized_and_deduplicated`: Verifies concurrent cancellation requests are serialized with zero deadlocks.
