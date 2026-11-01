# M15 UI-02.2 — Real Daemon Execution Smoke Test Evidence

## Metadata
- **Milestone**: M15 UI-02.2
- **Topic**: Real Rust Daemon (`agsd`) Execution Smoke Test
- **Target Branch**: `feat/m15-real-agent-execution-ui` (Draft PR #27)
- **Base Branch**: `feat/m14-mission-control-connected-integration`
- **Pinned Backend Commit**: `61cb7e88e4ae0e3a0432134b77eee0c30f56c302` (PR #28 / PR #26)
- **Date**: 2026-11-02

---

## 1. Scope & Constraints

This test validates real end-to-end task submission and lifecycle event processing using the compiled Rust daemon binary (`agsd.exe` on Windows / `agsd` on Linux) built from pinned commit `61cb7e88e4ae0e3a0432134b77eee0c30f56c302`.

### Strict Operational Constraints
- **Zero Billable AI Provider Calls**: All tests run against deterministic mock/fake runtimes or internal daemon state machine transitions.
- **Truthful Labeling**: Internal daemon execution with fixture runtimes is strictly labeled as simulated/fixture execution, never as live external frontier LLM code generation.
- **Isolated Workspace**: Tests execute within temporary isolated Git repositories. Original source checkouts are untouched.

---

## 2. Real Daemon Execution Verification Test Matrix

The smoke test suite `src/test/e2e/RealExecutionSmoke.test.ts` exercises:

1. **Daemon Spawning & IPC Bootstrap**:
   - Launches genuine compiled `agsd` binary with `--auto-execute` and `--pipe-name`/`--uds-path`.
   - Generates bootstrap JSON file containing auth token and IPC endpoint.
   - Connects `NativeIpcTransport` and `DaemonConnectedBridge` over genuine OS pipe / socket.

2. **Authoritative Studio & Workspace Provisioning**:
   - Connects with admin auth token.
   - Executes `IpcCommand::CreateStudio` with isolated temporary Git workspace.
   - Receives genuine Rust-assigned studio UUID.

3. **Authoritative Execution Readiness Query**:
   - Dispatches `IpcCommand::GetExecutionReadiness`.
   - Verifies daemon status `is_ready: true`.
   - Inspects discovered runtime instances, model catalog, and tri-state capabilities.

4. **Operator Confirmation & Challenge Generation**:
   - Exercises native operator confirmation dialog handler.
   - Verifies generation of HMAC-SHA256 challenge bound to studio UUID, prompt hash, workspace path, model, provider, runtime instance, and daemon `sessionId`.

5. **Authoritative Task Submission (`IpcCommand::SubmitTask`)**:
   - Dispatches `RustBatchTaskSpec` containing `TaskExecutionSpec`.
   - Validates length-prefixed framing and wire serialization under 4 MiB.
   - Receives genuine Rust task UUID from `IpcResponse::TasksSubmitted`.

6. **Lifecycle Event Streaming & Store Synchronization**:
   - Subscribes to real daemon control plane event stream.
   - Captures lifecycle events:
     - `task_submitted`
     - `task_started`
     - `run_started`
     - `task_state_transitioned`
   - Validates that `TaskExecutionStore` and `MissionControlProjection` track state transitions accurately.

7. **Workspace Integrity & Git Isolation**:
   - Verifies temporary repository isolation.
   - Ensures no unexpected modifications occur outside authorized paths.
   - Verifies that task cancellation (`cancel_task`) gracefully transitions tasks to terminal state without data loss.
