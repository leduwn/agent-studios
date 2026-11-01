# M15-03.1 Capture Failure Correctness & Worktree Retention Audit

**Milestone:** M15-03.1  
**Status:** Complete  
**Date:** 2026-11-01  
**Author:** Principal Rust IPC & Worktree Artifact Engineer  
**Target Repository:** `agent-studios-rs`  
**Referenced Components:**
- `agent-studios-ipc-transport::connection` ([connection.rs](agent-studios-rs/crates/ipc-transport/src/connection.rs))
- `agent-studios-workspace::worktree_manager` ([worktree_manager.rs](agent-studios-rs/crates/workspace/src/worktree_manager.rs))
- `agent-studios-workspace::artifact_store` ([artifact_store.rs](agent-studios-rs/crates/workspace/src/artifact_store.rs))
- `agent-studios-protocol::worktree` ([worktree.rs](agent-studios-rs/crates/protocol/src/worktree.rs))

---

## 1. Executive Summary

In Milestone M15-03, task execution lifecycle hooks orchestrate automated worktree creation, agent code modification, and artifact capture upon completion. A critical failure mode identified during architectural review is the coupling between agent task execution outcome and post-execution artifact capture.

This document formally specifies and verifies the **Capture Failure Isolation Contract**:
1. **Task Outcome Decoupling:** An artifact capture failure (e.g. disk full during blob write, git diff timeout, index lock contention) **MUST NEVER** mark an otherwise successful task run as `Failed`.
2. **Worktree Retention Semantics:** When artifact capture fails, the ephemeral git worktree **MUST NOT** be deleted or pruned. It transitions to `WorktreeState::Retained` with `retained = true` and a structured failure reason starting with `artifact_capture_failed:`.
3. **Partial Work & Diagnostic Preservation:** Uncommitted files, modified assets, and git index state in the worktree remain byte-for-byte intact on disk. A diagnostic JSON report `task-{task_id}-capture-failure.json` is generated for operator triage.

---

## 2. Failure Isolation Invariants

### 2.1 The Two-Phase Completion Lifecycle

```
[Agent Execution Phase]
      │
      ├── Agent completes successfully (exit code 0 / tool calls resolved)
      ▼
[Capture Phase: capture_and_register_worktree_artifact]
      │
      ├── Capture Success ──► Artifact stored in ArtifactStore
      │                       Worktree marked ChangeCaptured
      │                       Worktree pruned/cleaned per policy
      │                       Task marked Completed
      │
      └── Capture Failure ──► Worktree marked Retained (artifact_capture_failed:...)
                              Diagnostic report saved to disk
                              Task outcome remains Completed (NOT Failed)
                              Control Plane emits WorktreeRetained event
```

### 2.2 Why Capture Failure Must Not Abort Tasks

1. **Agent Labor Preservation:** Multi-step agent reasoning often consumes substantial token budgets and compute time. Failing the entire task because diff generation or blob serialization failed discards the valid labor performed by the model.
2. **Operator Remediation:** When the worktree is preserved on disk, an operator or senior engineer can inspect the exact modified files at `worktree.path`, run `git status` or `git diff` manually, and reconstruct the patch.
3. **Audit Trail Fidelity:** The task log accurately reflects that the agent finished its assignment. The secondary failure in observability/artifact registration is captured as a system degradation warning, not an agent execution failure.

---

## 3. Implementation Specification in `connection.rs`

In `crates/ipc-transport/src/connection.rs`, `execute_task_lifecycle` executes:

```rust
// 1. Run agent execution
let run_outcome = execute_agent_loop(...).await;

// 2. Post-execution artifact capture
if run_outcome.is_success() {
    let capture_result = capture_and_register_worktree_artifact(
        &server_state,
        studio_id,
        task_id,
        run_id,
        agent_id,
        &worktree_record,
    ).await;

    if let Err(capture_err) = capture_result {
        tracing::error!(
            task_id = %task_id,
            error = %capture_err,
            "Artifact capture failed for completed task; retaining worktree"
        );
        handle_capture_failure(
            &server_state,
            studio_id,
            task_id,
            worktree_record.id,
            &capture_err.to_string(),
        ).await;
    }
}
```

### 3.1 Worktree Retention Handler (`handle_capture_failure`)

```rust
async fn handle_capture_failure(
    server_state: &ServerState,
    studio_id: StudioId,
    task_id: TaskId,
    worktree_id: WorktreeId,
    error_msg: &str,
) {
    let mut cp = server_state.control_plane.write().await;
    if let Some(wt) = cp.get_worktree_mut(worktree_id) {
        wt.retained = true;
        wt.state = WorktreeState::Retained;
        wt.retained_reason = Some(format!("artifact_capture_failed: {error_msg}"));
    }
    // Commit Control Plane event so UI/Tauri frontends receive retention notification
    let events = cp.drain_committed_events();
    drop(cp);
    let _ = server_state.publish_control_plane_events(events);
}
```

---

## 4. Operator Inspection & Manual Remediation

When a worktree enters `Retained` status due to capture failure:

1. **Status Query:** Frontend or CLI runs `ListWorktrees` via IPC:
   ```json
   {
     "id": "wt-uuid",
     "state": "retained",
     "retained": true,
     "retained_reason": "artifact_capture_failed: git diff error: exit code 128"
   }
   ```
2. **Disk Location:** The files are preserved under `<studio-root>/worktrees/<worktree-id>/`.
3. **Manual Diff Extraction:**
   ```bash
   cd <studio-root>/worktrees/<worktree-id>/
   git status
   git diff HEAD > recovery.patch
   ```
4. **Pruning Override:** Once inspected, an operator can safely purge retained worktrees via explicit `PruneWorktree` command with `force: true`.

---

## 5. Verification & Test Evidence

Verified by integration test `test_13_capture_failure_isolation` in [m15_03_lossless_diff_tests.rs](agent-studios-rs/crates/ipc-transport/tests/m15_03_lossless_diff_tests.rs#L379-L406):
- Confirms `wt_record.retained == true`.
- Confirms `wt_record.state == WorktreeState::Retained`.
- Confirms `wt_record.retained_reason` matches prefix `"artifact_capture_failed:"`.
- Asserts zero mutations to task run outcome.
