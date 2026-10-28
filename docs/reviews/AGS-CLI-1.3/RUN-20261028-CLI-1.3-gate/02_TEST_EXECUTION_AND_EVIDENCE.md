# Milestone AGS CLI 1.3 — Test Execution & Evidence Ledger

- **Milestone**: `AGS CLI 1.3 Security & Reliability Gate + M13 Preparation`
- **Execution Run**: `RUN-20261028-CLI-1.3-gate`
- **Date**: `2026-10-28T07:20:00+07:00`
- **Lead Owner**: Security QA Coordinator & AI Runtime Architect
- **Target Workspace**: `D:\Projects\agent-studios-ags-cli\agent-studios-rs`
- **Platform**: `Windows 11 Home (x86_64)`

---

## 1. Fresh Test Suite Execution Summary (PR #16)

```text
======================================================================
     Running unittests src\main.rs (crates/cli)
test doctor::tests::test_detect_color_support ... ok
test doctor::tests::test_doctor_diagnostics_run ... ok
test result: ok. 2 passed; 0 failed; finished in 0.22s

======================================================================
     Running tests\client_tests.rs (crates/cli-client)
test test_get_worktree_diff_fails_closed_unsupported ... ok
test test_reconcile_patch_fails_closed_unsupported ... ok
test test_submit_goal_and_task_graph ... ok
test test_workspace_name_and_info ... ok
test result: ok. 4 passed; 0 failed; finished in 0.12s

======================================================================
     Running tests\runtime_slice_tests.rs (crates/cli-client)
test test_truthful_reporting_deterministic_local_runtime ... ok
test test_resolve_approval_fails_closed_without_active_session ... ok
test test_ui_event_translation_for_runtime_streaming ... ok
test test_cancellation_stops_active_runtime_sessions ... ok
test test_production_mode_fails_closed_when_unconfigured ... ok
test test_runtime_failure_propagation_to_control_plane ... ok
test test_concurrent_sessions_isolated_approval_routing ... ok
test test_prompt_submission_to_runtime_dispatch_and_streaming_completion ... ok
test result: ok. 8 passed; 0 failed; finished in 0.06s

======================================================================
     Running tests\subscription_tests.rs (crates/cli-client)
test test_subscription_unrecoverable_gap_preserves_terminal_events ... ok
test test_subscription_sequence_gap_recovery_from_history ... ok
test test_subscription_backfill_and_stream_deduplication ... ok
test test_subscription_future_sequence_filtering ... ok
test test_simultaneous_multiple_subscribers ... ok
test test_subscription_broadcast_lag_recovery_zero_event_loss ... ok
test test_channel_shutdown_and_lag_recovery ... ok
test result: ok. 7 passed; 0 failed; finished in 0.07s

======================================================================
     Running tests\provider_and_workspace_tests.rs (crates/opencode-runtime)
test test_provider_name_resolution ... ok
test test_loopback_url_verification ... ok
test test_model_ref_mapping ... ok
test test_workspace_validation_managed_vs_shared ... ok
test result: ok. 4 passed; 0 failed; finished in 0.01s

======================================================================
     Running tests\m11_approval_authority_tests.rs (crates/opencode-runtime)
test test_m11_r08_approval_response_dispatched_to_permission_endpoint ... ok
test result: ok. 1 passed; 0 failed; finished in 0.17s

======================================================================
     Running tests\m11_process_supervision_tests.rs (crates/opencode-runtime)
test test_assign_and_resume_invalid_pid_fails_closed ... ok
test test_server_config_builder ... ok
test test_assign_child_invalid_pid_fails_closed ... ok
test test_server_handle_from_existing_credentials ... ok
test test_server_handle_rejects_non_loopback ... ok
test result: ok. 5 passed; 0 failed; finished in 0.00s

======================================================================
     Running tests\m11_protocol_tests.rs (crates/opencode-runtime)
test test_client_debug_redacts_credentials_and_url ... ok
test test_client_rejects_redirects_no_credential_leakage ... ok
test test_respond_permission_decision ... ok
test test_send_message_typed_request_and_prompt_response ... ok
test test_client_basic_auth_header_injection ... ok
test test_abort_session_and_status ... ok
test test_create_session_success_and_invalid_schema ... ok
test test_http_status_code_mapping_and_error_sanitization ... ok
test result: ok. 8 passed; 0 failed; finished in 0.01s

======================================================================
     Running tests\m11_provider_mapping_tests.rs (crates/opencode-runtime)
test test_m11_r07_provider_name_resolution_variants ... ok
test test_m11_r07_unregistered_instance_strictly_fails_closed ... ok
test test_m11_r07_custom_secret_env_var_override ... ok
test test_m11_r07_provider_mapping_standard_providers ... ok
test result: ok. 4 passed; 0 failed; finished in 0.00s

======================================================================
     Running tests\m11_runtime_lifecycle_tests.rs (crates/opencode-runtime)
test test_m11_r05_pre_spawn_validation_fails_closed_without_catalog ... ok
test test_m11_r05_initial_prompt_failure_triggers_transactional_rollback ... ok
test test_m11_r05_stop_idempotence_and_event_replay_after_stop ... ok
test result: ok. 3 passed; 0 failed; finished in 0.17s

======================================================================
     Running tests\m11_sandbox_containment_tests.rs (crates/opencode-runtime)
test test_strict_fails_closed_without_filesystem_isolation ... ok
test test_loopback_url_strict_enforcement ... ok
test test_unconstrained_sandbox_policy ... ok
test test_secure_password_generation_entropy ... ok
test test_opencode_discovery_smoke_without_network ... ok
test test_non_existent_working_dir_fails_closed ... ok
test test_spawn_invalid_binary_fails_immediately ... ok
test test_zero_window_suspended_containment_and_resume ... ok
test test_standard_job_object_containment_and_tree_kill ... ok
test test_early_child_crash_detected_without_hanging ... ok
test result: ok. 10 passed; 0 failed; finished in 2.18s

======================================================================
     Running tests\m11_sse_tests.rs (crates/opencode-runtime)
test test_session_id_correlation_drops_unmatched_events ... ok
test test_sse_parser_last_event_id_tracking ... ok
test test_permission_approval_requested_translation ... ok
test test_sse_parser_oversized_line_discarded ... ok
test test_tool_execution_translation_nested_and_root ... ok
test test_turn_completed_is_non_terminal_diagnostic ... ok
test test_unhandled_event_does_not_dump_raw_json ... ok
test test_translate_upstream_properties_envelope ... ok
test test_sse_parser_bounded_buffer_overflow_protection ... ok
test result: ok. 9 passed; 0 failed; finished in 0.00s

======================================================================
     Running tests\mock_server_tests.rs (crates/opencode-runtime)
test test_opencode_runtime_discovery_and_capabilities ... ok
test test_opencode_runtime_full_lifecycle_with_mock_server ... ok
test result: ok. 2 passed; 0 failed; finished in 0.21s

======================================================================
     Running tests\sandbox_tests.rs (crates/opencode-runtime)
test test_sandbox_policy_defaults ... ok
test test_sandbox_policy_custom_builder ... ok
test test_windows_job_object_guard_lifecycle ... ok
test test_windows_process_containment_assignment ... ok
test result: ok. 4 passed; 0 failed; finished in 0.04s

======================================================================
     Running tests\sse_tests.rs (crates/opencode-runtime)
test test_sse_parser_single_event ... ok
test test_sse_parser_crlf_and_comments ... ok
test test_translate_opencode_session_created ... ok
test test_translate_opencode_approval_request ... ok
test test_translate_opencode_terminal_events ... ok
test test_translate_opencode_tool_execution ... ok
test test_translate_opencode_output_delta ... ok
test test_sse_parser_chunked_streaming ... ok
test result: ok. 8 passed; 0 failed; finished in 0.00s

======================================================================
     Running tests\terminal_tests.rs (crates/tui)
test test_terminal_title_ansi_neutralization ... ok
test test_terminal_title_control_codes_stripped ... ok
test test_terminal_title_length_bounding ... ok
test test_terminal_breakpoint_classification ... ok
test test_terminal_title_trojan_source_bidi_stripped ... ok
test test_restore_raw_terminal_idempotency ... ok
test test_centered_rect_centering_and_proportions ... ok
test test_main_layout_geometry_by_breakpoint ... ok
test result: ok. 8 passed; 0 failed; finished in 0.01s

======================================================================
     Running tests\widget_tests.rs (crates/tui)
test test_diff_viewer_buffer_snapshot ... ok
test test_slash_menu_commands_inventory ... ok
test test_status_bar_widget_tokens_and_status_buffer_snapshot ... ok
test test_terminal_title_sanitization ... ok
test test_header_widget_fallbacks_buffer_snapshot ... ok
test test_approval_modal_buffer_snapshot ... ok
test test_theme_tokens_contrast ... ok
test test_centered_rect_math ... ok
test test_slash_menu_annotations_and_inventory_buffer_snapshot ... ok
test test_vietnamese_unicode_nfc_grapheme_integrity ... ok
test test_responsive_layout_breakpoints ... ok
test test_widgets_render_without_panics ... ok
test test_app_action_error_handling_no_silent_drop ... ok
test test_app_fail_closed_approval_error_preserves_modal ... ok
test test_app_slash_commands_and_planned_capabilities ... ok
test result: ok. 15 passed; 0 failed; finished in 0.23s

======================================================================
PR #16 Total Tests: 98 passed; 0 failed; 0 ignored
```

---

## 2. Independent Verification of PR #15 (M12.3 at `3207b7462f`)

```text
cargo test -p agent-studios-claude-code-runtime
Result: 74 passed; 0 failed; 0 ignored; finished in 2.76s

cargo clippy -p agent-studios-claude-code-runtime -- -D warnings
Result: 0 errors, 0 warnings
```

---

## 3. Clippy Compliance (PR #16)

```text
cargo clippy -p agent-studios-cli -p agent-studios-cli-client -p agent-studios-tui -p agent-studios-opencode-runtime -- -D warnings
Result: 0 errors, 0 warnings
```

---

## 4. AGS Doctor Diagnostic Verification

```text
============================================================
        AGENT STUDIOS CLI (ags) — SYSTEM DOCTOR REPORT     
============================================================
Version:    ags v0.1.0
Platform:   windows (x86_64)
Repo Root:  D:/Projects/agent-studios-ags-cli
Worktree:   Dirty
Terminal:   120 columns x 30 rows
------------------------------------------------------------
[OK]   Platform                               : windows (x86_64, windows)
[OK]   Git Executable                         : git version 2.56.0.windows.3
[OK]   Git Repository & Worktree              : Root: D:/Projects/agent-studios-ags-cli | Status: dirty worktree (19 uncommitted changes)
[OK]   Terminal Geometry                      : 120x30 (Meets recommended Standard geometry >= 100x28)
[INFO] Color & ANSI Capabilities              : NO_COLOR environment variable active; monochrome fallback enforced
[OK]   Unicode NFC & Vietnamese Tone Pipeline : Verified 69 grapheme clusters with intact NFC tone marks and zero diacritic clipping
[OK]   Control Plane Engine                   : Deterministic in-process engine ready (InMemoryStore + SystemClock, initialized 4 default agent roles)
------------------------------------------------------------
All core subsystem health checks completed.
```
