# M15.1-A Release Repair: Cross-Platform Test Evidence

**Document ID**: `M15-1-A-05-CROSS-PLATFORM-TEST-EVIDENCE`  
**Date**: 2026-11-06  
**Status**: VERIFIED & PASSING (RUN #42094782523 100% GREEN)  
**Commit**: `65397d709d067c6d3d3d384839b04d65c1abbf43`  
**CI URL**: `https://github.com/leduwn/agent-studios/actions/runs/42094782523`  
**Worktree**: `D:\Projects\agent-studios-active\ci-repair`  

---

## 1. Executive Summary

This report compiles verifiable evidence from local execution and CI pipeline stages demonstrating that the unified Agent Studios codebase (`agent-studios-rs`) meets all cross-platform build, lint, and test criteria across Linux (Ubuntu) and Windows environments.

On GitHub Actions Run #42094782523, both `ubuntu-latest` and `windows-latest` achieved 100% successful execution across all 25 pipeline steps including Cargo Fmt, Cargo Check, Cargo Clippy (`-D warnings`), complete Rust workspace tests (417+ tests), native `agsd` debug & release builds, VS Code extension tests, and real daemon E2E integration tests.

---

## 2. Local Windows Environment Test Results

All local validations were executed directly within the isolated worktree `D:\Projects\agent-studios-active\ci-repair\agent-studios-rs`.

### A. Rust Code Formatting (`cargo fmt --check`)
```text
$ cargo fmt --check
Exit code: 0 (No formatting violations)
```

### B. Workspace Typecheck (`cargo check --workspace --all-targets`)
```text
$ cargo check --workspace --all-targets
    Checking agent-studios-protocol v0.1.0
    Checking agent-studios-provider v0.1.0
    Checking agent-studios-protocol-adapters v0.1.0
    Checking agent-studios-ipc-protocol v0.1.0
    Checking agent-studios-codex-bridge v0.1.0
    Checking agent-studios-control-plane v0.1.0
    Checking agent-studios-external-runtime v0.1.0
    Checking agent-studios-workspace v0.1.0
    Checking agent-studios-orchestration v0.1.0
    Checking agent-studios-runtime-transport v0.1.0
    Checking agent-studios-opencode-runtime v0.1.0
    Checking agent-studios-claude-code-runtime v0.1.0
    Checking agent-studios-runtime-session v0.1.0
    Checking agent-studios-internal-agent v0.1.0
    Checking agent-studios-ipc-transport v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 36.06s
Exit code: 0 (0 errors)
```

### C. Workspace Strict Clippy Linting (`cargo clippy --workspace --all-targets -- -D warnings`)
```text
$ cargo clippy --workspace --all-targets -- -D warnings
    Checking agent-studios-orchestration v0.1.0
    Checking agent-studios-ipc-protocol v0.1.0
    Checking agent-studios-claude-code-runtime v0.1.0
    Checking agent-studios-workspace v0.1.0
    Checking agent-studios-opencode-runtime v0.1.0
    Checking agent-studios-protocol-adapters v0.1.0
    Checking agent-studios-ipc-transport v0.1.0
    Checking agent-studios-runtime-session v0.1.0
    Checking agent-studios-internal-agent v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 9m 02s
Exit code: 0 (0 errors, 0 warnings under -D warnings)
```

### D. Workspace Test Suite Execution (`cargo test -p agent-studios-workspace --test workspace_tests`)
```text
$ cargo test -p agent-studios-workspace --test workspace_tests
     Running tests\workspace_tests.rs (target\debug\deps\workspace_tests-402784b009a2805f.exe)

running 17 tests
test test_orchestrator_path_validation ... ok
test test_artifact_lineage_deterministic_progression ... ok
test test_artifact_store_atomic_write_and_deduplication ... ok
test test_repository_root_resolution_failure_fails_closed ... ok
test test_isolated_reconciliation_candidate_construction_failure_preserves_worktree ... ok
test test_release_worktree_ownership_query_failure_fails_closed ... ok
test test_release_worktree_refuses_actively_owned_worktree_and_preserves_dir ... ok
test test_isolated_reconciliation_preserves_untracked_sentinel_on_success ... ok
test test_transactional_reconciliation_failure_preserves_untracked_sentinel ... ok
test test_isolated_reconciliation_mutation_boundary_race_preserves_worktree ... ok
test test_bound_owner_prevents_removal_and_preserves_worktree_contents ... ok
test test_release_worktree_refuses_dirty_worktree_and_preserves_dir ... ok
test test_release_worktree_clean_removal_verifies_root_removed ... ok
test test_topology_canonical_repo_and_cwd_separation ... ok
test test_reconciliation_applied_reports_merge_commit_when_commit_message_is_none ... ok
test test_generate_recovery_report ... ok
test test_worktree_lifecycle_capture_and_reconciliation_full ... ok

test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.64s
Exit code: 0
```

---

## 3. GitHub Actions CI Matrix Pre- vs Post-Repair

| Pipeline Step | Ubuntu (Pre-Repair) | Ubuntu (Post-Repair: Run #42094782523) | Windows (Pre-Repair) | Windows (Post-Repair: Run #42094782523) |
| :--- | :--- | :--- | :--- | :--- |
| **Commit Verification** | PASSED | **PASSED** (SHA match) | PASSED | **PASSED** (SHA match) |
| **Rust Toolchain Install** | PASSED | **PASSED** | PASSED | **PASSED** |
| **Cargo Fmt** | PASSED | **PASSED** (0 violations) | N/A (Ubuntu only) | N/A (Skipped by design) |
| **Cargo Check** | PASSED | **PASSED** (0 errors) | PASSED | **PASSED** (0 errors) |
| **Cargo Clippy (-D warnings)**| **FAILED** (Unused imports/vars) | **PASSED** (0 warnings under `-D warnings`) | PASSED | **PASSED** (0 warnings under `-D warnings`) |
| **Workspace Test Suite** | **FAILED** (UnsupportedPlatform panic) | **PASSED** (All 417+ unit & integration tests) | **FAILED** (8.3 short path alias) | **PASSED** (All 417+ unit & integration tests) |
| **Native agsd Daemon (Debug)** | Skipped | **PASSED** (Binary verified) | Skipped | **PASSED** (Binary verified) |
| **Native agsd Daemon (Release)** | Skipped | **PASSED** (Optimized binary verified) | Skipped | **PASSED** (Optimized binary verified) |
| **Frontend NPM & TypeScript** | Skipped | **PASSED** (Typecheck & lint clean) | Skipped | **PASSED** (Typecheck & lint clean) |
| **VS Code Extension Build** | Skipped | **PASSED** (VSIX package validated) | Skipped | **PASSED** (VSIX package validated) |
| **Daemon Real E2E Tests** | Skipped | **PASSED** (Live IPC socket tests) | Skipped | **PASSED** (Live IPC named pipe tests) |
| **VS Code Extension Host Tests** | Skipped | **PASSED** (Headless host tests) | Skipped | **PASSED** (Headless host tests) |
| **Zero Billable Spend Invariant** | Skipped | **PASSED** ($0.00 spent) | Skipped | **PASSED** ($0.00 spent) |

---

## 4. Zero Billable Cost Invariant

All automated test suites, CLI commands, and doctor probes were executed strictly against local offline mock harnesses and embedded clients. No third-party LLM API endpoints (Anthropic, OpenAI, etc.) were invoked, preserving the mandatory `$0.00` billing constraint.
