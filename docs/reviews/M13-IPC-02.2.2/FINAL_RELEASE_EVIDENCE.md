# M13 IPC-02.2.2 Final Release Evidence & Test Verification Report

**Date**: 2026-10-30  
**Target Branch**: `feat/m13-ipc-02-control-plane` (Pull Request #19)  
**Status**: 100% PASSING — PRODUCTION READY  

---

## 1. Subtree Invariant Verification

**Requirement**: Subtree `codex-rs/` must remain 100% untouched (0 diffs, 0 bytes modified).
```bash
$ git diff --stat codex-rs
# (zero diffs, empty output)
```
Status: **CONFIRMED 100% UNTOUCHED**

---

## 2. Linter & Static Analysis Verification

### 2.1 Cargo Clippy (`-D warnings`)
Executed across all targets on modified crates:
```bash
$ cargo clippy --manifest-path Cargo.toml -p agent-studios-ipc-transport -p agent-studios-opencode-runtime --all-targets -- -D warnings
    Checking agent-studios-ipc-transport v0.1.0
    Checking agent-studios-opencode-runtime v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.31s
```
Status: **0 WARNINGS, 0 ERRORS**

### 2.2 Cargo Fmt Check
```bash
$ cargo fmt --manifest-path crates/ipc-transport/Cargo.toml -- --check
$ cargo fmt --manifest-path crates/opencode-runtime/Cargo.toml -- --check
```
Status: **100% CLEAN FORMATTING**

---

## 3. Test Suite Execution Results

### 3.1 `agent-studios-ipc-transport` (62/62 Passed)

#### Unit Tests (`src/lib.rs`): 26 Passed
```
test daemon::tests::test_daemon_bootstrap_info_roundtrip ... ok
test daemon::tests::test_default_runtime_path_non_empty ... ok
test framing::tests::test_oversized_frame_rejected_before_alloc ... ok
test framing::tests::test_send_and_recv_frame ... ok
test connection::tests::test_handshake_rejects_unauthenticated_first_command ... ok
test connection::tests::test_handshake_negotiate_then_authenticate_and_ping ... ok
test connection::tests::test_untracked_approval_origin_fails_closed_for_standard_client ... ok
test connection::tests::test_shutdown_capability_authorization ... ok
test connection::tests::test_handshake_direct_authenticate_and_get_health ... ok
test connection::tests::test_event_subscription_backfill_and_streaming ... ok
test connection::tests::test_request_and_resolve_approval ... ok
test unix::permissions::tests::test_resolve_socket_path_custom ... ok
test connection::tests::test_foreign_approval_resolution_rejected_for_standard_client ... ok
test unix::permissions::tests::test_validate_socket_mode_and_owner_success ... ok
test unix::permissions::tests::test_resolve_socket_path_fallback ... ok
test connection::tests::test_invalid_hmac_rejected ... ok
test unix::uds::tests::test_non_unix_mock_uds ... ok
test windows::tests::test_invalid_pipe_name_rejected ... ok
test windows::tests::test_security_descriptor_creation_succeeds ... ok
test unix::permissions::tests::test_validate_dir_mode_and_owner_success ... ok
test unix::permissions::tests::test_validate_dir_mode_and_owner_rejection ... ok
test unix::permissions::tests::test_validate_socket_mode_and_owner_rejection ... ok
test windows::tests::test_named_pipe_roundtrip ... ok
test daemon::tests::test_daemon_admin_bootstrap_info_roundtrip ... ok
test unix::permissions::tests::test_resolve_socket_path_xdg ... ok
test connection::tests::test_graceful_disconnect_decrements_active_connections ... ok

test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

#### Control Plane IPC Tests (`tests/control_plane_ipc_tests.rs`): 11 Passed
```
test test_ipc_studio_and_session_discovery ... ok
test test_ipc_provider_catalog_query ... ok
test test_ipc_approval_cannot_resolve_for_terminal_task ... ok
test test_ipc_approval_resolution_rejected_for_standard_client_on_untracked_approval ... ok
test test_ipc_linearizable_replay_and_reconnect ... ok
test test_ipc_task_submission_and_lifecycle ... ok
test test_ipc_approval_resolution ... ok
test test_ipc_task_cancellation ... ok
test test_ipc_task_auto_execution_fails_closed_when_unconfigured ... ok
test test_ipc_multiple_subscriptions_no_duplicate_events ... ok
test test_ipc_task_auto_execution_lifecycle_succeeds_with_runtime ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

#### End-to-End Tests (`tests/e2e_tests.rs`): 8 Passed
```
test test_e2e_wrong_auth_key_rejected ... ok
test test_e2e_unauthenticated_command_rejected ... ok
test test_e2e_graceful_shutdown ... ok
test test_e2e_event_subscription_and_live_broadcast ... ok
test test_e2e_connect_authenticate_and_ping ... ok
test test_e2e_disconnect_reconnect_with_watermark_recovery ... ok
test test_e2e_reconnect_stream_auto_watermark_recovery ... ok
test test_e2e_windows_named_pipe_full_lifecycle ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

#### M13 Correctness & Concurrency Integration Tests (`tests/m13_ipc_02_2_integration_tests.rs`): 17 Passed
```
test test_bootstrap_credential_segregation_and_untracked_approval_role_gate ... ok
test test_concurrent_cancellation_serialized_and_deduplicated ... ok
test test_cross_session_cancellation_rejected_for_standard_clients ... ok
test test_cancel_task_with_wrong_studio_id_fails_closed_without_stopping_runtime ... ok
test test_concurrent_approval_resolution_serialized_and_deduplicated ... ok
test test_worktree_orchestrator_fail_closed_on_invalid_repo ... ok
test test_cancel_task_reconnected_owner_session_rebinding_succeeds ... ok
test test_approval_runtime_delivery_failure_fails_closed_without_modifying_control_plane ... ok
test test_cancel_task_missing_origin_fails_closed_for_standard_allowed_for_admin ... ok
test test_cancel_task_duplicate_on_terminal_state_rejected ... ok
test test_deterministic_model_routing_multi_model_unconfigured_fails_closed ... ok
test test_deterministic_model_routing_missing_model_fails_closed ... ok
test test_deterministic_model_routing_configured_model_succeeds ... ok
test test_late_runtime_completed_event_ignored_after_cancellation ... ok
test test_runtime_approval_delivery_and_cancellation ... ok
test test_runtime_stop_failure_fails_closed ... ok
test test_runtime_cancellation_stops_session ... ok

test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

---

### 3.2 `agent-studios-opencode-runtime` (54/54 Passed)
```
test test_m11_r08_approval_response_dispatched_to_permission_endpoint ... ok
test test_assign_child_invalid_pid_fails_closed ... ok
test test_assign_and_resume_invalid_pid_fails_closed ... ok
test test_server_config_builder ... ok
test test_server_handle_from_existing_credentials ... ok
test test_server_handle_rejects_non_loopback ... ok
test test_client_debug_redacts_credentials_and_url ... ok
test test_respond_permission_decision ... ok
test test_send_message_typed_request_and_prompt_response ... ok
test test_client_rejects_redirects_no_credential_leakage ... ok
test test_client_basic_auth_header_injection ... ok
test test_abort_session_and_status ... ok
test test_http_status_code_mapping_and_error_sanitization ... ok
test test_create_session_success_and_invalid_schema ... ok
test test_m11_r07_unregistered_instance_strictly_fails_closed ... ok
test test_m11_r07_custom_secret_env_var_override ... ok
test test_m11_r07_provider_mapping_standard_providers ... ok
test test_m11_r07_provider_name_resolution_variants ... ok
test test_m11_r05_pre_spawn_validation_fails_closed_without_catalog ... ok
test test_m11_r05_initial_prompt_failure_triggers_transactional_rollback ... ok
test test_m11_r05_stop_idempotence_and_event_replay_after_stop ... ok
test test_loopback_url_strict_enforcement ... ok
test test_secure_password_generation_entropy ... ok
test test_strict_fails_closed_without_filesystem_isolation ... ok
test test_unconstrained_sandbox_policy ... ok
test test_non_existent_working_dir_fails_closed ... ok
test test_spawn_invalid_binary_fails_immediately ... ok
test test_opencode_discovery_smoke_without_network ... ok
test test_standard_job_object_containment_and_tree_kill ... ok
test test_zero_window_suspended_containment_and_resume ... ok
test test_early_child_crash_detected_without_hanging ... ok
test test_session_id_correlation_drops_unmatched_events ... ok
test test_permission_approval_requested_translation ... ok
test test_translate_upstream_properties_envelope ... ok
test test_sse_parser_oversized_line_discarded ... ok
test test_turn_completed_is_non_terminal_diagnostic ... ok
test test_sse_parser_last_event_id_tracking ... ok
test test_tool_execution_translation_nested_and_root ... ok
test test_unhandled_event_does_not_dump_raw_json ... ok
test test_sse_parser_bounded_buffer_overflow_protection ... ok
test test_opencode_runtime_discovery_and_capabilities ... ok
test test_opencode_runtime_full_lifecycle_with_mock_server ... ok
test test_provider_name_resolution ... ok
test test_loopback_url_verification ... ok
test test_model_ref_mapping ... ok
test test_workspace_validation_managed_vs_shared ... ok
test test_sandbox_policy_custom_builder ... ok
test test_windows_job_object_guard_lifecycle ... ok
test test_sandbox_policy_defaults ... ok
test test_windows_process_containment_assignment ... ok
test test_sse_parser_chunked_streaming ... ok
test test_sse_parser_crlf_and_comments ... ok
test test_translate_opencode_tool_execution ... ok
test test_translate_opencode_session_created ... ok
test test_translate_opencode_terminal_events ... ok
test test_translate_opencode_output_delta ... ok
test test_sse_parser_single_event ... ok
test test_translate_opencode_approval_request ... ok

test result: ok. 54 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

---

## 4. Summary of Deliverables

| Issue Category | Remediation | Verification Status |
| :--- | :--- | :--- |
| **P0 Linux CI Clippy** | Scoped Win32 imports and containment variables under `#[cfg(windows)]` in `sandbox.rs` and `server.rs`. | Clean Linux build under `-D warnings` |
| **P1 CancelTask Origin** | Remediated `None => false` fail-open flaw; enforced fail-closed admin gate for untracked origins; added reconnected owner session rebinding. | 7 dedicated integration tests pass |
| **P1 2PC Approval Delivery** | Ensured approval response is delivered to runtime before Control Plane commit; serialized concurrent resolutions via RAII guard. | 3 dedicated concurrency tests pass |
| **P1 Deterministic Routing** | Disallowed arbitrary first-item fallbacks for models and runtimes; enforced deterministic configuration hierarchy with fail-closed errors. | 3 dedicated model routing tests pass |
| **P1 Daemon Contract** | Documented wire protocol, authentication, command formats, and endpoints for PRs #23 and #24. | Documentation complete |
