# M15 UI-06 — Report 07: Real Daemon Integration Acceptance

## Metadata
- **Milestone**: M15 UI-06: Agent-First Experience, Shared Session Continuity & Real Daemon Integration Acceptance
- **Worktree**: `D:\Projects\agent-studios-active\m15-ui-06`
- **Target Binary**: Compiled native Rust `agsd.exe` (`D:\Projects\agent-studios\agent-studios-rs\target\debug\agsd.exe`)
- **Test Suite**: `src/test/e2e/RealAgentIdeContinuityE2E.test.ts`
- **Date**: 2026-11-05
- **Status**: **VERIFIED & ACCEPTED (6/6 PASSING)**

---

## 1. Acceptance Overview

This report documents real end-to-end integration acceptance of the VS Code extension against the compiled native Rust `agsd` daemon binary. Tests run against a live daemon spawned in an isolated temporary directory with a dedicated SQLite database and Windows Named Pipe (`\\.\pipe\ags-continuity-e2e-<uuid>`).

---

## 2. Test Execution & Evidence Log

### Test 1: Authenticates standard operator role via two-phase HMAC challenge
- **Protocol**: Two-phase handshake (capabilities negotiation followed by HMAC-SHA256 challenge with nonce, session ID, and timestamp).
- **Execution**: Connects `DaemonConnectedBridge` using `agsd.json`. Sends `get_health`.
- **Evidence**:
  ```
  Health status: healthy
  Active clients: 1
  Uptime: > 0s
  Studio created: "M15-UI-06 Continuity Studio"
  ```
- **Verdict**: **PASSED**

### Test 2: Hydrates live native daemon session into SessionStateProjection
- **Execution**: Binds `BridgeManager` to connected bridge and calls `initializeFromBridge()`.
- **Evidence**:
  - `stateProjection.session` contains hydrated session object with ID matching created studio.
  - Initial session status evaluates to `idle`.
- **Verdict**: **PASSED**

### Test 3: Mode transition (openInIde & switchToAgentMode) preserves session identity without restarting daemon
- **Execution**:
  1. Captures pre-transition PID (`daemonProcess.pid`) and `sessionId`.
  2. Executes `agent-studios.openInIde` handler → asserts `workbench.view.explorer` executed.
  3. Executes `agent-studios.switchToAgentMode` handler → asserts `workbench.view.extension.agent-studios-activitybar` and `agent-studios.chat.focus` executed.
  4. Verifies PID unchanged (`assert.strictEqual(daemonProcess.pid, initialPid)`).
  5. Verifies `session_id` unchanged (`assert.strictEqual(postSessionId, preSessionId)`).
  6. Verifies zero task submissions (`postTasks.length === initialTasks.length`).
- **Verdict**: **PASSED**

### Test 4: Reconnection preserves session state from native SQLite WAL store
- **Execution**:
  1. Captures initial session ID.
  2. Disconnects bridge transport (`bridge.disconnect()`).
  3. Reconnects bridge transport (`bridge.connect()`).
  4. Re-queries current session from daemon.
- **Evidence**:
  - Transport reconnects cleanly.
  - Session ID returned from SQLite WAL store matches pre-disconnect session ID.
- **Verdict**: **PASSED**

### Test 5: Role separation: standard operator client cannot execute shutdown
- **Execution**: Standard client sends `{ type: 'shutdown' }` over IPC.
- **Evidence**:
  - Daemon rejects command with `permission denied` / `unauthorized`.
  - Daemon child process remains running (`exitCode === null`).
- **Verdict**: **PASSED**

### Test 6: Authorized admin role executes clean shutdown terminating native daemon
- **Execution**: Connects `NativeIpcTransport` with credentials from `agsd-admin.json` (`admin_key`, key_id `ags-client-admin`). Sends `{ type: 'shutdown' }`.
- **Evidence**:
  - Daemon responds with `{ type: 'shutdown_acknowledged' }`.
  - Child process exits with return code `0`.
  - Bootstrap files (`agsd.json`, `agsd-admin.json`) cleaned up.
- **Verdict**: **PASSED**

---

## 3. Summary Acceptance Verdict

| Acceptance Criterion | Result | Status |
|---|---|---|
| Native Rust `agsd.exe` Lifecycle | Managed cleanly via spawn and admin shutdown | **ACCEPTED** |
| HMAC Authentication & Role Separation | Enforced (operator denied shutdown, admin accepted) | **ACCEPTED** |
| Session Identity Preservation | Preserved across mode transitions and transport disconnects | **ACCEPTED** |
| Zero Daemon Restarts Invariant | Verified via PID invariance | **ACCEPTED** |
| Zero Task Duplication Invariant | Verified via task count invariance | **ACCEPTED** |
| SQLite WAL Persistence | Verified across reconnect | **ACCEPTED** |
