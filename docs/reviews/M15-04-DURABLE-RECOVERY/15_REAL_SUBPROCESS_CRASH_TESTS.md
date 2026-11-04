# M15-04.1 Subprocess Crash & Restart Recovery Test Strategy

## 1. Test Methodology

To eliminate the risk of confounding in-memory mock reloads with authentic OS crash survival, M15-04.1 implements two distinct test tiers:

1. **`IN_PROCESS_DURABLE_TESTS` (`m15_04_durable_recovery_tests.rs`):**
   - Direct verification of `SqliteEventStore`, WAL transaction atomicity, sequence monotonic enforcement, corruption detection (`PRAGMA quick_check;`), and state reconstruction in memory.
2. **`REAL_SUBPROCESS_CRASH_TESTS` (`m15_04_process_recovery_tests.rs`):**
   - Direct invocation of the compiled `agsd` executable as an independent OS subprocess.
   - Communicates exclusively over authentic OS IPC channels:
     - **Windows:** Named Pipes (`\\.\pipe\agent-studios-test-<uuid>`)
     - **Linux:** Unix Domain Sockets (`/tmp/agent-studios-test-<uuid>.sock`)
   - Uses completely disposable temporary directories for state, database, and bootstrap credentials.
   - Tests abnormal process termination (SIGKILL on Unix / `taskkill /F` or `TerminateProcess` on Windows).

---

## 2. Tested Subprocess Scenarios

1. **Clean Restart & Identity Continuity:**
   - Subprocess 1 starts, writes bootstrap credentials, binds IPC pipe.
   - Client authenticates, creates a studio, and submits an initial task.
   - Subprocess 1 is cleanly stopped via `IpcCommand::Shutdown`.
   - Subprocess 2 starts targeting the identical database.
   - Client queries task graph and verifies exact task ID, studio ID, and sequence number continuity.
2. **Single-Writer Lock Rejection:**
   - While Subprocess 1 is running, Subprocess 2 is launched with `--db_path` pointing to Subprocess 1's active database.
   - Subprocess 2 immediately exits with a non-zero exit code due to `DatabaseLock` acquisition failure.
   - Subprocess 1 continues operating unaffected.
3. **Abrupt Subprocess Crash & In-Flight Reconciliation:**
   - Subprocess 1 runs a task.
   - Test harness sends SIGKILL (or Windows process kill) to Subprocess 1 while task is in-flight.
   - Subprocess 2 starts on the same database.
   - Recovery status verifies:
     - `interrupted_tasks_count >= 1`
     - Interrupted task state is transitioned to `TaskState::Failed` with domain reason `"daemon_interrupted"`.
     - In-flight runs transitioned to `RunState::Failed`.
     - No automatic task resumption or unauthorized model calls.
4. **Least-Privilege Diagnostics Authorization:**
   - Standard client receives `RecoveryStatusReport` with `database_path: None`.
   - Admin client receives `RecoveryStatusReport` with `database_path: Some(...)`.
5. **Startup Failure Cleanup:**
   - Subprocess started with an intentionally corrupted database file.
   - Subprocess fails preflight check and exits.
   - Test harness confirms NO bootstrap credentials files (`agsd.json`, `agsd-admin.json`) were left on disk.
