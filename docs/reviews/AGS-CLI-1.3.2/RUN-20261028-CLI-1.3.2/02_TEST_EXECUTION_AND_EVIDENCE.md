# Milestone AGS CLI 1.3.2 — Test Execution & Evidence Ledger

- **Milestone**: `AGS CLI 1.3.2 CI Hotfix Verification`
- **Execution Run**: `RUN-20261028-CLI-1.3.2`
- **Date**: `2026-10-28T17:35:00+07:00`
- **Branch**: `feat/ags-cli-tui-foundation`
- **Platform**: `Windows 11 Home Single Language 10.0.26200 (x86_64-pc-windows-msvc)`
- **Toolchain**: `stable-x86_64-pc-windows-msvc (rustc 1.85.0 / 2024 edition)`
- **Status**: **ALL TESTS GREEN (100% PASSING)**

---

## 1. Test Suite Verification Summary

| Test Suite / Target | Command | Result | Pass/Fail Count | Execution Duration |
|---|---|:---:|:---:|:---:|
| **Client Tests** | `cargo test --test client_tests` | **PASS** | 4 / 4 | 1.53s |
| **Subscription Tests** | `cargo test --test subscription_tests` | **PASS** | 7 / 7 | 13.10s |
| **Worktree Isolation Tests** | `cargo test --test worktree_isolation_tests` | **PASS** | 3 / 3 | 6.61s |
| **Runtime Slice Tests** | `cargo test --test runtime_slice_tests` | **PASS** | 10 / 10 | 33.45s |
| **OpenCode Runtime Suite** | `cargo test -p agent-studios-opencode-runtime` | **PASS** | 50 / 50 | 2.80s |
| **TUI Suite** | `cargo test -p agent-studios-tui` | **PASS** | 23 / 23 | 0.96s |
| **Clippy Linter** | `cargo clippy --workspace --all-targets -- -D warnings` | **PASS** | 0 warnings | 1.79s |
| **Rustfmt** | `rustfmt --edition 2024 --check <files>` | **PASS** | Clean format | <0.5s |
| **AGS CLI Binary Build** | `cargo build --bin ags` | **PASS** | Binary produced | 19.54s |
| **AGS Doctor Smoke Test** | `target/debug/ags.exe doctor` | **PASS** | All checks OK | <0.5s |
| **AGS Help Smoke Test** | `target/debug/ags.exe --help` | **PASS** | Valid CLI menu | <0.5s |

---

## 2. Detailed Execution Outputs

### 2.1. Client Tests (`client_tests.rs`)
```text
running 4 tests
test test_get_worktree_diff_fails_closed_unsupported ... ok
test test_reconcile_patch_fails_closed_unsupported ... ok
test test_workspace_name_and_info ... ok
test test_submit_goal_and_task_graph ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.53s
```

### 2.2. Subscription Tests (`subscription_tests.rs`)
```text
running 7 tests
test test_subscription_unrecoverable_gap_preserves_terminal_events ... ok
test test_subscription_sequence_gap_recovery_from_history ... ok
test test_subscription_backfill_and_stream_deduplication ... ok
test test_subscription_future_sequence_filtering ... ok
test test_simultaneous_multiple_subscribers ... ok
test test_channel_shutdown_and_lag_recovery ... ok
test test_subscription_broadcast_lag_recovery_zero_event_loss ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.10s
```

### 2.3. Worktree Isolation Tests (`worktree_isolation_tests.rs`)
```text
running 3 tests
test test_embedded_client_provisions_real_worktree_record ... ok
test test_physical_worktree_isolation_and_dirty_source_preservation ... ok
test test_worktree_lease_ownership_and_retention ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.61s
```

### 2.4. OpenCode Runtime Integration Suite (`crates/opencode-runtime`)
```text
running 50 tests across 11 test suites:
- m11_approval_authority_tests: 1 passed
- m11_process_supervision_tests: 5 passed
- m11_protocol_tests: 8 passed
- m11_provider_mapping_tests: 4 passed
- m11_runtime_lifecycle_tests: 3 passed
- m11_sandbox_containment_tests: 10 passed
- m11_sse_tests: 9 passed
- mock_server_tests: 2 passed
- provider_and_workspace_tests: 4 passed
- sandbox_tests: 4 passed
- sse_tests: 8 passed

test result: ok. 50 passed; 0 failed; 0 ignored; 0 measured; finished in 2.80s
```

### 2.5. AGS CLI System Doctor Smoke Verification (`ags doctor`)
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
[OK]   Git Repository & Worktree              : Root: D:/Projects/agent-studios-ags-cli | Status: dirty worktree (6 uncommitted changes)
[OK]   Terminal Geometry                      : 120x30 (Meets recommended Standard geometry >= 100x28)
[INFO] Color & ANSI Capabilities              : NO_COLOR environment variable active; monochrome fallback enforced
[OK]   Unicode NFC & Vietnamese Tone Pipeline : Verified 69 grapheme clusters with intact NFC tone marks and zero diacritic clipping
[OK]   Control Plane Engine                   : Deterministic in-process engine ready (InMemoryStore + SystemClock, initialized 4 default agent roles)
------------------------------------------------------------
All core subsystem health checks completed.
```

---

## 3. Remote GitHub Actions CI Verification

- **Workflow**: `Agent Studios Rust CI` (`.github/workflows/agent-studios-rust-ci.yml`)
- **Target Commit**: `215ffd37243c22ad81b53c15383a158b753aefb5`
- **Pull Request**: PR #16 (`https://github.com/leduwn/agent-studios/pull/16`)
- **PR Run ID**: `40692576580` (`https://github.com/leduwn/agent-studios/actions/runs/40692576580`)
- **Push Run ID**: `40692565680` (`https://github.com/leduwn/agent-studios/actions/runs/40692565680`)
- **Overall Status**: **TOTAL SUCCESS (100% GREEN ACROSS UBUNTU & WINDOWS)**

### 3.1. Matrix Job Results (Run 40692576580)

| Job | OS Platform | Status | Conclusion | Key Verified Steps |
|---|---|:---:|:---:|---|
| **Rust Quality Gate (ubuntu-latest)** | `ubuntu-latest` | completed | **success** | Check Formatting, Cargo Check, Cargo Clippy, Cargo Test, Build AGS CLI Binary, Deterministic Smoke Tests |
| **Rust Quality Gate (windows-latest)** | `windows-latest` | completed | **success** | Cargo Check, Cargo Clippy, Cargo Test, Build AGS CLI Binary, Deterministic Smoke Tests |

### 3.2. Matrix Job Results (Run 40692565680)

| Job | OS Platform | Status | Conclusion | Key Verified Steps |
|---|---|:---:|:---:|---|
| **Rust Quality Gate (ubuntu-latest)** | `ubuntu-latest` | completed | **success** | Check Formatting, Cargo Check, Cargo Clippy, Cargo Test, Build AGS CLI Binary, Deterministic Smoke Tests |
| **Rust Quality Gate (windows-latest)** | `windows-latest` | completed | **success** | Cargo Check, Cargo Clippy, Cargo Test, Build AGS CLI Binary, Deterministic Smoke Tests |

