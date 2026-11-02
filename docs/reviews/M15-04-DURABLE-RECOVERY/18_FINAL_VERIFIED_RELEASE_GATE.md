# M15-04.1 Final Verified Release Gate & Audit Sign-Off

## 1. Release Invariants & Gates

| Gate Item | Verification Standard | Target | Status |
| :--- | :--- | :--- | :--- |
| **G1: Clippy Cleanliness** | `cargo clippy --all-targets -- -D warnings` | Zero warnings across all crates | Verified |
| **G2: Formatting** | `cargo fmt --check` | 100% rustfmt compliant | Verified |
| **G3: Cross-Process Lock** | OS kernel level single-writer ownership | Second daemon rejected, no WAL corruption | Verified |
| **G4: Durability Configuration** | `DurabilityMode::Full` vs `Normal` | Fsync barrier acknowledged before commit | Verified |
| **G5: Checked Arithmetic** | `u16::try_from`, `u64::try_from` | Zero unchecked integer casts on row parsing | Verified |
| **G6: Least-Privilege IPC** | `GetRecoveryStatus` role redaction | Non-admin receives `database_path: None` | Verified |
| **G7: Startup Failure Safety** | Stale bootstrap credentials prevented | Bootstrap files emitted only after successful DB open | Verified |
| **G8: Subprocess Crash Tests** | Real `agsd` process termination & restart | In-flight tasks/runs marked `Failed` ("daemon_interrupted") | Verified |
| **G9: Worktree & Artifact Layout** | Persistent state directory | Centralized under `%APPDATA%` / `~/.local/share` | Verified |
| **G10: CI Parity** | Ubuntu & Windows remote workflows | Green build on all targets | Verified |

---

## 2. Cross-Process vs In-Process Test Separation

- **In-Process Tests:** `m15_04_durable_recovery_tests.rs` (5 tests):
  1. `test_sqlite_wal_persistence_survives_daemon_restart`
  2. `test_crash_recovery_transitions_in_flight_tasks_and_runs`
  3. `test_sqlite_preflight_corruption_detection_and_rejection`
  4. `test_sqlite_batch_atomicity_and_gap_rejection`
  5. `test_worktree_startup_reconciliation_missing_dir`
- **Subprocess Integration Tests:** `m15_04_process_recovery_tests.rs`:
  1. `test_subprocess_clean_restart_and_identity_preservation`
  2. `test_subprocess_single_writer_lock_rejection`
  3. `test_subprocess_kill_and_crash_recovery_reconciliation`
  4. `test_subprocess_role_based_recovery_status_redaction`
  5. `test_subprocess_startup_failure_cleans_bootstrap_files`

---

## 3. Production Verdict

- **Milestone:** M15-04.1
- **Verdict:** APPROVED & VERIFIED
- **Target Branch:** `feat/m15-04-durable-state-recovery`
- **PR:** #30
