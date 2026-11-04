# M15-04 Test Strategy & Coverage Analysis

## 1. Test Architecture

The test suite exercises all layers of the persistence and recovery architecture across multiple levels:
1. **Unit & In-Memory Tests:**
   - Schema validation, duplicate ID rejection, sequence gap and regression checks in `crates/control-plane/tests/integration_tests.rs`.
2. **Durable SQLite Engine Tests:**
   - File-backed database creation, WAL mode configuration, `PRAGMA quick_check` verification, and multi-event transactional commits.
3. **Cross-Restart Simulation Tests:**
   - End-to-end multi-process daemon lifecycle simulations in `crates/ipc-transport/tests/m15_04_durable_recovery_tests.rs`.

---

## 2. Integration Test Matrix

| Test Name | File | Description | Status |
|---|---|---|---|
| `test_sqlite_wal_persistence_survives_daemon_restart` | `m15_04_durable_recovery_tests.rs` | Server 1 commits Studio/Task to SQLite, shuts down, Server 2 restarts from file, verifies entities and continues mutations | PASS |
| `test_crash_recovery_transitions_in_flight_tasks_and_runs` | `m15_04_durable_recovery_tests.rs` | Seeds in-flight Running Task, Running Run, and Pending Approval; restarts daemon; verifies auto-transitions to Failed/Cancelled with `daemon_interrupted` | PASS |
| `test_sqlite_preflight_corruption_detection_and_rejection` | `m15_04_durable_recovery_tests.rs` | Corrupts database bytes on disk; verifies fail-fast error on open without panicking | PASS |
| `test_sqlite_batch_atomicity_and_gap_rejection` | `m15_04_durable_recovery_tests.rs` | Verifies atomic rollback on sequence gaps, sequence regressions, and intra-batch duplicate EventIds | PASS |
| `test_worktree_startup_reconciliation_missing_dir` | `m15_04_durable_recovery_tests.rs` | Reconciles active worktree whose directory was deleted; verifies transition to WorktreeState::Failed | PASS |
| `test_control_plane_sqlite_restore_from_store_and_resume_mutations` | `control-plane/tests/integration_tests.rs` | Verifies non-duplicating replay and resumption of state mutations on `ControlPlane` | PASS |
| `test_control_plane_reconcile_interrupted_executions` | `control-plane/tests/integration_tests.rs` | Verifies domain engine state transitions for interrupted runs and tasks | PASS |

---

## 3. Distinction: In-Process vs Genuine Restart Tests

All test cases in `m15_04_durable_recovery_tests.rs` simulate genuine restart lifecycles:
- `Server 1` and all client streams are explicitly dropped and closed, releasing locks.
- The SQLite file on disk is re-opened by an independent `Server 2` instance.
- Verified that no shared memory handles or static variables leak state between phases.
