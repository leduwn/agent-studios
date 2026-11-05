# Agent Studios — Milestone M15.1 Joint Integration Preflight
## Document 03: CI Repair Gate Evidence & Platform Containment Hardening

**Milestone:** M15.1 (Joint Integration Preflight & Cross-Platform Release Gate)  
**Date:** 2026-11-06  
**Role:** Principal Integration Engineer & Release QA Lead  
**PR Under Audit:** PR #33 (`fix/m15-unified-cross-platform-ci`)  
**Base Target:** `integration/m15-unified-codebase`  
**Status:** CI REPAIR GATE AUDITED & HARDENED  

---

### 1. CI Repair Gate Mandate & Objectives

Before any joint integration verdict can be rendered for Milestone M15.1, the underlying cross-platform CI matrix on PR #33 must be evaluated. The baseline repository suffered from historical CI failures across both Ubuntu and Windows runners:
1. **Ubuntu Clippy Failures:** Unused variables, dead code, and strict `-D warnings` enforcement failures in backend Rust crates.
2. **Linux OS Sandbox Fail-Closed Panics:** Tests exercising child process spawning and crash detection failed on Linux runners because `SandboxLevel::Standard` strictly requires Windows Job Objects and fails closed with `UnsupportedPlatform`.
3. **Windows 8.3 Short Path Canonicalization:** GitHub Actions Windows runners utilize 8.3 short names (e.g. `C:\Users\RUNNER~1\AppData\...`), causing path equality assertions in workspace tests to fail against expanded paths.

---

### 2. CI Run Chronology & Evolution

| Run ID | Commit SHA | Trigger Event | Ubuntu Status | Windows Status | Root Cause / Outcome |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **42081071268** | `7c8014dc52` | `pull_request` | **FAILED** (Clippy) | **CANCELLED** | Unused variables and dead code in backend crates under `-D warnings`. |
| **42088555963** | `5045fd0dc3` | `pull_request` | **FAILED** (Tests) | **CANCELLED** | `approval_concurrency_tests.rs` panicked: `SandboxLevel::Standard requires OS-level containment`. |
| **42091275576** | `bc72285b1d` | `pull_request` | **FAILED** (Tests) | **SUCCESS** (Tests passed) | Windows passed 100% of Rust tests and built `agsd.exe`. Ubuntu failed only on 2 tests in `m11_sandbox_containment_tests.rs`. |
| **42094782523** | `b0696ace58` | `pull_request` | **IN PROGRESS** | **IN PROGRESS** | Both `claude-code-runtime` and `opencode-runtime` tests gated. |

---

### 3. Root Cause Investigation & Architectural Fixes

#### 3.1 Linux OS Containment & Fail-Closed Platform Security
- **The Invariant:** On Windows, `SandboxLevel::Standard` and `SandboxLevel::Strict` leverage Win32 Job Objects (`JobObjectGuard`) with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, process count limits, and memory quotas. On non-Windows platforms (such as Linux CI runners), Windows Job Objects do not exist and cgroups are not yet bound. The sandbox architecture is intentionally designed to **fail closed**:
  ```rust
  #[cfg(not(windows))]
  SandboxLevel::Strict | SandboxLevel::Standard => {
      Err(OpenCodeError::UnsupportedPlatform(format!(
          "Sandbox level {:?} requires OS-level containment. On this platform, Windows Job Objects are unavailable and Linux cgroups are not yet configured. Failing closed.",
          policy.level
      )))
  }
  ```
- **The Bug:** In `crates/claude-code-runtime/tests/approval_concurrency_tests.rs` and `crates/opencode-runtime/tests/m11_sandbox_containment_tests.rs`, unit tests exercising dummy binary spawning (`/bin/sh` or `cmd.exe`) initialized configurations with `SandboxPolicy::default()` (which defaults to `SandboxLevel::Standard`). On Linux, this immediately triggered the fail-closed error, failing tests that were intended to test process I/O or exit detection.
- **The Remediation:**
  1. In `approval_concurrency_tests.rs`: Configured process configs to use `if cfg!(windows) { SandboxPolicy::default() } else { SandboxPolicy::unconstrained() }`.
  2. In `m11_sandbox_containment_tests.rs`: Configured `test_spawn_invalid_binary_fails_immediately` and `test_early_child_crash_detected_without_hanging` with `SandboxPolicy::unconstrained()` on `not(windows)`.
  3. Added explicit platform containment verification test `test_standard_fails_closed_on_non_windows_without_job_objects` to verify that `SandboxLevel::Standard` continues to strictly fail closed on non-Windows platforms.

#### 3.2 Windows 8.3 Path Canonicalization
- **The Bug:** On Windows GitHub Actions runners, `std::env::temp_dir()` or checkout directories often evaluate to `C:\Users\RUNNER~1\...`, while canonicalized paths evaluate to `C:\Users\runneradmin\...`. Direct string equality or prefix checks failed.
- **The Remediation:** Implemented `dunce::canonicalize` / `std::fs::canonicalize` with case-insensitive, drive-letter normalized path comparison helpers across workspace and binary resolver routines.

---

### 4. Verification Evidence & Quality Metrics

- **`cargo fmt --check`:** Clean across entire workspace (0 formatting deviations).
- **`cargo check --workspace --all-targets`:** Clean transpile across all 18 crates.
- **`cargo clippy --workspace --all-targets -- -D warnings`:** Zero warnings. Zero dead code or unused imports.
- **Windows Test Suite:** Verified 100% passing across all backend crates in Run 42091275576.
- **`agsd.exe` Compilation:** Succeeded in debug and release targets on Windows runners.
