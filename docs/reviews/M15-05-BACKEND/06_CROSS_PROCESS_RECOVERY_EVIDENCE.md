# 06 — Cross-Process Recovery & Process Boundary Evidence

## 1. Overview of Cross-Process Isolation Verification

To guarantee that daemon recovery is resilient against real operating-system process terminations rather than merely in-memory Tokio task aborts, M15-04 and M15-05 maintain a suite of authentic cross-process tests in `crates/ipc-transport/tests/m15_04_process_recovery_tests.rs`.

These tests compile the real `agsd` binary and spawn it as an isolated child process via `std::process::Command`, verifying:
1. **Single-Writer Lock Rejection**: Mutual exclusion across concurrent processes.
2. **Crash & Kill Recovery Reconciliation**: SIGKILL / `process.kill()` handling and orphaned state recovery.
3. **Clean Restart & Identity Preservation**: Session re-authentication and persistent metadata continuity.
4. **Bootstrap File Cleanup on Failure**: Ephemeral socket and bootstrap file hygiene.
5. **Role-Based Recovery Status Redaction**: Non-exposure of host paths to standard clients.

---

## 2. Test Evidence from `m15_04_process_recovery_tests.rs`

### Test 1: `test_subprocess_single_writer_lock_rejection`
- **Mechanism**:
  1. Process A spawns with a target state directory and acquires the OS file lock (`agsd.lock`).
  2. Process B attempts to start pointing at the same state directory.
  3. Process B fails immediately with exit code 1 and emits an error message indicating that another daemon instance holds the lock.
  4. Process A is terminated; the lock is automatically released by the OS kernel.
  5. Process C spawns pointing to the directory and successfully acquires the lock.
- **Result**: **PASS** (Zero file corruption, zero split-brain).

### Test 2: `test_subprocess_kill_and_crash_recovery_reconciliation`
- **Mechanism**:
  1. Daemon 1 spawns, creates a studio, and registers tasks and runs.
  2. While tasks are in `Running` state, Daemon 1 is killed abruptly via `child.kill()` (simulating an uncatchable crash or SIGKILL).
  3. Daemon 2 is spawned against the persisted SQLite database.
  4. Daemon 2 runs startup reconciliation:
     - All in-flight `Running` tasks are transitioned to `Failed` with reason `"daemon_crash_reconciliation"`.
     - In-flight runs are marked `Failed`.
     - Missing or unmanaged worktree paths are flagged `Failed` without auto-deleting uncommitted files.
  5. An authenticated client connects to Daemon 2 and validates that the control plane state is consistent and stable.
- **Result**: **PASS** (Zero orphaned in-flight tasks, zero data corruption).

### Test 3: `test_subprocess_clean_restart_and_identity_preservation`
- **Mechanism**:
  1. Daemon 1 gracefully exits via shutdown command.
  2. Daemon 2 boots from the identical database.
  3. Client connects, authenticates with HMAC, queries recovery report, and verifies that `total_persisted_events` matches the committed event count.
- **Result**: **PASS**.

### Test 4: `test_subprocess_startup_failure_cleans_bootstrap_files`
- **Mechanism**:
  1. Daemon is invoked with invalid parameters (e.g., unwritable database directory).
  2. Daemon terminates with non-zero exit code.
  3. Verified that no dangling bootstrap files (`agsd-bootstrap.json`, socket locks) are left on disk.
- **Result**: **PASS**.

### Test 5: `test_subprocess_role_based_recovery_status_redaction`
- **Mechanism**:
  1. Daemon starts with valid SQLite database.
  2. Standard client connects and authenticates with `Role::Standard`.
  3. Admin client connects and authenticates with `Role::Admin`.
  4. Querying `GetRecoveryStatus`:
     - Standard client receives `database_path: None` (path redacted to prevent host filesystem reconnaissance).
     - Admin client receives `database_path: Some(".../agsd.db")`.
- **Result**: **PASS**.

---

## 3. Test Execution Output Summary

```
Running tests\m15_04_process_recovery_tests.rs
test test_subprocess_startup_failure_cleans_bootstrap_files ... ok
test test_subprocess_role_based_recovery_status_redaction ... ok
test test_subprocess_kill_and_crash_recovery_reconciliation ... ok
test test_subprocess_single_writer_lock_rejection ... ok
test test_subprocess_clean_restart_and_identity_preservation ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; finished in 4.85s
```
