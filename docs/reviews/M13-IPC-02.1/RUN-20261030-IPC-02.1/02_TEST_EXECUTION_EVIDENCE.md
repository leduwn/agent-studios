# Milestone M13 IPC-02.1 — Test Execution Evidence

- **Execution Run**: `RUN-20261030-IPC-02.1`
- **Date**: `2026-10-30T10:37:00+07:00`
- **Target**: `agent-studios-ipc-transport`
- **Result**: **44 / 44 PASSED (100% SUCCESS, 0 FAILURES, 0 CLIPPY WARNINGS)**

---

## 1. Unit & Integration Test Summary

```text
running 25 tests in src/lib.rs
test daemon::tests::test_default_runtime_path_non_empty ... ok
test connection::tests::test_handshake_rejects_unauthenticated_first_command ... ok
test connection::tests::test_handshake_direct_authenticate_and_get_health ... ok
test connection::tests::test_request_and_resolve_approval ... ok
test connection::tests::test_handshake_negotiate_then_authenticate_and_ping ... ok
test connection::tests::test_event_subscription_backfill_and_streaming ... ok
test connection::tests::test_invalid_hmac_rejected ... ok
test connection::tests::test_untracked_approval_origin_fails_closed_for_standard_client ... ok
test framing::tests::test_oversized_frame_rejected_before_alloc ... ok
test framing::tests::test_send_and_recv_frame ... ok
test connection::tests::test_foreign_approval_resolution_rejected_for_standard_client ... ok
test unix::permissions::tests::test_validate_dir_mode_and_owner_rejection ... ok
test connection::tests::test_shutdown_capability_authorization ... ok
test unix::permissions::tests::test_resolve_socket_path_fallback ... ok
test unix::permissions::tests::test_validate_dir_mode_and_owner_success ... ok
test unix::permissions::tests::test_validate_socket_mode_and_owner_rejection ... ok
test unix::permissions::tests::test_validate_socket_mode_and_owner_success ... ok
test unix::uds::tests::test_non_unix_mock_uds ... ok
test windows::tests::test_invalid_pipe_name_rejected ... ok
test windows::tests::test_security_descriptor_creation_succeeds ... ok
test unix::permissions::tests::test_resolve_socket_path_custom ... ok
test windows::tests::test_named_pipe_roundtrip ... ok
test unix::permissions::tests::test_resolve_socket_path_xdg ... ok
test daemon::tests::test_daemon_bootstrap_info_roundtrip ... ok
test connection::tests::test_graceful_disconnect_decrements_active_connections ... ok
test result: ok. 25 passed; 0 failed

running 11 tests in tests/control_plane_ipc_tests.rs
test test_ipc_studio_and_session_discovery ... ok
test test_ipc_approval_cannot_resolve_for_terminal_task ... ok
test test_ipc_approval_resolution_rejected_for_standard_client_on_untracked_approval ... ok
test test_ipc_approval_resolution ... ok
test test_ipc_task_cancellation ... ok
test test_ipc_task_submission_and_lifecycle ... ok
test test_ipc_provider_catalog_query ... ok
test test_ipc_linearizable_replay_and_reconnect ... ok
test test_ipc_task_auto_execution_fails_closed_when_unconfigured ... ok
test test_ipc_task_auto_execution_lifecycle_succeeds_with_runtime ... ok
test test_ipc_multiple_subscriptions_no_duplicate_events ... ok
test result: ok. 11 passed; 0 failed

running 8 tests in tests/e2e_tests.rs
test test_e2e_wrong_auth_key_rejected ... ok
test test_e2e_unauthenticated_command_rejected ... ok
test test_e2e_graceful_shutdown ... ok
test test_e2e_connect_authenticate_and_ping ... ok
test test_e2e_event_subscription_and_live_broadcast ... ok
test test_e2e_disconnect_reconnect_with_watermark_recovery ... ok
test test_e2e_reconnect_stream_auto_watermark_recovery ... ok
test test_e2e_windows_named_pipe_full_lifecycle ... ok
test result: ok. 8 passed; 0 failed
```

---

## 2. Clippy & Linter Verification

```bash
cargo clippy -p agent-studios-ipc-transport -- -D warnings
```
Output:
```text
Checking agent-studios-ipc-transport v0.1.0 (D:\Projects\agent-studios-m13\agent-studios-rs\crates\ipc-transport)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.31s
```
Status: **0 errors, 0 warnings**.
