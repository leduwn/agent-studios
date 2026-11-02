# M15 UI-04 — Frontend Authentic Daemon Crash Recovery Test Strategy

## 1. Test Methodology: Real Subprocess Execution

To avoid false confidence from mock bridges, the frontend test suite executes the compiled `agsd` binary directly via Node.js `child_process.spawn()` over real OS IPC:
- **Windows**: `\\.\pipe\agent-studios-e2e-recovery-<uuid>`
- **Unix**: `/tmp/agent-studios-e2e-recovery-<uuid>.sock`

---

## 2. Test Scenarios Covered

1. **Scenario 1: Clean Daemon Restart & State Persistence**:
   - Subprocess A spawns with dedicated temporary SQLite DB.
   - Frontend connects via `DaemonConnectedBridge`, creates studio, submits task.
   - Frontend triggers authorized admin shutdown.
   - Subprocess B spawns targeting the identical SQLite DB.
   - Frontend reconnects and verifies task ID, studio ID, and sequence number continuity.

2. **Scenario 2: Abnormal Termination (SIGKILL / Forced Process Kill)**:
   - Subprocess A spawns.
   - Frontend submits task.
   - Test harness abruptly kills Subprocess A via `child.kill('SIGKILL')` or Windows `taskkill /F`.
   - Subprocess B spawns on the same database.
   - Frontend calls `getRecoveryStatus()`:
     - `interrupted_tasks_count >= 1`
     - `database_healthy === true`
   - Frontend queries `getTask()`: task state is `failed`, not `running`.

3. **Scenario 3: Role-Based Diagnostic Redaction**:
   - Standard client connection receives `recoveryStatus.database_path === null` or `undefined`.
   - Admin client connection receives `recoveryStatus.database_path !== null`.

4. **Scenario 4: Watermark Resynchronization & Replay Monotonicity**:
   - Daemon produces event stream with sequence $1 \dots N$.
   - Client disconnects at watermark $K$.
   - Client reconnects requesting `subscribe_events` with `from_sequence: K + 1`.
   - Client verifies gapless, duplicate-free event backfill.

5. **Scenario 5: Artifact Retrieval Across Daemon Restarts**:
   - Artifact captured in Subprocess A is retrieved with SHA-256 integrity verification in Subprocess B.
