# M15.1-A Release Repair: Ubuntu Clippy Failure Forensic & Remediation Report

**Document ID**: `M15-1-A-03-UBUNTU-CLIPPY-ROOT-CAUSE`  
**Date**: 2026-11-06  
**Status**: RESOLVED & SURGICALLY FIXED  
**Failed Runs**: GitHub Actions Run #42059131296 & Run #42070017450  
**Resolution Commits**: `922f119426`, `cde030d0d4`, `1bbb93b0d7`, `5045fd0dc3`, `bc72285b1d`, & `65397d709d`  

---

## 1. Executive Summary

Unified CI runs on GitHub Actions failed during the `Cargo Clippy` step on `ubuntu-latest` runners due to `-D warnings` enforcement against unguarded platform-specific code in `agent-studios-claude-code-runtime`.

Specifically, Process Containment primitives designed for Windows Win32 Job Objects (`JobObjectGuard`) declared imports and variables unconditionally at file scope, resulting in `unused import` and `unused variable` warnings on non-Windows platforms. Furthermore, Clippy identified an un-collapsed nested `if` pattern in Unix signal translation, and an un-asserted `event` binding in a `#[cfg(unix)]` integration test.

All issues have been resolved with strict surgical precision using platform-conditional compilation (`#[cfg(windows)]`), pattern-matching idioms, and explicit test verification without suppressing warnings globally, disabling `-D warnings`, or modifying sandbox security policy.

---

## 2. Raw Diagnostic Log Extraction

Extracted from GitHub Actions Run #42059131296 (Job: `Unified CI (ubuntu-latest)`):

```text
error: unused import: `std::sync::Arc`
 --> crates/claude-code-runtime/src/sandbox.rs:5:5
  |
5 | use std::sync::Arc;
  |     ^^^^^^^^^^^^^^
  |
  = note: `-D unused-imports` implied by `-D warnings`

error: unused import: `info`
 --> crates/claude-code-runtime/src/sandbox.rs:6:17
  |
6 | use tracing::{info, warn};
  |               ^^^^

error: unused variable: `requires_os_containment`
   --> crates/claude-code-runtime/src/process.rs:202:9
    |
202 |     let requires_os_containment = config.sandbox_policy.level.requires_os_containment();
    |         ^^^^^^^^^^^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_requires_os_containment`
    |
    = note: `-D unused-variables` implied by `-D warnings`

error: this `if` statement can be collapsed
   --> crates/claude-code-runtime/src/runtime.rs:751:13
    |
750 |         if let Some(ref s) = status {
751 |             if let Some(sig) = s.signal() {
    |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#collapsible_if
    = note: `-D clippy::collapsible-if` implied by `-D warnings`
```

---

## 3. Forensic Root Cause Analysis

### Issue A: Windows-Specific Imports in `sandbox.rs`
- **Location**: `crates/claude-code-runtime/src/sandbox.rs:5-6`
- **Root Cause**: `Arc` and `tracing::info` were only utilized inside the `JobObjectGuard` struct and standard job-object setup logic, which is gated behind `#[cfg(windows)]`. On Unix, `warn!` was used for unsupported platforms, leaving `Arc` and `info` completely unreferenced.
- **Architectural Impact**: Harmless at runtime, but violates the strict zero-warning policy enforced by CI.

### Issue B: Unguarded Containment Variable in `process.rs`
- **Location**: `crates/claude-code-runtime/src/process.rs:202`
- **Root Cause**: In `spawn_claude_process`, `requires_os_containment` was computed at function entry. On Windows, this boolean determines whether the process is created with `CREATE_SUSPENDED` and bound to the Job Object prior to execution. On Unix platforms, containment uses different OS primitives, and the code path did not reference `requires_os_containment`.
- **Architectural Impact**: Declaring the variable unconditionally triggered rustc's `unused-variables` lint under Unix compilation targets.

### Issue C: Collapsible `if` Chain in `runtime.rs`
- **Location**: `crates/claude-code-runtime/src/runtime.rs:750-756`
- **Root Cause**: Translating Unix exit signals checked `status.is_some()`, then `s.signal()`, then `sig == 2 || sig == 15` across three nested blocks:
  ```rust
  if let Some(ref s) = status {
      if let Some(sig) = s.signal() {
          if sig == 2 || sig == 15 {
              return (RuntimeLifecycleState::Stopped, RuntimeEventKind::Stopped);
          }
      }
  }
  ```
- **Architectural Impact**: Flagged by Clippy's `clippy::collapsible_if` lint.

### Issue D: Unused `event` Binding in `#[cfg(unix)]` Test Block
- **Location**: `crates/claude-code-runtime/tests/runtime_lifecycle_tests.rs:585`
- **Root Cause**: In `test_process_exit_status_wrapper`, the Unix exit status classification test unpacked `let (state, event) = classify_exit_status(Some(status_0), "");`, but only verified `state == Completed` without asserting `event`. Because the block was gated behind `#[cfg(unix)]`, Windows compilation skipped it, but Linux CI flagged it under `-D unused-variables`.
- **Architectural Impact**: Prevented test compilation on Linux targets.

### Issue E: Unused Import `ClaudeCodeError` in `sandbox_containment_tests.rs`
- **Location**: `crates/claude-code-runtime/tests/sandbox_containment_tests.rs:2:5`
- **Root Cause**: In `sandbox_containment_tests.rs`, `ClaudeCodeError` was only referenced in `test_sandbox_strict_fails_closed_on_windows` (`#[cfg(windows)]`). On Linux targets, no test function referenced `ClaudeCodeError`, causing `-D unused-imports` failure.
- **Architectural Impact**: Flagged by Clippy on Linux CI runners.

### Issue F: Unused Import `ProcessContainment` in `opencode-runtime/tests/sandbox_tests.rs`
- **Location**: `crates/opencode-runtime/tests/sandbox_tests.rs:3:38`
- **Root Cause**: `ProcessContainment` was imported at file scope alongside `SandboxLevel` and `SandboxPolicy`, but was only invoked in `test_windows_process_containment_assignment`, which is gated by `#[cfg(windows)]`. On Linux runners, `-D unused-imports` caused Clippy to reject the test suite.
- **Architectural Impact**: Blocked Ubuntu CI Clippy gate.

### Issue G: Fail-Closed Standard Sandbox Invariant in Concurrency Tests
- **Location**: `crates/claude-code-runtime/tests/approval_concurrency_tests.rs:336, 433, 824`
- **Root Cause**: Concurrency tests `test_child_stdin_write_failure_fails_closed_and_not_marked_delivered`, `test_two_phase_commit_stdin_write_failure_transitions_to_delivery_failed`, and `test_watchdog_timeout_stdin_failure_terminates_process_fail_closed` spawned mock child processes (`/bin/sh` or `cmd.exe`) using default `SandboxPolicy::default()`. Because default policy specifies `SandboxLevel::Standard`, and Windows Job Object primitives do not exist on Linux, `ProcessContainment::new` correctly fails closed with `UnsupportedPlatform`. On Windows, the mock process succeeded; on Linux, process spawning failed immediately.
- **Architectural Impact**: Triggered test panic `Process spawn should succeed: UnsupportedPlatform(...)` on Linux test runner while correctly proving fail-closed security enforcement. Mock process tests now explicitly select `SandboxPolicy::unconstrained()` when `!cfg!(windows)`.

---

## 4. Applied Surgical Fixes

### Fix A: `crates/claude-code-runtime/src/sandbox.rs`
```rust
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::sync::Arc;
#[cfg(windows)]
use tracing::info;
use tracing::warn;
```

### Fix B: `crates/claude-code-runtime/src/process.rs`
```rust
pub async fn spawn_claude_process(
    config: ClaudeCodeProcessConfig,
) -> Result<(Arc<ClaudeCodeProcess>, ChildStdout), ClaudeCodeError> {
    #[cfg(windows)]
    let requires_os_containment = config.sandbox_policy.level.requires_os_containment();
    let containment = ProcessContainment::new(config.sandbox_policy.clone())?;
```

### Fix C: `crates/claude-code-runtime/src/runtime.rs`
```rust
pub fn classify_exit_status(
    status: Option<std::process::ExitStatus>,
    stderr: &str,
) -> (RuntimeLifecycleState, RuntimeEventKind) {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(2 | 15) = status.and_then(|s| s.signal()) {
            return (RuntimeLifecycleState::Stopped, RuntimeEventKind::Stopped);
        }
    }
    classify_exit_code(status.and_then(|s| s.code()), stderr)
}
```

### Fix D: `crates/claude-code-runtime/tests/runtime_lifecycle_tests.rs`
```rust
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        let status_0 = std::process::ExitStatus::from_raw(0);
        let (state, event) = classify_exit_status(Some(status_0), "");
        assert_eq!(state, RuntimeLifecycleState::Completed);
        assert_eq!(event, RuntimeEventKind::Completed { summary: None });

        let status_sigint = std::process::ExitStatus::from_raw(2); // SIGINT
        let (state_sig, _) = classify_exit_status(Some(status_sigint), "");
        assert_eq!(state_sig, RuntimeLifecycleState::Stopped);
    }
```

### Fix E: `crates/claude-code-runtime/tests/sandbox_containment_tests.rs`
```rust
#[test]
#[cfg(not(windows))]
fn test_sandbox_strict_and_standard_fail_closed_on_unix() {
    let strict_policy = SandboxPolicy::strict();
    let strict_res = ProcessContainment::new(strict_policy);
    assert!(
        strict_res.is_err(),
        "Strict sandbox must fail closed on non-Windows platforms"
    );
    match strict_res.err().unwrap() {
        ClaudeCodeError::UnsupportedPlatform(msg) => {
            assert!(
                msg.contains("Strict") && msg.contains("unavailable"),
                "Expected UnsupportedPlatform explaining unavailable containment: {msg}"
            );
        }
        other => panic!(
            "Expected ClaudeCodeError::UnsupportedPlatform, got {:?}",
            other
        ),
    }

    let standard_policy = SandboxPolicy::standard();
    let standard_res = ProcessContainment::new(standard_policy);
    assert!(
        standard_res.is_err(),
        "Standard sandbox must fail closed on non-Windows platforms"
    );
    match standard_res.err().unwrap() {
        ClaudeCodeError::UnsupportedPlatform(msg) => {
            assert!(
                msg.contains("Standard") && msg.contains("unavailable"),
                "Expected UnsupportedPlatform explaining unavailable containment: {msg}"
            );
        }
        other => panic!(
            "Expected ClaudeCodeError::UnsupportedPlatform, got {:?}",
            other
        ),
    }
}
```

### Fix F: `crates/opencode-runtime/tests/sandbox_tests.rs`
```rust
#[cfg(windows)]
use agent_studios_opencode_runtime::{JobObjectGuard, ProcessContainment};
use agent_studios_opencode_runtime::{SandboxLevel, SandboxPolicy};
```

### Fix G: `crates/claude-code-runtime/tests/approval_concurrency_tests.rs`
```rust
sandbox_policy: if cfg!(windows) {
    SandboxPolicy::default()
} else {
    SandboxPolicy::unconstrained()
},
```

### Fix H: `crates/opencode-runtime/tests/m11_sandbox_containment_tests.rs`
```rust
.with_sandbox_policy(if cfg!(windows) {
    SandboxPolicy::default()
} else {
    SandboxPolicy::unconstrained()
});
```

Added explicit test `test_standard_fails_closed_on_non_windows_without_job_objects` to verify that `SandboxLevel::Standard` fails closed on non-Windows platforms.

---

## 5. Verification & Security Assessment

1. **Zero Lint Suppression**: No `#![allow(...)]` or compiler flags were modified. All crates continue to be checked with `-- -D warnings`.
2. **Sandbox Integrity**: The Windows Job Object isolation logic (`JobObjectGuard`) remains fully intact with identical CPU, memory limits, and kill-on-close semantics.
3. **Cross-Platform Parity**: The code compiles cleanly on both Linux (Ubuntu) and Windows targets.
