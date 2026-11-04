# M15-04 Failure Modes & Adversarial Verification

## 1. Adversarial Testing Scenarios

We analyzed and tested key edge cases and adversarial scenarios:

### 1.1 Database File Corruption
- **Scenario:** The database file is truncated, contains random noise, or has a broken B-Tree.
- **Handling:** `PRAGMA quick_check;` is executed on initialization. Any non-`ok` response or SQLite read error causes `SqliteEventStore::open` to fail fast with `StoreError::StorageFailure`.
- **Result:** Daemon does NOT start with corrupted state; reports clear diagnostic message without silent in-memory fallback.

### 1.2 Sequence Tampering & Out-of-Order Batches
- **Scenario:** A batch attempts to inject sequence gaps (e.g. sequence 6 when latest is 4) or sequence regressions (e.g. sequence 2 when latest is 4).
- **Handling:** `append_batch` verifies monotonic continuity inside an immediate transaction. Gaps trigger `StoreError::SequenceGap`; regressions trigger `StoreError::SequenceRegression`.
- **Result:** Entire transaction is rolled back; latest sequence remains unchanged.

### 1.3 Duplicate Event IDs
- **Scenario:** Duplicate UUIDs supplied within the same batch or matching an existing committed event.
- **Handling:** Checked in-memory via `HashSet` and on-disk via `SELECT 1 FROM events WHERE event_id = ?`.
- **Result:** Fails with `StoreError::DuplicateEventId`; transaction rolls back.

### 1.4 Power Cut During Active Execution
- **Scenario:** Daemon terminates abruptly while an agent is modifying files or running commands.
- **Handling:** On restart, `reconcile_interrupted_executions` finds runs in `Starting` or `Running` and tasks in `Running`.
- **Result:** Tasks/runs are safely marked `Failed` with `"daemon_interrupted"`; approvals are cancelled; partial changes in worktree are preserved.
