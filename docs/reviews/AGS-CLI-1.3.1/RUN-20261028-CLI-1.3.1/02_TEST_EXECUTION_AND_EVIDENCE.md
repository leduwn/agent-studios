# Milestone AGS CLI 1.3.1 — Test Execution & Evidence Ledger

- **Milestone**: `AGS CLI 1.3.1 Quality Gate & Correctness Audit`
- **Execution Run**: `RUN-20261028-CLI-1.3.1`
- **Date**: `2026-10-28T16:05:00+07:00`
- **Platform**: `Windows 11 Home Single Language 10.0.26200 (x86_64-pc-windows-msvc)`
- **Rust Toolchain**: `stable-x86_64-pc-windows-msvc`
- **Status**: **ALL TESTS GREEN (47/47 PASSING)**

---

## 1. Test Suite Results Summary

| Crate | Suite | Tests Passed | Tests Failed | Ignored | Total Duration |
|---|---|:---:|:---:|:---:|:---:|
| `crates/cli-client` | `client_tests.rs` | 4 | 0 | 0 | 6.33s |
| `crates/cli-client` | `runtime_slice_tests.rs` | 10 | 0 | 0 | 67.86s |
| `crates/cli-client` | `subscription_tests.rs` | 7 | 0 | 0 | 401.80s |
| `crates/cli-client` | `worktree_isolation_tests.rs` | 3 | 0 | 0 | 3.92s |
| `crates/tui` | `widget_tests.rs` | 23 | 0 | 0 | 1.15s |
| **Total** | | **47** | **0** | **0** | **481.06s** |

---

## 2. Cli-Client Test Execution Evidence

### 2.1. Worktree Isolation & Dirty Source Preservation (`worktree_isolation_tests.rs`)
```text
running 3 tests
test test_embedded_client_provisions_real_worktree_record ... ok
test test_physical_worktree_isolation_and_dirty_source_preservation ... ok
test test_worktree_lease_ownership_and_retention ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.92s
```

Key verified properties:
1. `test_physical_worktree_isolation_and_dirty_source_preservation`:
   - `assert_ne!(managed.root, managed.source_root)` passed.
   - `assert_eq!(managed.head_sha, base_sha)` passed.
   - Untracked file `dirty_file.txt` in source is NOT present in worktree (`assert!(!managed.root.join("dirty_file.txt").exists())`).
   - Tracked file `file_in_main.txt` in worktree has base content, while dirty uncommitted edit in source remains intact.
2. `test_worktree_lease_ownership_and_retention`:
   - Worktree thread binding and lease ownership checks.
   - Releasing owned worktree fails closed.
   - Retained worktree remains on disk.
3. `test_embedded_client_provisions_real_worktree_record`:
   - `EmbeddedClient::submit_goal` allocates genuine physical worktree via `WorkspaceOrchestrator`.
   - Control Plane record contains correct `base_sha` and `root != source_dir`.

### 2.2. Runtime Slice & Approval Routing Tests (`runtime_slice_tests.rs`)
```text
running 10 tests
test test_approval_routing_duplicate_response_fails_closed ... ok
test test_approval_routing_session_terminated_fails_closed ... ok
test test_cancellation_stops_active_runtime_sessions ... ok
test test_concurrent_sessions_isolated_approval_routing ... ok
test test_production_mode_fails_closed_when_unconfigured ... ok
test test_prompt_submission_to_runtime_dispatch_and_streaming_completion ... ok
test test_resolve_approval_fails_closed_without_active_session ... ok
test test_runtime_failure_propagation_to_control_plane ... ok
test test_truthful_reporting_deterministic_local_runtime ... ok
test test_ui_event_translation_for_runtime_streaming ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 67.86s
```

### 2.3. Event Stream Linearizability Tests (`subscription_tests.rs`)
```text
running 7 tests
test test_channel_shutdown_and_lag_recovery ... ok
test test_simultaneous_multiple_subscribers ... ok
test test_subscription_backfill_and_stream_deduplication ... ok
test test_subscription_broadcast_lag_recovery_zero_event_loss ... ok
test test_subscription_future_sequence_filtering ... ok
test test_subscription_sequence_gap_recovery_from_history ... ok
test test_subscription_unrecoverable_gap_preserves_terminal_events ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 401.80s
```

### 2.4. Basic Client Operations (`client_tests.rs`)
```text
running 4 tests
test test_get_worktree_diff_fails_closed_unsupported ... ok
test test_reconcile_patch_fails_closed_unsupported ... ok
test test_submit_goal_and_task_graph ... ok
test test_workspace_name_and_info ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.33s
```

---

## 3. TUI Widget Test Execution Evidence

```text
running 23 tests
test tests::test_assistant_chunk_appends_to_current_message ... ok
test tests::test_assistant_message_preserves_code_fences_across_chunks ... ok
test tests::test_chunk_appends_to_previous_text ... ok
test tests::test_diff_stat_calculation_matches_line_changes ... ok
test tests::test_diff_widget_clamps_scroll_offset ... ok
test tests::test_diff_widget_renders_unified_syntax ... ok
test tests::test_event_to_message_conversion_preserves_ordering ... ok
test tests::test_footer_renders_contextual_keybindings ... ok
test tests::test_input_history_navigation_preserves_current_buffer ... ok
test tests::test_input_widget_handles_empty_submission ... ok
test tests::test_input_widget_nfc_tone_mark_combining ... ok
test tests::test_input_widget_renders_cursor_at_end ... ok
test tests::test_markdown_parser_handles_empty_input ... ok
test tests::test_markdown_parser_plain_text ... ok
test tests::test_markdown_table_rendering ... ok
test tests::test_multiline_paste_preserves_line_breaks ... ok
test tests::test_onboarding_widget_renders_options ... ok
test tests::test_render_does_not_panic ... ok
test tests::test_status_bar_displays_context_percentage ... ok
test tests::test_status_bar_displays_workspace_path_cleanly ... ok
test tests::test_status_bar_renders_demo_mode_truthfully ... ok
test tests::test_transcript_auto_scrolls_to_bottom ... ok
test tests::test_transcript_retains_events_up_to_capacity ... ok

test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.15s
```

---

## 4. Lint & Formatting Verification

### 4.1. Workspace Clippy Gate
```bash
cargo clippy --workspace --all-targets -- -D warnings
```
**Exit Code**: 0  
**Output**: Clean (0 warnings, 0 errors).

### 4.2. Workspace Rustfmt Gate
```bash
cargo fmt --all -- --check
```
**Exit Code**: 0  
**Output**: Clean (all files formatted according to rustfmt standards).

---

## 5. CLI Binary Smoke Tests

1. `ags --help`:
   - Returns exit code 0.
   - Correctly renders subcommands: `run`, `doctor`, `worktree`, `task`, `config`.
2. `ags doctor`:
   - Returns exit code 0.
   - Accurately checks local Git installation, Claude Code CLI runtime discovery, OpenCode environment, and local workspace directory health.
