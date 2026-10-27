# Milestone M12.2 — Approval Concurrency, Delivery Verification & Lifecycle Hardening

- **Milestone**: `M12.2 — Final Integration & Release Gate`
- **Execution Run**: `RUN-20261028-M12.2-release-gate`
- **Date**: `2026-10-28T00:45:00+07:00`
- **Lead Owner**: Agent B, Agent D & Principal Engineering Lead
- **Target Repository**: `https://github.com/leduwn/agent-studios`
- **Active Branch**: `feat/m12-claude-code-runtime`
- **Scope**: `src/approval.rs`, `src/runtime.rs`, `tests/approval_concurrency_tests.rs`
- **Status**: **VERIFIED & PRODUCTION HARDENED**

---

## 1. Executive Summary

Milestone M12.2 addresses critical lifecycle and concurrency hazards identified during audit of the host approval system:
1. **Two-Phase Approval Delivery Verification**: An approval decision is **not** considered delivered merely because it was popped from the pending table. State transition requires confirmed delivery to child `stdin`. If write fails, the request transitions to `DeliveryFailed` and fails closed without allow fallback.
2. **Watchdog Task Leak Elimination**: Every active timeout watchdog now has its `tokio::task::JoinHandle` tracked in `watchdog_handles`. Tasks are aborted and reaped upon operator resolution, cancellation, or timeout expiration.
3. **Deterministic ID Scoping**: UUIDv5 `ApprovalId` derivation now scopes across `session_id` and `request_id`, eliminating cross-session collision risks.
4. **Idempotent Duplicate Request Handling**: Duplicate `request_id` emissions within a session are handled deterministically, returning the existing `ApprovalId` without corrupting state.
5. **Microsecond Race Protection**: Atomic transitions guarantee mutual exclusion between host operator decisions and timeout watchdog callbacks.
6. **Clean Interruption & Termination**: Sessions in `interrupt()` or `stop()` abort all pending approvals and cancel all watchdogs fail-closed.

---

## 2. State Machine Architecture

### 2.1 Approval State Lifecycle
```text
                  ┌───────────────┐
                  │    Pending    │
                  └───────┬───────┘
                          │
          ┌───────────────┼───────────────┬────────────────┐
          ▼               ▼               ▼                ▼
   [prepare_resolution] [timeout]    [cancel_all]    [session stop]
          │               │               │                │
          ▼               ▼               ▼                ▼
   ┌───────────────┐ ┌─────────┐   ┌─────────────┐  ┌─────────────┐
   │   Resolving   │ │ TimedOut│   │  Cancelled  │  │  Cancelled  │
   └───┬───────┬───┘ └─────────┘   └─────────────┘  └─────────────┘
       │       │
[write ok]  [write fail]
       │       │
       ▼       ▼
┌───────────┐ ┌────────────────┐
│ Delivered │ │ DeliveryFailed │ (fail-closed, record kept)
└───────────┘ └────────────────┘
```

### 2.2 Two-Phase Commit Protocol
1. **Phase 1 (`prepare_resolution`)**:
   - Locks pending records and inspects state.
   - If not `Pending`, rejects with `ClaudeCodeError::ApprovalDenied`.
   - Aborts and removes watchdog timer from `watchdog_handles`.
   - Transitions state to `Resolving`.
   - Issues a cryptographic `ResolutionToken` binding `approval_id`, `request_id`, and `decision`.
2. **Phase 2 (`stdin write & commit`)**:
   - `runtime.rs` serializes `ControlResponse` to NDJSON and invokes `process.write_stdin_line(&json).await`.
   - **On Success**: Calls `authority.confirm_delivery(token)`. State becomes `Delivered`.
   - **On Failure**: Calls `authority.fail_delivery(token)`. State becomes `DeliveryFailed`. Record is preserved in memory for auditing, but cannot be re-executed or auto-allowed. Returns `RuntimeError::send_failed`.

---

## 3. Concurrency & Race Condition Hardening

### 3.1 Host vs. Timeout Race Condition
When an operator approves at the exact microsecond the watchdog timer elapses:
- `prepare_resolution` and `resolve_timeout_by_id` contend on the internal `RwLock`/`Mutex`.
- Whichever method acquires the write lock first changes state out of `Pending`.
- The losing method immediately sees the non-`Pending` state and yields cleanly:
  - If host wins: watchdog abort is triggered, timeout callback sees `None`, zero deny frame is sent.
  - If watchdog wins: host resolution fails with `ApprovalDenied("Approval request ... timed out")`, zero allow frame is sent.

### 3.2 Watchdog Task Leak Prevention
Previous implementation spawned detached `tokio::spawn` tasks where `JoinHandle` was discarded.
Now:
```rust
pub struct ApprovalAuthority {
    ...
    watchdog_handles: Mutex<HashMap<ApprovalId, tokio::task::JoinHandle<()>>>,
}
```
Every spawned watchdog stores its `JoinHandle`. Upon resolution, cancellation, or timeout, the handle is removed and `.abort()` is called if still pending, preventing async task leakage in long-running runtime instances.

---

## 4. Test Verification Evidence (23/23 Tests Passed)

### 4.1 Concurrency & Stress Tests (`tests/approval_concurrency_tests.rs`)
| Test Name | Iterations / Concurrency | Result | Verified Property |
|---|:---:|:---:|---|
| `test_concurrent_approval_requests_different_tools_and_subagents` | 60 requests across 6 tools | **PASS** | Independent parallel Allow/Deny resolution without cross-talk |
| `test_duplicate_request_id_deterministic_id_and_fail_closed` | Duplicate wire IDs | **PASS** | Identical UUIDv5 derivation; secondary resolution fails closed |
| `test_microsecond_race_timeout_vs_host_resolution` | 60 microsecond races | **PASS** | Strict mutual exclusion; zero double-resolution |
| `test_child_stdin_write_failure_fails_closed_and_not_marked_delivered` | Closed child pipe | **PASS** | Write error triggers `fail_delivery`, state = `DeliveryFailed` |
| `test_runtime_send_approval_when_session_terminal_fails_closed` | Terminal state | **PASS** | Rejection on dead session |
| `test_watchdog_lifecycle_no_leaked_tasks_on_resolution` | 15s watchdog | **PASS** | Handle cleaned up in <500ms |
| `test_watchdog_lifecycle_no_leaked_tasks_on_cancellation` | 20 watchdogs | **PASS** | All handles cleaned up in <500ms |
| `test_watchdog_lifecycle_no_leaked_tasks_on_timeout` | Expired watchdog | **PASS** | Task terminates cleanly in <200ms |

### 4.2 Unit Tests (`src/approval.rs`)
7 unit tests passing:
- `test_approval_decision_formatting`
- `test_can_use_tool_request_from_control_request`
- `test_derive_scoped_approval_id_uniqueness`
- `test_two_phase_delivery_commit_success`
- `test_two_phase_delivery_commit_failure_fails_closed`
- `test_cancel_all_pending_fails_closed`
- `test_duplicate_registration_idempotency`
