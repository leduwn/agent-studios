# M13 IPC-02.2.1 Test Execution & Verification Evidence

**Date**: 2026-10-30  
**Milestone**: M13 (IPC-02.2.1 Cross-Platform CI & Security Correctness)  
**Target Branch**: `feat/m13-ipc-02-control-plane` (Pull Request #19)  
**Status**: VERIFIED & PASSING  

---

## 1. Test Execution Summary

### 1.1 `agent-studios-ipc-transport` Test Suite

Command:
```powershell
cargo test -p agent-studios-ipc-transport --manifest-path D:\Projects\agent-studios-m13\agent-studios-rs\Cargo.toml
```

Results:
- **Unit Tests (`connection::tests`)**: 26 passed; 0 failed
- **Integration Tests (`control_plane_ipc_tests.rs`)**: 11 passed; 0 failed
  - `test_ipc_approval_cannot_resolve_for_terminal_task`: ok
  - `test_ipc_linearizable_replay_and_reconnect`: ok
  - `test_ipc_studio_and_session_discovery`: ok
  - `test_ipc_provider_catalog_query`: ok
  - `test_ipc_approval_resolution_rejected_for_standard_client_on_untracked_approval`: ok
  - `test_ipc_approval_resolution`: ok
  - `test_ipc_task_submission_and_lifecycle`: ok
  - `test_ipc_task_cancellation`: ok
  - `test_ipc_task_auto_execution_fails_closed_when_unconfigured`: ok
  - `test_ipc_multiple_subscriptions_no_duplicate_events`: ok
  - `test_ipc_task_auto_execution_lifecycle_succeeds_with_runtime`: ok (using `IsolatedTestRepo`)
- **End-to-End Tests (`e2e_tests.rs`)**: 8 passed; 0 failed
  - `test_e2e_unauthenticated_command_rejected`: ok
  - `test_e2e_event_subscription_and_live_broadcast`: ok
  - `test_e2e_reconnect_stream_auto_watermark_recovery`: ok
  - `test_e2e_graceful_shutdown`: ok
  - `test_e2e_wrong_auth_key_rejected`: ok
  - `test_e2e_connect_authenticate_and_ping`: ok
  - `test_e2e_disconnect_reconnect_with_watermark_recovery`: ok
  - `test_e2e_event_replay_buffer_overflow_and_recovery`: ok
- **Integration Tests (`m13_ipc_02_2_integration_tests.rs`)**: 9 passed; 0 failed
  - `test_worktree_orchestrator_fail_closed_on_invalid_repo`: ok
  - `test_bootstrap_credential_segregation_and_untracked_approval_role_gate`: ok
  - `test_approval_runtime_delivery_failure_fails_closed_without_modifying_control_plane`: ok
  - `test_runtime_approval_delivery_and_cancellation`: ok
  - `test_runtime_cancellation_stops_session`: ok
  - `test_cross_session_cancellation_rejected_for_standard_clients`: ok
  - `test_runtime_stop_failure_fails_closed`: ok
  - `test_cancel_task_with_wrong_studio_id_fails_closed_without_stopping_runtime`: ok
  - `test_late_runtime_completed_event_ignored_after_cancellation`: ok
- **Total Tests for `agent-studios-ipc-transport`**: 54 passed; 0 failed; 0 ignored

---

### 1.2 `agent-studios-opencode-runtime` Test Suite

Command:
```powershell
cargo test -p agent-studios-opencode-runtime --manifest-path D:\Projects\agent-studios-m13\agent-studios-rs\Cargo.toml
```

Results:
- `m11_approval_authority_tests`: 1 passed; 0 failed
- `m11_process_supervision_tests`: 5 passed; 0 failed
- `m11_protocol_tests`: 8 passed; 0 failed
- `m11_provider_mapping_tests`: 4 passed; 0 failed
- `m11_runtime_lifecycle_tests`: 3 passed; 0 failed
- `m11_sandbox_containment_tests`: 10 passed; 0 failed (Win32 Job Object tests conditional)
- `m11_sse_tests`: 9 passed; 0 failed
- `mock_server_tests`: 2 passed; 0 failed
- `provider_and_workspace_tests`: 4 passed; 0 failed
- `sandbox_tests`: 4 passed; 0 failed
- **Total Tests for `agent-studios-opencode-runtime`**: 50 passed; 0 failed; 0 ignored

---

## 2. Quality Gate Verification

### 2.1 Code Formatting (`cargo fmt`)
Command:
```powershell
cargo fmt --manifest-path D:\Projects\agent-studios-m13\agent-studios-rs\Cargo.toml -p agent-studios-ipc-transport -p agent-studios-opencode-runtime -- --check
```
Exit code: 0 (0 diffs).

### 2.2 Static Analysis & Linter (`cargo clippy`)
Command:
```powershell
cargo clippy -p agent-studios-ipc-transport -p agent-studios-opencode-runtime --all-targets --manifest-path D:\Projects\agent-studios-m13\agent-studios-rs\Cargo.toml -- -D warnings
```
Exit code: 0 (0 warnings).

### 2.3 Subtree Invariant Verification
Commands:
```powershell
git -C D:\Projects\agent-studios-m13 status --short codex-rs
git -C D:\Projects\agent-studios-m13 diff --stat codex-rs
```
Result: 0 files modified, 0 bytes changed.
