# M13 IPC-02.2.1 Final Handoff Notice

**Date**: 2026-10-30  
**Milestone**: M13 (IPC-02.2.1 Cross-Platform CI & Security Correctness)  
**Target Branch**: `feat/m13-ipc-02-control-plane` (Pull Request #19)  
**Pull Request Status**: DRAFT (Strict NO SELF-MERGE policy enforced)  

---

## 1. Summary of Changes

This release delivers the complete remediation of cross-platform build blockers, lock starvation issues, and security authorization gaps identified in IPC-02.2.

### Modified Files:
1. `crates/opencode-runtime/src/lib.rs`:
   - Gated re-export of Win32 `JobObjectGuard` under `#[cfg(windows)]`.
2. `crates/opencode-runtime/tests/m11_sandbox_containment_tests.rs`:
   - Gated Windows-specific Job Object containment tests under `#[cfg(windows)]`.
3. `crates/opencode-runtime/tests/sandbox_tests.rs`:
   - Gated Windows-specific Job Object guard lifecycle tests under `#[cfg(windows)]`.
4. `crates/opencode-runtime/src/discovery.rs`:
   - Added `AGENT_STUDIOS_OPENCODE_PATH` environment variable inspection and made `resolve_binary_path()` `pub(crate)`.
5. `crates/opencode-runtime/src/runtime.rs`:
   - Unified binary resolution using `crate::discovery::resolve_binary_path()`, guaranteeing path canonicalization and regular file validation across all environments.
6. `crates/ipc-transport/src/server.rs`:
   - Added `default_repo_path: Option<PathBuf>` to `ServerState` and `IpcServerBuilder` for deterministic workspace isolation.
7. `crates/ipc-transport/src/connection.rs`:
   - Reordered `CancelTask` execution to validate session origin, role, studio ownership, and non-terminal state before invoking `runtime.stop()`.
   - Implemented transactional two-phase commit (2PC) in `ResolveApproval`: decision is delivered to runtime before mutating Control Plane state.
   - Refactored `execute_task_lifecycle` to execute physical git worktree creation outside the Control Plane write lock with rollback on concurrent cancellation.
   - Guarded terminal states against late runtime `Completed`/`Failed` events.
8. `crates/ipc-transport/tests/control_plane_ipc_tests.rs`:
   - Integrated `IsolatedTestRepo` fixture into `test_ipc_task_auto_execution_lifecycle_succeeds_with_runtime`.
9. `crates/ipc-transport/tests/m13_ipc_02_2_integration_tests.rs`:
   - Integrated `IsolatedTestRepo` across all runtime execution tests.
   - Added 5 targeted security and correctness tests verifying wrong studio cancellation, cross-session cancellation prevention, runtime stop failure handling, late event immunity, and 2PC approval delivery failure handling.

---

## 2. Invariant Compliance Checklist

- [x] **Subtree Invariant**: Subtree `codex-rs/` is 100% untouched (0 diffs, 0 bytes modified).
- [x] **Branch Invariant**: All work committed directly to `feat/m13-ipc-02-control-plane`.
- [x] **No Concurrent PR Interference**: PRs #20, #21, #22, #23, and #24 untouched.
- [x] **No Self-Merge**: PR #19 remains in Draft status for human team review.
- [x] **No History Rewrites**: Zero force pushes or rebase manipulations.
- [x] **Zero Secret Leakage**: Zero raw credentials, bearer tokens, or HMAC secrets committed to git or printed to logs.
- [x] **Full Quality Gate**: `cargo fmt` clean, `cargo clippy` clean, all 104 tests passing across `ipc-transport` and `opencode-runtime`.

---

## 3. Reviewer Verification Steps

Reviewers can verify this work on Linux, macOS, or Windows by running:
```bash
# 1. Verify codex-rs has zero modifications
git diff --stat codex-rs/

# 2. Run formatting and clippy check
cargo fmt -p agent-studios-ipc-transport -p agent-studios-opencode-runtime -- --check
cargo clippy -p agent-studios-ipc-transport -p agent-studios-opencode-runtime --all-targets -- -D warnings

# 3. Execute test suites
cargo test -p agent-studios-ipc-transport
cargo test -p agent-studios-opencode-runtime
```
