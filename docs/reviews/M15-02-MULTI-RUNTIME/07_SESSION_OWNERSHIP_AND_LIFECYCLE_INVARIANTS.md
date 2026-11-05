# M15-02 Session Ownership & Lifecycle Invariants

**Milestone**: M15-02  
**Component**: `agent-studios-ipc-transport::server::ServerState` & `connection`  
**Author**: Principal Systems Architect (Claude 1)  
**Date**: 2026-10-31  

---

## 1. Problem Statement: The Lifecycle Routing Gap

When an IPC task is executing, external asynchronous events (task cancellation via `IpcCommand::CancelTask` or human approval resolution via `IpcCommand::ResolveApproval`) must reach the specific runtime adapter managing that session.

In a multi-runtime system, if `active_runtime_sessions` only stored `RuntimeSessionRef`:
```rust
// Flawed legacy approach
pub active_runtime_sessions: Arc<RwLock<HashMap<TaskId, RuntimeSessionRef>>>,
```
the daemon would have to guess or scan which runtime owned the session. Worse, if multiple runtimes use numeric or overlapping session IDs, cancellation could be sent to the wrong runtime adapter, leaving the real session running orphaned.

---

## 2. The Owning Runtime Pointer Invariant

M15-02 addresses this by pairing the session reference and approval ID with an immutable reference-counted handle (`Arc<dyn AgentRuntime>`) to the exact adapter instance that created it:

```rust
// M15-02 ServerState Lifecycle Registries
pub active_runtime_sessions: Arc<RwLock<HashMap<TaskId, (RuntimeSessionRef, Arc<dyn AgentRuntime>)>>>,
pub pending_runtime_approvals: Arc<RwLock<HashMap<ApprovalId, (RuntimeApprovalId, Arc<dyn AgentRuntime>)>>>,
```

### 2.1 Task Execution Dispatch
During `auto_execute_task`, once `selected_runtime` is resolved and `start()` succeeds:
```rust
let session_ref = selected_runtime.start(start_request).await?;

// Authoritative session binding
server_state.active_runtime_sessions.write().await.insert(
    task_id,
    (session_ref.clone(), Arc::clone(&selected_runtime)),
);
```

### 2.2 Task Cancellation Routing
When a client sends `cancel_task(studio_id, task_id)`:
```rust
let session_binding = {
    let mut sessions = server_state.active_runtime_sessions.write().await;
    sessions.remove(&task_id)
};

if let Some((session_ref, owning_runtime)) = session_binding {
    // Direct dispatch to owning runtime
    let stop_request = RuntimeStopRequest::new(session_ref, reason);
    if let Err(err) = owning_runtime.stop(stop_request).await {
        tracing::error!("Failed to stop owning runtime: {}", err);
    }
}
```
**Invariant**: Cancellation is guaranteed to dispatch to the originating runtime adapter, regardless of registry additions, removals, or defaults.

### 2.3 Approval Resolution Routing
When the runtime emits an approval request:
```rust
server_state.pending_runtime_approvals.write().await.insert(
    control_plane_approval_id,
    (runtime_approval_id, Arc::clone(&owning_runtime)),
);
```
When `resolve_approval` is invoked:
```rust
let approval_binding = {
    let mut approvals = server_state.pending_runtime_approvals.write().await;
    approvals.remove(&approval_id)
};

if let Some((runtime_approval_id, owning_runtime)) = approval_binding {
    owning_runtime.resolve_approval(runtime_approval_id, decision).await?;
}
```
**Invariant**: Approval decisions are dispatched directly to the owning runtime without ambiguous cross-runtime lookups.
