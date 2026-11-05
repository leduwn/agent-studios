# M13 IPC-02.2.1 Security & Correctness Audit Report

**Date**: 2026-10-30  
**Milestone**: M13 (IPC-02.2.1 Cross-Platform CI & Security Correctness)  
**Target Branch**: `feat/m13-ipc-02-control-plane` (Pull Request #19)  
**Status**: VERIFIED & PASSING  

---

## 1. Executive Summary

This report documents the security hardening, cross-platform build correctness, and concurrency audit for **M13 IPC-02.2.1** on the `feat/m13-ipc-02-control-plane` branch.

All architectural invariants and security constraints have been satisfied:
1. **Subtree Invariant**: Subtree `codex-rs/` is 100% untouched (0 diffs, 0 bytes modified).
2. **Cross-Platform Conditional Compilation**: Fixed Ubuntu CI failure `E0432` by gating Win32 `JobObjectGuard` re-exports and tests with `#[cfg(windows)]`.
3. **Lock Scope Minimization**: Extracted slow git worktree creation outside Control Plane write lock to eliminate lock starvation and deadlocks.
4. **Disruptive Action Authorization Order**: Reordered `CancelTask` handler to enforce session origin, role, studio ownership, and non-terminal state validation *before* invoking `runtime.stop()`, preventing cross-tenant DoS.
5. **Fail-Closed 2PC Approval Delivery**: Transactional two-phase commit sends approval response to runtime before mutating Control Plane state. Delivery failures fail closed and retain pending mapping for retry.
6. **OpenCode Discovery Fallback**: Added `AGENT_STUDIOS_OPENCODE_PATH` resolution support in discovery and runtime startup.
7. **Test Repo Determinism**: Built `IsolatedTestRepo` fixture providing hermetic git environments for worktree tests, eliminating `.git/index.lock` contention across concurrent test runners.

---

## 2. Vulnerability & Correctness Remediations

### 2.1 E0432: Win32 Conditional Compilation on Linux
- **Vulnerability / Defect**: In `crates/opencode-runtime/src/lib.rs`, `JobObjectGuard` was unconditionally re-exported from `sandbox`. On Linux/Unix platforms where `JobObjectGuard` is omitted from `sandbox.rs`, compilation failed with `E0432: unresolved import`.
- **Remediation**:
  - Gated re-export in `lib.rs`: `#[cfg(windows)] pub use sandbox::JobObjectGuard;`
  - Gated tests referencing `JobObjectGuard` in `tests/m11_sandbox_containment_tests.rs` and `tests/sandbox_tests.rs` with `#[cfg(windows)]`.

### 2.2 Cross-Tenant Runtime Process Termination (CWE-284 / CWE-863)
- **Vulnerability / Defect**: In previous iterations, `CancelTask` could invoke `runtime.stop()` prior to checking if the requesting client had permission to cancel the task, or if the studio ID matched. A malicious or confused client could terminate another tenant's agent process by guessing or intercepting a `TaskId`.
- **Remediation**:
  1. **Origin Verification**: Non-admin clients are restricted to cancelling tasks initiated by their own session (`task_origins`).
  2. **Studio Ownership**: Tasks must match `studio_id` in request; mismatches return immediate error.
  3. **Terminal State Gate**: Tasks already in a terminal state (`Succeeded`, `Failed`, `Cancelled`) cannot be cancelled.
  4. **Runtime Stop**: `runtime.stop()` is invoked only after all checks pass.
  5. **Fail-Closed Execution**: If `runtime.stop()` returns an error, the cancellation does not commit to the Control Plane.

### 2.3 Lock Starvation via Subprocess I/O Under Control Plane Write Lock
- **Vulnerability / Defect**: `execute_task_lifecycle` held `server_state.control_plane.write().await` while calling `create_worktree_async`, which executes `git worktree add` and backoff sleeps. This starved concurrent read operations (such as status queries, event streaming, and heartbeats) and caused test timeouts on Windows.
- **Remediation**:
  - Moved `create_worktree_async` outside the write lock.
  - Re-acquired write lock to register the worktree record and re-verify that the task was not cancelled while the worktree was being created.
  - If concurrent cancellation occurred, the worktree is rolled back.

### 2.4 State Desynchronization via Unchecked Approval Resolution
- **Vulnerability / Defect**: If `ResolveApproval` committed the state change to the Control Plane before confirming delivery to the external runtime, network or process crashes could leave the Control Plane showing `Approved` while the runtime never received the user decision.
- **Remediation**:
  - Implemented transactional 2PC: `pra.runtime.send(&pra.session_ref, input)` executes first.
  - If `runtime.send()` fails, an error response is returned to the client, the pending approval mapping is retained in `pending_runtime_approvals`, and the Control Plane state remains `Pending`.
  - Only upon successful runtime delivery does the Control Plane record the resolution and transition state to `Approved`.

### 2.5 Late Runtime Event Race After Cancellation
- **Vulnerability / Defect**: After a task was cancelled, an asynchronous runtime event loop could deliver a late `Completed` or `Failed` event, overwriting `TaskState::Cancelled` with `TaskState::Succeeded`.
- **Remediation**:
  - Event handlers inspect task state under lock: if `task.state.is_terminal()`, late completion/failure transitions are safely discarded.

### 2.6 Binary Resolution Sanitization & Validation (`AGENT_STUDIOS_OPENCODE_PATH`)
- **Vulnerability / Defect**: In initial implementations, `runtime.rs` directly parsed `AGENT_STUDIOS_OPENCODE_PATH` without `is_file()` validation or path canonicalization, bypassing the sanitary checks in `discovery.rs`.
- **Remediation**:
  - Exposed `resolve_binary_path` as `pub(crate)` in `src/discovery.rs` and consumed directly in `src/runtime.rs`.
  - Guarantees symlink resolution, `..` directory traversal elimination, and regular file existence checks across all binary sources.

### 2.7 Cross-Platform Sandbox Containment Test Compatibility
- **Vulnerability / Defect**: In `tests/m11_sandbox_containment_tests.rs`, `test_strict_fails_closed_without_filesystem_isolation` asserted only on `OpenCodeError::SandboxPolicyViolation`. On non-Windows hosts, strict containment fails closed with `OpenCodeError::UnsupportedPlatform`.
- **Remediation**:
  - Updated test assertion to validate both `SandboxPolicyViolation` on Windows and `UnsupportedPlatform` on non-Windows, ensuring cross-platform fail-closed verification.

---

## 3. Threat Model & Invariants Summary

| Threat / Invariant | Status | Mechanism |
| :--- | :--- | :--- |
| **Cross-Tenant Process Kill** | PREVENTED | Caller session and studio ownership validated prior to `runtime.stop()` |
| **Approval Desynchronization** | PREVENTED | 2PC delivery ensures downstream confirmation before CP state mutation |
| **Control Plane Lock Starvation**| PREVENTED | Long I/O operations execute outside lock boundaries |
| **Late Event State Corruption** | PREVENTED | Terminal states are monotonic and reject post-termination events |
| **Credential Leakage in Logs** | PREVENTED | Credentials redacted via `SanitizedRuntimeMessage` and Debug redactions |
| **Subtree Contamination** | PREVENTED | `codex-rs/` has 0 changes |
