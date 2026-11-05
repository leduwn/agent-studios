# M15-04.1 IPC Authorization & Startup Failure Hardening

## 1. Least-Privilege Recovery Status Diagnostics

### Defect
Previously, `IpcCommand::GetRecoveryStatus` returned the full `RecoveryStatusReport` containing the server's absolute host database path (`database_path: Some(String)`) to any authenticated client, regardless of whether the client had standard or administrative privileges.

### M15-04.1 Role Redaction
In `crates/ipc-transport/src/connection.rs`, responses to `GetRecoveryStatus` are filtered by client role:
- **`ClientRole::Admin`:** Receives the full `database_path` for administrative diagnostics and debugging.
- **`ClientRole::Standard`:** Has `database_path` set to `None`. Safe operational metrics (status, health, task/run counts, total events) remain accessible.
- **Unauthenticated:** Request is rejected at the framing layer with `Unauthorized`.

This preserves wire compatibility while adhering to least-privilege security.

---

## 2. Startup Failure Safety & Stale Credential Prevention

### Defect
In earlier revisions, the daemon generated authentication tokens and wrote runtime bootstrap files (`agsd.json` and `agsd-admin.json`) before attempting to open the SQLite database, acquire the lock, or run migrations. If the database was corrupted or locked by another process, the daemon aborted, leaving valid-looking bootstrap files on disk that pointed to a dead endpoint.

### M15-04.1 Ordered Startup Lifecycle
The startup order in `run_daemon` has been strictly sequenced:

1. **State Directory Preparation:** Ensure state directory exists and permissions are secure (`0700` on Unix).
2. **Process Lock Acquisition:** Acquire `DatabaseLock` on `<db_path>.lock`. If another process holds the lock, fail immediately before creating any bootstrap records.
3. **Durable Store Initialization:** Open SQLite store with WAL mode and synchronous verification (`PRAGMA quick_check;`).
4. **Schema Migrations:** Apply pending schema versions within immediate transactions.
5. **State Replay:** Restore `ControlPlaneState` from durable event store via `restore_from_store`.
6. **Recovery Reconciliation:** Reconcile interrupted tasks/runs and missing worktrees.
7. **Bootstrap File Emission:** ONLY after all persistence and state restoration checks succeed, write `agsd.json` and `agsd-admin.json` with restricted permissions (`0600`).
8. **Failure Cleanup:** If any failure occurs during binding or socket initialization, a cleanup guard removes any newly created bootstrap files so clients are never misled.
